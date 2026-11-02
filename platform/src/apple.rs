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
    fn hz_attach(view: *mut c_void, importer: bool);
    fn hz_mode(playing: bool);
    fn hz_poll(input: *mut Input) -> bool;
    fn hz_clipboard_get() -> *const c_char;
    fn hz_clipboard_set(text: *const c_char);
    fn hz_text() -> *const c_char;
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
    std::env::set_current_dir(&home)?;
    // Called before any worker threads exist, so changing the environment is safe.
    unsafe {
        std::env::set_var("HORIZON_HOME", &home);
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
    let mut event = Input::default();
    unsafe { hz_poll(&mut event) }.then_some(event)
}
pub fn text() -> String {
    string(unsafe { hz_text() })
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
