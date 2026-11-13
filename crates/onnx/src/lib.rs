//! The one ONNX Runtime environment for the whole process.
//!
//! ONNX Runtime allows a single environment per process. Voice-activity detection (`stt`) and
//! speaker labelling (`diarize`) both run ONNX models, so they must share this one; building a
//! second environment fails with "only one environment is allowed per process".

use std::sync::OnceLock;

pub use ort;

/// The shared environment, created on first use. The error is the runtime's own message.
pub fn env() -> Result<&'static ort::environment::Environment, String> {
    static ENV: OnceLock<Result<ort::environment::Environment, String>> = OnceLock::new();
    ENV.get_or_init(|| ort::init().with_name("cilica").build().map_err(|e| e.to_string())).as_ref().map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    #[test]
    fn two_callers_share_one_environment() {
        let a = super::env().expect("first caller");
        let b = super::env().expect("second caller");
        assert!(std::ptr::eq(a, b));
    }
}
