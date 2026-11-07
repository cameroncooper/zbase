use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=native/updater.m");
    println!("cargo:rerun-if-env-changed=SPARKLE_DIR");
    if env::var_os("CARGO_FEATURE_MACOS_UPDATER").is_none()
        || env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos")
    {
        return;
    }
    let framework = PathBuf::from(
        env::var_os("SPARKLE_DIR")
            .expect("Run scripts/macos/prepare_sparkle.sh and set SPARKLE_DIR"),
    );
    println!(
        "cargo:rerun-if-changed={}/Sparkle.framework/Headers",
        framework.display()
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("updater.o");
    let status = Command::new("clang")
        .args([
            "-fobjc-arc",
            "-fblocks",
            "-Wall",
            "-Wextra",
            "-Werror=return-type",
            "-Werror=protocol",
            "-mmacosx-version-min=15.0",
            "-c",
            "native/updater.m",
            "-F",
        ])
        .arg(&framework)
        .arg("-o")
        .arg(&object)
        .status()
        .expect("clang");
    assert!(status.success(), "Sparkle bridge failed to compile");
    assert!(
        Command::new("ar")
            .arg("crs")
            .arg(out.join("libzbase_updater.a"))
            .arg(object)
            .status()
            .unwrap()
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=zbase_updater");
    println!("cargo:rustc-link-search=framework={}", framework.display());
    println!("cargo:rustc-link-lib=framework=Sparkle");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
}
