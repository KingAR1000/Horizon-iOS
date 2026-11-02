//! Build tool: compile only Horizon's original exporter code; never package Minecraft files.
use std::{
    fs,
    path::{Path, PathBuf},
};
fn copy(from: &Path, to: &Path, extension: Option<&str>) -> Result<(), String> {
    if from.is_dir() {
        fs::create_dir_all(to).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_symlink() {
                return Err("Symlink in preparation kit".into());
            }
            copy(&entry.path(), &to.join(entry.file_name()), extension)?;
        }
    } else if extension.is_none() || from.extension().and_then(|s| s.to_str()) == extension {
        fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::copy(from, to).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() -> Result<(), String> {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if args.len() != 3 {
        return Err("usage: horizon-ios-kit ENGINE VERIFIED_INPUTS DESTINATION".into());
    }
    let (root, minecraft, dest) = (&args[0], &args[1], &args[2]);
    let version = horizon_prepare::artifacts::Lock::read(root)?.version;
    let version_dir = minecraft.join("versions").join(&version);
    let options = horizon_prepare::Options {
        root: root.clone(),
        import: true,
        minecraft_dir: Some(minecraft.clone()),
        server_jar: Some(version_dir.join("server.jar")),
        java_home: None,
        output: None,
        inputs: horizon_prepare::artifacts::Inputs {
            version_json: Some(version_dir.join(format!("{version}.json"))),
            client_jar: Some(version_dir.join("client.jar")),
            libraries_dir: Some(minecraft.join("libraries")),
            assets_dir: Some(minecraft.join("assets")),
        },
    };
    let classes = horizon_prepare::compile_exporters(&options, &|line| eprintln!("{line}"))?;
    let parity = Path::new("mods/horizon/src/test/java/dev/horizonlab/parity");
    copy(&root.join(parity), &dest.join(parity), Some("java"))?;
    let compiler = Path::new("crates/horizon-prepare/src/exporters.rs");
    copy(&root.join(compiler), &dest.join(compiler), None)?;
    copy(
        &root.join("research-lock.json"),
        &dest.join("research-lock.json"),
        None,
    )?;
    for side in ["server", "client"] {
        copy(
            &classes.join(side),
            &dest.join("exporters").join(side),
            Some("class"),
        )?;
    }
    copy(
        &classes.join("manifest.json"),
        &dest.join("exporters/manifest.json"),
        None,
    )?;
    if !horizon_prepare::exporters::available(dest) {
        return Err("Exporter kit validation failed".into());
    }
    println!("Validated preparation kit: {}", dest.display());
    Ok(())
}
