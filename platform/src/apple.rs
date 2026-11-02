use super::Input;
use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::PathBuf,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::{Window, WindowId},
};

unsafe extern "C" {
    fn hz_documents() -> *const c_char;
    fn hz_bundle() -> *const c_char;
    fn hz_setup_busy(busy: bool);
    fn hz_attach(view: *mut c_void, importer: bool);
    fn hz_mode(playing: bool);
    fn hz_poll(input: *mut Input) -> bool;
    fn hz_clipboard_get() -> *const c_char;
    fn hz_clipboard_set(text: *const c_char);
    fn hz_background(begin: bool);
}
fn string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // The native function retains the NSString until its next invocation.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}
pub fn documents() -> PathBuf {
    PathBuf::from(string(unsafe { hz_documents() }))
}
pub fn initialize() -> std::io::Result<()> {
    let home = documents();
    std::fs::create_dir_all(&home)?;
    let resources = home.join("resources");
    let previous = resources.with_extension("previous");
    if !resources.exists() && previous.exists() {
        std::fs::rename(previous, resources)?;
    }
    std::env::set_current_dir(&home)?;
    // Called before any worker threads exist, so changing the environment is safe.
    unsafe {
        std::env::set_var("HORIZON_HOME", &home);
    }
    let mut account = saved_account();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if account["expires_at"]
        .as_u64()
        .is_some_and(|expires| expires > now + 60)
    {
        account.as_object_mut().map(|v| v.remove("refresh"));
        let path = std::env::temp_dir().join("horizon-session.json");
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(account.to_string().as_bytes())?;
        unsafe {
            std::env::set_var("HORIZON_SESSION_FILE", path);
        }
    }
    // Conservative first-run defaults; later changes are owned by Horizon's settings UI.
    if !home.join("options.txt").exists() {
        std::fs::write(
            home.join("options.txt"),
            "renderDistance:4\nsimulationDistance:4\nmaxFps:60\nenableVsync:true\n",
        )?;
    }
    if !home.join("horizon-graphics.txt").exists() {
        std::fs::write(
            home.join("horizon-graphics.txt"),
            "resolutionScale:75\nworkers:2\ntextureBudgetMiB:256\nlodDistance:0\nhdr:off\n",
        )?;
    }
    Ok(())
}
pub fn attach(window: &Window, importer: bool) {
    if let Ok(handle) = window.window_handle()
        && let RawWindowHandle::UiKit(handle) = handle.as_raw()
    {
        // winit owns the view; native code only retains an overlay attached to it.
        unsafe {
            hz_attach(handle.ui_view.as_ptr(), importer);
        }
    }
}
pub fn playing(value: bool) {
    unsafe {
        hz_mode(value);
    }
}
pub fn poll() -> Option<Input> {
    tick_setup();
    let mut event = Input::default();
    unsafe { hz_poll(&mut event) }.then_some(event)
}
pub fn clipboard_get() -> String {
    string(unsafe { hz_clipboard_get() })
}
pub fn clipboard_set(text: &str) {
    if let Ok(text) = CString::new(text) {
        unsafe {
            hz_clipboard_set(text.as_ptr());
        }
    }
}
pub fn background(begin: bool) {
    unsafe {
        hz_background(begin);
    }
}

pub fn import_screen() -> Result<(), Box<dyn std::error::Error>> {
    struct Importer(Option<Window>);
    impl ApplicationHandler for Importer {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.0.is_none() {
                let window = event_loop
                    .create_window(Window::default_attributes())
                    .expect("iOS window");
                attach(&window, true);
                self.0 = Some(window);
            }
        }
        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            tick_setup();
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                std::time::Instant::now() + std::time::Duration::from_millis(100),
            ));
        }
    }
    EventLoop::new()?.run_app(&mut Importer(None))?;
    Ok(())
}
unsafe extern "C" {
    fn hz_active() -> bool;
}
pub fn active() -> bool {
    unsafe { hz_active() }
}

