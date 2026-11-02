//! Executes Horizon's real preparer inside the application, without desktop processes or JIT.
use horizon_prepare::java::InProcessJava;
use ristretto_vm::{ClassPath, ConfigurationBuilder, Reference, VM, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

struct Interpreter {
    home: PathBuf,
    cached: Mutex<Option<Cached>>,
}
struct Cached {
    runtime: tokio::runtime::Runtime,
    vm: Arc<VM>,
    key: Vec<String>,
    log: Arc<Mutex<fs::File>>,
}
struct Output(Arc<Mutex<fs::File>>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| std::io::Error::other("Preparation log lock failed"))?
            .write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0
            .lock()
            .map_err(|_| std::io::Error::other("Preparation log lock failed"))?
            .flush()
    }
}
async fn invoke(vm: &VM, main: &str, parameters: &[String]) -> ristretto_vm::Result<bool> {
    let mut strings = Vec::with_capacity(parameters.len());
    for parameter in parameters {
        strings.push(
            vm.try_invoke(
                "java/lang/String",
                "valueOf(Ljava/lang/Object;)Ljava/lang/String;",
                &[parameter.as_str()],
            )
            .await?,
        );
    }
    let class = vm.class("[Ljava/lang/String;").await?;
    let array = Value::new_object(
        vm.garbage_collector(),
        Reference::try_from((class, strings))?,
    );
    if let Err(failure) = vm
        .invoke(main, "main([Ljava/lang/String;)V", &[array])
        .await
    {
        if matches!(&failure, ristretto_vm::Error::InternalError(message) if message == "horizon.embedded.exit:0")
        {
            return Ok(true);
        }
        if let ristretto_vm::Error::Throwable(value) = &failure {
            if let Ok(object) = value.as_object_ref() {
                if object.class().name() == "java/lang/InternalError"
                    && object
                        .value("detailMessage")
                        .and_then(|v| v.as_string())
                        .ok()
                        .as_deref()
                        == Some("Internal error: horizon.embedded.exit:0")
                {
                    return Ok(true);
                }
            }
            let _ = vm
                .invoke(
                    "java/lang/Throwable",
                    "printStackTrace()V",
                    &[value.clone()],
                )
                .await;
        }
        return Err(failure);
    }
    Ok(false)
}
struct ReleaseVm;
impl Drop for ReleaseVm {
    fn drop(&mut self) {
        horizon_prepare::java::release_in_process();
    }
}
static DIRECTORY: Mutex<()> = Mutex::new(());
struct RestoreDirectory(PathBuf);
impl Drop for RestoreDirectory {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}
fn error(error: ristretto_vm::Error) -> String {
    if let ristretto_vm::Error::Throwable(value) = error {
        if let Ok(object) = value.as_object_ref() {
            let message = object
                .value("detailMessage")
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            return format!("{}: {message}", object.class().name());
        }
        "Java preparation failed".into()
    } else {
        error.to_string()
    }
}
impl InProcessJava for Interpreter {
    fn home(&self) -> PathBuf {
        self.home.clone()
    }
    fn major(&self) -> i64 {
        25
    }
    fn release(&self) {
        if let Ok(mut cached) = self.cached.lock() {
            *cached = None;
        }
    }
    fn run(&self, args: &[String], cwd: Option<&Path>, log: &fs::File) -> Result<(), String> {
        let _lock = DIRECTORY
            .lock()
            .map_err(|_| "Preparation directory lock failed")?;
        let _restore = RestoreDirectory(std::env::current_dir().map_err(|e| e.to_string())?);
        if let Some(cwd) = cwd {
            std::env::set_current_dir(cwd).map_err(|e| e.to_string())?;
        }
        let writer = Arc::new(Mutex::new(log.try_clone().map_err(|e| e.to_string())?));
        let output = Arc::new(tokio::sync::Mutex::new(Output(writer.clone())));
        let mut builder = ConfigurationBuilder::new()
            .interpreted(true)
            .java_home(self.home.clone())
            .stdout(output.clone())
            .stderr(output);
        let mut main = None;
        let mut jar = None;
        let mut parameters = Vec::new();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-cp" | "-classpath" | "--class-path" => {
                    let cp = iter.next().ok_or("Missing exporter class path")?;
                    let paths = resource_classpath(cp, &self.home)?;
                    builder = builder.class_path(ClassPath::from(&paths));
                }
                "-jar" => {
                    jar = Some(PathBuf::from(iter.next().ok_or("Missing Java archive")?));
                    parameters.extend(iter.cloned());
                    break;
                }
                a if a.starts_with("-D") => {
                    let (key, value) = a[2..].split_once('=').unwrap_or((&a[2..], ""));
                    builder = builder.add_system_property(key, value);
                }
                a if a.starts_with("-Xmx") || a.starts_with("-XX:") => {}
                a if a.starts_with('-') => return Err(format!("Unsupported exporter option: {a}")),
                name => {
                    main = Some(name.to_owned());
                    parameters.extend(iter.cloned());
                    break;
                }
            }
        }
        if jar.is_some() {
            return Err("Embedded preparation requires a verified class path".into());
        }
        let main = main.ok_or("Missing exporter main class")?;
        builder = builder.main_class(&main);
        builder = builder
            .add_system_property("horizon.embedded", "true")
            .add_system_property("java.awt.headless", "true")
            .add_system_property(
                "log4j2.loggerContextFactory",
                "org.apache.logging.log4j.simple.SimpleLoggerContextFactory",
            )
            .add_system_property("log4j2.simplelog.level", "OFF")
            .add_system_property("log4j2.disableJmx", "true");
        if let Some(cwd) = cwd {
            builder = builder.add_system_property("user.dir", cwd.to_string_lossy());
        }
        let mut key: Vec<String> = args
            .iter()
            .take_while(|a| **a != main)
            .filter(|a| !a.starts_with("-Xmx") && !a.starts_with("-XX:"))
            .cloned()
            .collect();
        key.push(
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .into_owned(),
        );
        let isolated = main.ends_with("VanillaCommandParseOracle")
            || main.ends_with("VanillaCreativeTabsExporter");
        let mut cached = self.cached.lock().map_err(|_| "Interpreter lock failed")?;
        if isolated || cached.as_ref().is_some_and(|c| c.key != key) {
            *cached = None;
        }
        if cached.is_none() {
            let configuration = builder.build().map_err(error)?;
            // Explicit interpretation and a current-thread executor require no executable pages.
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            let vm = runtime.block_on(VM::new(configuration)).map_err(error)?;
            *cached = Some(Cached {
                runtime,
                vm,
                key,
                log: writer,
            });
        }
        let state = cached.as_mut().unwrap();
        *state
            .log
            .lock()
            .map_err(|_| "Preparation log lock failed")? =
            log.try_clone().map_err(|e| e.to_string())?;
        let result = state
            .runtime
            .block_on(invoke(&state.vm, &main, &parameters))
            .map_err(error);
        if isolated || result.as_ref().copied().unwrap_or(true) {
            *cached = None;
        }
        result.map(|_| ())
    }
}

