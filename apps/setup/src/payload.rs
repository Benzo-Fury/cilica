//! The installer Setup carries: Tauri's NSIS installer for this version of Cilica, built into
//! the binary by `build.rs` from `CILICA_SETUP_PAYLOAD`. Empty in the copy that sits next to
//! Cilica, which only removes it.

/// The NSIS installer's bytes, or nothing.
pub static INSTALLER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));

/// How many bytes Cilica takes once installed, recorded at build time (`CILICA_SETUP_APP_BYTES`).
pub fn app_bytes() -> Option<u64> {
    option_env!("CILICA_SETUP_APP_BYTES").and_then(|v| v.trim().parse().ok())
}

/// Whether this copy can install Cilica.
pub fn present() -> bool {
    !INSTALLER.is_empty()
}
