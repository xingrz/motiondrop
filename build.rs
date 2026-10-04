fn main() {
    println!("cargo:rerun-if-changed=native/player.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/player.m")
            .flag("-fobjc-arc")
            .flag("-Wno-deprecated-declarations")
            .compile("motiondrop_player");
        for framework in [
            "AVFoundation",
            "CoreMedia",
            "CoreVideo",
            "Foundation",
            "ImageIO",
            "CoreGraphics",
            "AppKit",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