unsafe extern "C" {
    fn hz_setup_requested() -> bool;
    fn hz_setup_status(text: *const c_char);
    fn hz_setup_device(code: *const c_char, url: *const c_char);
    fn hz_secret_get() -> *const c_char;
    fn hz_secret_set(json: *const c_char) -> bool;
}
fn setup_status(text: &str) {
    if let Ok(text) = CString::new(text) {
        unsafe {
            hz_setup_status(text.as_ptr());
        }
    }
}
fn saved_account() -> serde_json::Value {
    serde_json::from_str(&string(unsafe { hz_secret_get() })).unwrap_or_default()
}
fn tick_setup() {
    use std::sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    };
    static BUSY: OnceLock<Arc<AtomicBool>> = OnceLock::new();
    let busy = BUSY.get_or_init(|| Arc::new(AtomicBool::new(false)));
    if !unsafe { hz_setup_requested() } || busy.swap(true, Ordering::Relaxed) {
        return;
    }
    let busy = busy.clone();
    let home = documents();
    let saved = saved_account();
    let kit = PathBuf::from(string(unsafe { hz_bundle() })).join("PrepareKit");
    unsafe {
        hz_setup_busy(true);
    }
    let spawned = std::thread::Builder::new()
        .name("Microsoft sign-in".into())
        .spawn(move || {
            let run = || -> Result<(), String> {
                let client = option_env!("HORIZON_MSA_CLIENT_ID").unwrap_or("");
                let installer = crate::setup::Installer::default();
                let cancelled = AtomicBool::new(false);
                let device = |code: &str, url: &str| {
                    if let (Ok(code), Ok(url)) = (CString::new(code), CString::new(url)) {
                        unsafe {
                            hz_setup_device(code.as_ptr(), url.as_ptr());
                        }
                    }
                };
                let result = installer.sign_in(
                    client,
                    saved["refresh"].as_str(),
                    &cancelled,
                    &setup_status,
                    &device,
                );
                let mut session = match result {
                    Err(_) if saved["refresh"].is_string() => {
                        installer.sign_in(client, None, &cancelled, &setup_status, &device)?
                    }
                    other => other?,
                };
                session.json["refresh"] = session.refresh.into();
                let text =
                    CString::new(session.json.to_string()).map_err(|_| "Invalid account data")?;
                if !unsafe { hz_secret_set(text.as_ptr()) } {
                    return Err("Could not save your account in Keychain.".into());
                }
                if home.join("resources/horizon-resources.json").is_file() {
                    setup_status("Signed in. Close and reopen Horizon to use this account.");
                    return Ok(());
                }
                installer.download_game(&home.join("minecraft"), &cancelled, &setup_status)?;
                let runtime = home.join(".horizon-java");
                if !horizon_ios_bootstrap::runtime_ready(&runtime) {
                    setup_status("Downloading Java class libraries for on-device preparation…");
                    let spec = horizon_ios_bootstrap::runtime_spec();
                    let archive = home.join(".horizon-java.tar.gz");
                    installer.download(
                        spec["url"].as_str().ok_or("Missing runtime URL")?,
                        spec["sha256"].as_str().ok_or("Missing runtime checksum")?,
                        spec["size"].as_u64().ok_or("Missing runtime size")?,
                        &archive,
                        &cancelled,
                    )?;
                    setup_status("Installing preparation class libraries…");
                    horizon_ios_bootstrap::install_runtime(&archive, &runtime)?;
                    std::fs::remove_file(archive).map_err(|e| e.to_string())?;
                }
                let writable_kit = home.join(".horizon-prepare");
                copy_kit(&kit, &writable_kit)?;
                let log = std::fs::File::create(home.join("preparation.log"))
                    .map_err(|e| e.to_string())?;
                setup_status("Preparing resources on this iPhone. Keep Horizon open…");
                horizon_ios_bootstrap::prepare(
                    &writable_kit,
                    &home.join("minecraft"),
                    &runtime,
                    &home.join("resources"),
                    &|p| {
                        setup_status(&format!(
                            "Preparing resources on this iPhone: {:.0}%",
                            p * 100.0
                        ))
                    },
                    &log,
                )?;
                setup_status(
                    "Setup complete. Close and reopen Horizon to play. No computer is needed.",
                );
                Ok(())
            };
            if let Err(error) = run() {
                setup_status(&error);
            }
            unsafe {
                hz_setup_busy(false);
            }
            busy.store(false, Ordering::Relaxed);
        });
    if spawned.is_err() {
        unsafe {
            hz_setup_busy(false);
        }
        BUSY.get().unwrap().store(false, Ordering::Relaxed);
        setup_status("Could not start sign-in. Restart Horizon and retry.");
    }
}

fn copy_kit(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            copy_kit(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
        } else {
            return Err("Invalid preparation kit entry".into());
        }
    }
    Ok(())
}
