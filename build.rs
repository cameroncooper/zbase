use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=native/updater.m");
    println!("cargo:rerun-if-changed=native/notifications.m");
    println!("cargo:rerun-if-changed=native/status_item.m");
    println!("cargo:rerun-if-env-changed=SPARKLE_DIR");
    #[cfg(windows)]
    windows_resources();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    build_native_bridges();
    if env::var_os("CARGO_FEATURE_MACOS_UPDATER").is_none() {
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

#[cfg(windows)]
fn windows_resources() {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    // Kit's platform backend embeds the application manifest, including Common
    // Controls v6 and DPI awareness. Only add our icon here: /MANIFEST:EMBED
    // would generate another resource with the same ID and fail MSVC linking.
    println!("cargo:rerun-if-changed=assets/windows/zbase.rc");
    println!("cargo:rerun-if-changed=assets/windows/zBase.ico");
    embed_resource::compile_for("assets/windows/zbase.rc", ["zbase"], embed_resource::NONE)
        .manifest_required()
        .unwrap();
}

/// Desktop notifications and the menu bar icon, linked as one static library.
fn build_native_bridges() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let objects = ["notifications", "status_item"].map(|name| {
        let object = out.join(format!("{name}.o"));
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
            ])
            .arg(format!("native/{name}.m"))
            .arg("-o")
            .arg(&object)
            .status()
            .expect("clang");
        assert!(status.success(), "native/{name}.m failed to compile");
        object
    });
    let library = out.join("libzbase_native.a");
    // `ar crs` adds to an existing archive; start over so no stale object stays.
    let _ = std::fs::remove_file(&library);
    assert!(
        Command::new("ar")
            .arg("crs")
            .arg(&library)
            .args(objects)
            .status()
            .unwrap()
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=zbase_native");
    println!("cargo:rustc-link-lib=framework=UserNotifications");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=AppKit");
}
