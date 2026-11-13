//! Builds the window (tauri-build) and bundles the NSIS installer Setup carries.
//!
//! `CILICA_SETUP_PAYLOAD` names Tauri's `*-setup.exe` for this version of Cilica; it is copied to
//! `OUT_DIR/payload.bin` and built into the binary (src/payload.rs). Without it the payload is
//! empty and the binary can only remove Cilica: that is the copy installed next to Cilica.
//! `CILICA_SETUP_APP_BYTES` (Cilica's installed size) is read by src/payload.rs at compile time.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=CILICA_SETUP_PAYLOAD");
    println!("cargo:rerun-if-env-changed=CILICA_SETUP_APP_BYTES");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("payload.bin");
    match std::env::var("CILICA_SETUP_PAYLOAD").ok().filter(|p| !p.trim().is_empty()) {
        Some(path) => {
            println!("cargo:rerun-if-changed={path}");
            std::fs::copy(&path, &out).unwrap_or_else(|e| panic!("could not read the installer at {path}: {e}"));
        }
        None => std::fs::write(&out, []).expect("could not write an empty payload"),
    }
    tauri_build::build()
}
