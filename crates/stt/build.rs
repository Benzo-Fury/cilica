//! ONNX Runtime's prebuilt static library (CoreML provider) calls `__isPlatformVersionAtLeast`,
//! which lives in clang's runtime. Rust's own copy can be dropped by LTO in release builds, so
//! link clang's runtime explicitly on macOS. The directive propagates to the final binary.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(out) = std::process::Command::new("xcrun").args(["clang", "--print-runtime-dir"]).output() else { return };
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !dir.is_empty() && std::path::Path::new(&dir).join("libclang_rt.osx.a").exists() {
        println!("cargo:rustc-link-search=native={dir}");
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    }
}
