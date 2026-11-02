use std::{fs, path::PathBuf};
fn main() -> Result<(), String> {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if args.len() == 3 && args[0] == PathBuf::from("--install-runtime") {
        return horizon_ios_bootstrap::install_runtime(&args[1], &args[2]);
    }
    if args.len() != 4 {
        return Err("usage: prepare KIT MINECRAFT JAVA_HOME OUTPUT".into());
    }
    let log = fs::File::create(args[3].with_extension("log")).map_err(|e| e.to_string())?;
    horizon_ios_bootstrap::prepare(
        &args[0],
        &args[1],
        &args[2],
        &args[3],
        &|p| println!("{:.0}%", p * 100.0),
        &log,
    )
}
