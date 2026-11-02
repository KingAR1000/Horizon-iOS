fn main() {
    println!("cargo:rerun-if-changed=src/ios.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        cc::Build::new()
            .file("src/ios.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("horizon_ios");
        for framework in [
            "UIKit",
            "Foundation",
            "UniformTypeIdentifiers",
            "GameController",
            "AVFoundation",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
