//! Input level metering for the UI.

/// RMS level on a perceptual 0..=1 scale: -60 dBFS (and below) is 0, 0 dBFS is 1, linear in dB
/// in between, with a fast attack and a slower release so the meter reads naturally.
#[derive(Debug, Clone)]
pub struct LevelMeter {
    sum_sq: f64,
    count: usize,
    display: f32,
}

const FLOOR_DB: f32 = -60.0;

impl Default for LevelMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelMeter {
    pub fn new() -> Self {
        Self { sum_sq: 0.0, count: 0, display: 0.0 }
    }

    pub fn push(&mut self, samples: &[f32]) {
        for s in samples {
            self.sum_sq += (*s as f64) * (*s as f64);
        }
        self.count += samples.len();
    }

    /// The level since the last call, smoothed. Call at the display rate (about 20 Hz).
    pub fn take(&mut self) -> f32 {
        let rms = if self.count == 0 { 0.0 } else { (self.sum_sq / self.count as f64).sqrt() as f32 };
        self.sum_sq = 0.0;
        self.count = 0;
        let target = perceptual(rms);
        // Attack immediately, release at ~ -0.35 per second at 20 Hz.
        self.display = if target >= self.display { target } else { (self.display - 0.06).max(target) };
        self.display
    }
}

/// Map an RMS amplitude to 0..=1 on a dB scale.
pub fn perceptual(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * rms.log10();
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale() {
        assert_eq!(perceptual(0.0), 0.0);
        assert!((perceptual(1.0) - 1.0).abs() < 1e-6);
        assert!((perceptual(0.001) - 0.0).abs() < 1e-6); // -60 dB
        assert!((perceptual(0.0316) - 0.5).abs() < 0.01); // -30 dB
    }

    #[test]
    fn attack_and_release() {
        let mut m = LevelMeter::new();
        m.push(&[0.5; 800]);
        let loud = m.take();
        assert!(loud > 0.8);
        m.push(&[0.0; 800]);
        let after = m.take();
        assert!(after < loud && after > 0.7, "release is gradual: {after}");
        for _ in 0..30 {
            m.push(&[0.0; 800]);
            m.take();
        }
        assert_eq!(m.take(), 0.0);
    }
}