/// The kit contains only Horizon's own exporter classes/sources and its pin file.
/// Mojang files stay in the user's sandbox and are never part of the application bundle.
pub fn prepare(
    kit: &Path,
    minecraft: &Path,
    jre: &Path,
    output: &Path,
    progress: &dyn Fn(f32),
    log: &fs::File,
) -> Result<(), String> {
    if !horizon_prepare::exporters::available(kit) {
        return Err("The preparation support files are missing or damaged".into());
    }
    if !horizon_prepare::java::in_process_installed() {
        horizon_prepare::java::install_in_process(Arc::new(Interpreter {
            home: jre.to_path_buf(),
            cached: Mutex::new(None),
        }))?;
    }
    let _release = ReleaseVm;
    let version = horizon_prepare::artifacts::Lock::read(kit)?.version;
    let version_dir = minecraft.join("versions").join(&version);
    // Leave the last complete installation intact if preparation is interrupted.
    let stage = output.with_extension("preparing");
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let options = horizon_prepare::Options {
        root: kit.to_path_buf(),
        import: true,
        minecraft_dir: Some(minecraft.to_path_buf()),
        server_jar: Some(version_dir.join("server.jar")),
        java_home: None,
        output: Some(stage.clone()),
        inputs: horizon_prepare::artifacts::Inputs {
            version_json: Some(version_dir.join(format!("{version}.json"))),
            client_jar: Some(version_dir.join("client.jar")),
            libraries_dir: Some(minecraft.join("libraries")),
            assets_dir: Some(minecraft.join("assets")),
        },
    };
    let say = |line: &str| {
        let mut file = log;
        let _ = writeln!(file, "{line}");
    };
    horizon_prepare::prepare_with_progress(&options, &say, progress)?;
    for kind in ["sounds", "panorama", "fonts", "packs", "languages"] {
        horizon_prepare::add_assets(
            kind,
            kit,
            Some(&stage),
            Some(minecraft),
            &options.inputs,
            &say,
        )?;
    }
    let missing = horizon_prepare::check(kit, &stage)?;
    if !missing.is_empty() {
        return Err(format!(
            "Resource preparation incomplete: {}",
            missing.join(", ")
        ));
    }
    let previous = output.with_extension("previous");
    if previous.exists() {
        fs::remove_dir_all(&previous).map_err(|e| e.to_string())?;
    }
    if output.exists() {
        fs::rename(output, &previous).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(&stage, output) {
        if previous.exists() {
            let _ = fs::rename(&previous, output);
        }
        return Err(e.to_string());
    }
    if previous.exists() {
        fs::remove_dir_all(previous).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn runtime_spec() -> serde_json::Value {
    serde_json::from_str(include_str!("../runtime.json")).expect("bundled runtime pin")
}
fn sha256(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn runtime_ready(home: &Path) -> bool {
    home.join("release").is_file()
        && sha256(&home.join("lib/modules")).ok().as_deref()
            == runtime_spec()["modules_sha256"].as_str()
}
/// Install only Java class-library data and configuration. No downloaded native code is executed.
pub fn install_runtime(archive: &Path, home: &Path) -> Result<(), String> {
    let spec = runtime_spec();
    if sha256(archive)? != spec["sha256"].as_str().unwrap() {
        return Err("Java class library checksum mismatch".into());
    }
    if runtime_ready(home) {
        return Ok(());
    }
    let stage = home.with_extension("installing");
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let source = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(source));
    let mut extracted = 0u64;
    let mut links = Vec::new();
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_symlink() {
            continue;
        }
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let mut components = path.components();
        components.next();
        let relative = components.as_path();
        if relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Unsafe Java runtime archive path".into());
        }
        let keep = relative == Path::new("release")
            || relative.starts_with("lib")
            || relative.starts_with("conf")
            || relative.starts_with("legal");
        let extension = relative.extension().and_then(|s| s.to_str()).unwrap_or("");
        if !keep
            || matches!(
                relative.to_str(),
                Some("lib/src.zip" | "lib/ct.sym" | "lib/classlist")
            )
            || matches!(
                extension,
                "so" | "dll" | "dylib" | "a" | "debuginfo" | "jsa"
            )
        {
            continue;
        }
        if kind.is_symlink() {
            let link = entry
                .link_name()
                .map_err(|e| e.to_string())?
                .ok_or("Missing runtime link target")?;
            let mut resolved = relative.parent().unwrap().to_path_buf();
            for component in link.components() {
                match component {
                    std::path::Component::Normal(name) => resolved.push(name),
                    std::path::Component::ParentDir if resolved.pop() => {}
                    std::path::Component::CurDir => {}
                    _ => return Err("Unsafe runtime data link".into()),
                }
            }
            links.push((relative.to_path_buf(), resolved));
            continue;
        }
        extracted = extracted
            .checked_add(entry.size())
            .ok_or("Runtime archive too large")?;
        if extracted > 512 * 1024 * 1024 {
            return Err("Runtime archive exceeds its extraction budget".into());
        }
        let target = stage.join(relative);
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut file = fs::File::create(target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut file).map_err(|e| e.to_string())?;
    }
    // Preserve the upstream legal/configuration links as ordinary sandboxed data files.
    while !links.is_empty() {
        let before = links.len();
        let mut remaining = Vec::new();
        for (relative, resolved) in links {
            let source = stage.join(resolved);
            if !source.is_file() {
                remaining.push((relative, source.strip_prefix(&stage).unwrap().to_path_buf()));
                continue;
            }
            let target = stage.join(relative);
            fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::copy(source, target).map_err(|e| e.to_string())?;
        }
        if remaining.len() == before {
            return Err("Unresolved runtime data link".into());
        }
        links = remaining;
    }
    if !runtime_ready(&stage) {
        return Err("Incomplete Java class library installation".into());
    }
    if home.exists() {
        fs::remove_dir_all(home).map_err(|e| e.to_string())?;
    }
    fs::rename(stage, home).map_err(|e| e.to_string())
}

/// The preparer verifies the complete source JARs before invoking Java. Expose their
/// unchanged resource bytes as files, so reading metadata does not depend on JVM
/// certificate/event-logging support. Class files remain in the verified JARs.
fn resource_classpath(cp: &str, home: &Path) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    let cache = home.parent().unwrap_or(home).join("java-resources");
    for name in cp.split(':') {
        let jar = Path::new(name);
        if jar.extension().and_then(|p| p.to_str()) == Some("jar") {
            let mut archive = zip::ZipArchive::new(fs::File::open(jar).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let signed = archive.file_names().filter_map(Result::ok).any(|n| {
                n.starts_with("META-INF/")
                    && matches!(n.rsplit('.').next(), Some("RSA" | "DSA" | "EC"))
            });
            if signed {
                let digest = sha256(jar)?;
                let output = cache.join(&digest);
                if !output.join(".complete").is_file() {
                    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
                    let mut bytes = 0u64;
                    for i in 0..archive.len() {
                        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
                        if file.is_dir()
                            || file.name().map_err(|e| e.to_string())?.ends_with(".class")
                        {
                            continue;
                        }
                        let relative = file
                            .enclosed_name()
                            .ok_or("Unsafe JAR resource path")?
                            .to_path_buf();
                        bytes = bytes
                            .checked_add(file.size())
                            .ok_or("JAR resource size overflow")?;
                        if bytes > 1024 * 1024 * 1024 {
                            return Err("JAR resource size budget exceeded".into());
                        }
                        let target = output.join(relative);
                        fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                        let mut target = fs::File::create(target).map_err(|e| e.to_string())?;
                        std::io::copy(&mut file, &mut target).map_err(|e| e.to_string())?;
                    }
                    fs::write(output.join(".complete"), digest).map_err(|e| e.to_string())?;
                }
                paths.push(output.to_string_lossy().into_owned());
            }
        }
        paths.push(name.to_string());
    }
    Ok(paths)
}

#[cfg(all(test, not(target_os = "ios")))]
mod tests {
    use super::*;
    #[test]
    fn collector_retains_and_reclaims_a_deep_graph() {
        use ristretto_gc::{Configuration, GarbageCollector, Gc, GcRootGuard, Trace};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::{Duration, Instant};
        struct Node {
            next: Option<Gc<Node>>,
            alive: Arc<AtomicUsize>,
        }
        impl Trace for Node {
            fn trace(&self, collector: &GarbageCollector) {
                self.next.trace(collector);
            }
        }
        impl Drop for Node {
            fn drop(&mut self) {
                self.alive.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let collector = GarbageCollector::with_config(
            Configuration::builder()
                .threads(1)
                .allocation_threshold(usize::MAX)
                .build(),
        );
        let alive = Arc::new(AtomicUsize::new(0));
        let mut head: Option<GcRootGuard<Node>> = None;
        for _ in 0..100_000 {
            alive.fetch_add(1, Ordering::SeqCst);
            let next = head.as_ref().map(|node| node.clone_gc());
            head = Some(Gc::new(
                &collector,
                Node {
                    next,
                    alive: alive.clone(),
                },
            ));
        }
        collector.start();
        let cycle = || {
            let before = collector.statistics().unwrap().collections_completed;
            collector.collect();
            let deadline = Instant::now() + Duration::from_secs(10);
            while collector.statistics().unwrap().collections_completed == before {
                assert!(Instant::now() < deadline, "collection did not finish");
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        cycle();
        assert_eq!(alive.load(Ordering::SeqCst), 100_000);
        let mut count = 0;
        let mut node = head.as_ref().map(|node| node.clone_gc());
        while let Some(current) = node {
            count += 1;
            node = current.next.clone();
        }
        assert_eq!(count, 100_000);
        drop(head);
        cycle();
        assert_eq!(alive.load(Ordering::SeqCst), 0);
        collector.stop().unwrap();
    }

    /// Runs on a host with JAVA_HOME: checks the language features required by the
    /// real exporters, and invokes two entry points through one retained interpreter.
    #[test]
    #[ignore = "requires pinned JDK 25 in JAVA_HOME"]
    fn exporter_language_features_and_vm_reuse() {
        let home = PathBuf::from(std::env::var_os("JAVA_HOME").expect("JAVA_HOME"));
        let dir = std::env::temp_dir().join(format!("horizon-java-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let source = r#"
import java.util.function.Supplier;
interface Codec<T> {
    default Supplier<String> field(String name) { return () -> name + ":" + this.toString(); }
}
public class Probe implements Codec<String> {
    static final ScopedValue<String> KEY = ScopedValue.newInstance();
    public String toString() { return "codec"; }
    public static void main(String[] args) throws Exception {
        if (!new Probe().field("value").get().equals("value:codec")) throw new AssertionError("interface lambda");
        if (KEY.isBound()) throw new AssertionError("binding leaked");
        ScopedValue.where(KEY, "outer").run(() -> {
            if (!KEY.get().equals("outer")) throw new AssertionError("outer");
            ScopedValue.where(KEY, "inner").run(() -> {
                if (!KEY.get().equals("inner")) throw new AssertionError("inner");
            });
            if (!KEY.get().equals("outer")) throw new AssertionError("restore");
        });
        if (KEY.isBound()) throw new AssertionError("scope cleanup");
        java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]), "passed");
    }
}
class Exit {
    public static void main(String[] args) { System.exit(Integer.parseInt(args[0])); }
}
class Second {
    public static void main(String[] args) throws Exception {
        if (!java.nio.file.Files.readString(java.nio.file.Path.of(args[0])).equals("passed")) throw new AssertionError("files");
        Probe.main(new String[]{args[1]});
    }
}
"#;
        fs::write(dir.join("Probe.java"), source).unwrap();
        assert!(
            std::process::Command::new(home.join("bin/javac"))
                .arg(dir.join("Probe.java"))
                .status()
                .unwrap()
                .success()
        );
        let interpreter = Interpreter {
            home,
            cached: Mutex::new(None),
        };
        let log = fs::File::create(dir.join("run.log")).unwrap();
        let first = dir.join("first.txt");
        let second = dir.join("second.txt");
        let prefix = vec!["-cp".into(), dir.to_string_lossy().into_owned()];
        let mut args = prefix.clone();
        args.extend(["Probe".into(), first.to_string_lossy().into_owned()]);
        interpreter.run(&args, Some(&dir), &log).unwrap();
        let mut args = prefix;
        args.extend([
            "Second".into(),
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ]);
        interpreter.run(&args, Some(&dir), &log).unwrap();
        let mut exit = vec![
            "-cp".into(),
            dir.to_string_lossy().into_owned(),
            "Exit".into(),
            "0".into(),
        ];
        interpreter.run(&exit, Some(&dir), &log).unwrap();
        exit[3] = "7".into();
        assert!(
            interpreter
                .run(&exit, Some(&dir), &log)
                .unwrap_err()
                .contains("exit:7")
        );
        interpreter.release();
        assert_eq!(fs::read_to_string(second).unwrap(), "passed");
        fs::remove_dir_all(dir).unwrap();
    }
}
