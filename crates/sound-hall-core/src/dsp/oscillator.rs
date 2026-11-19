use std::f32::consts::TAU;

use super::math::{flush_denormal, wrap_phase, Rng};

/// Supported oscillator waveforms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Waveform {
    Sine,
    Triangle,
    Saw,
    Square,
    Pulse(f32),
    WhiteNoise,
    PinkNoise,
}

/// 2-point polynomial band-limited step (PolyBLEP) residual for discontinuity anti-aliasing.
#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 || dt >= 0.5 {
        return 0.0;
    }
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// Anti-aliased PolyBLEP oscillator with optional phase modulation and integrated triangle.
#[derive(Clone, Debug)]
pub struct PolyBlepOsc {
    phase: f32,
    tri_integrator: f32,
    rng: Rng,
}

impl PolyBlepOsc {
    pub fn new(seed: u64) -> Self {
        Self {
            phase: 0.0,
            tri_integrator: 0.0,
            rng: Rng::new(seed),
        }
    }

    pub fn with_phase(mut self, phase: f32) -> Self {
        self.phase = wrap_phase(phase);
        self
    }

    #[inline]
    pub fn reset(&mut self, phase: f32) {
        self.phase = wrap_phase(phase);
        self.tri_integrator = 0.0;
    }

    #[inline]
    pub fn phase(&self) -> f32 {
        self.phase
    }

    /// Computes normalized phase increment `dt = freq / sr`.
    #[inline]
    pub fn dt(freq: f32, sample_rate: f32) -> f32 {
        (freq / sample_rate.max(1.0)).clamp(0.0, 0.49)
    }

    /// Renders one sample and advances phase by `dt`.
    #[inline]
    pub fn next_sample(&mut self, waveform: Waveform, dt: f32) -> f32 {
        self.next_sample_pm(waveform, dt, 0.0)
    }

    /// Renders one sample with phase modulation (`pm_radians`) and advances phase by `dt`.
    #[inline]
    pub fn next_sample_pm(&mut self, waveform: Waveform, dt: f32, pm_radians: f32) -> f32 {
        let dt = dt.clamp(0.0, 0.49);
        let p = if pm_radians != 0.0 {
            wrap_phase(self.phase + pm_radians / TAU)
        } else {
            self.phase
        };

        let out = match waveform {
            Waveform::Sine => (p * TAU).sin(),
            Waveform::Saw => {
                let naive = 2.0 * p - 1.0;
                naive - poly_blep(p, dt)
            }
            Waveform::Square => {
                let mut sq = if p < 0.5 { 1.0 } else { -1.0 };
                sq += poly_blep(p, dt);
                sq -= poly_blep(wrap_phase(p + 0.5), dt);
                sq
            }
            Waveform::Pulse(width) => {
                let w = width.clamp(0.05, 0.95);
                let mut pulse = if p < w { 1.0 } else { -1.0 };
                pulse += poly_blep(p, dt);
                pulse -= poly_blep(wrap_phase(p + (1.0 - w)), dt);
                // DC compensation for asymmetric duty cycles
                pulse - (2.0 * w - 1.0)
            }
            Waveform::Triangle => {
                if pm_radians != 0.0 || dt <= 1e-6 {
                    1.0 - 4.0 * (p - 0.5).abs()
                } else {
                    // Leaky-integrated PolyBLEP square gives clean band-limited triangle
                    let mut sq = if p < 0.5 { 1.0 } else { -1.0 };
                    sq += poly_blep(p, dt);
                    sq -= poly_blep(wrap_phase(p + 0.5), dt);
                    let leak = (1.0 - dt * 0.5).clamp(0.95, 0.9999);
                    let integ = leak * self.tri_integrator + 4.0 * dt * sq;
                    self.tri_integrator = flush_denormal(integ.clamp(-1.25, 1.25));
                    self.tri_integrator.clamp(-1.0, 1.0)
                }
            }
            Waveform::WhiteNoise => self.rng.next_f32_bipolar(),
            Waveform::PinkNoise => self.rng.next_pink(),
        };

        self.phase = wrap_phase(self.phase + dt);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveforms_stay_bounded() {
        let sr = 44100.0;
        let dt = PolyBlepOsc::dt(440.0, sr);
        let waves = [
            Waveform::Sine,
            Waveform::Triangle,
            Waveform::Saw,
            Waveform::Square,
            Waveform::Pulse(0.25),
            Waveform::WhiteNoise,
            Waveform::PinkNoise,
        ];
        for w in waves {
            let mut osc = PolyBlepOsc::new(123);
            for _ in 0..2000 {
                let s = osc.next_sample(w, dt);
                assert!(s.is_finite() && s.abs() <= 2.05, "out={s} for {w:?}");
            }
        }
    }
}
