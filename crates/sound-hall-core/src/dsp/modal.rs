use std::f32::consts::{PI, TAU};

use super::math::{flush_denormal, Rng};
use super::oscillator::PolyBlepOsc;

/// Maximum number of resonant modes in a `ModalBank`.
pub const MAX_MODES: usize = 10;

/// Single damped modal resonator specification.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModeSpec {
    /// Frequency ratio relative to note fundamental (e.g. 1.0, 2.756, 5.404).
    pub ratio: f32,
    /// Relative initial modal amplitude.
    pub amp: f32,
    /// Exponential time constant (seconds) for this mode.
    pub decay: f32,
}

impl ModeSpec {
    pub const fn new(ratio: f32, amp: f32, decay: f32) -> Self {
        Self { ratio, amp, decay }
    }
}

/// Bank of up to 10 parallel 2-pole resonant modes with mallet/hammer excitation.
#[derive(Clone, Debug)]
pub struct ModalBank {
    specs: [ModeSpec; MAX_MODES],
    mode_count: usize,
    g: [f32; MAX_MODES],
    a1: [f32; MAX_MODES],
    a2: [f32; MAX_MODES],
    y1: [f32; MAX_MODES],
    y2: [f32; MAX_MODES],
    active: [bool; MAX_MODES],
    cached_freq: f32,
    cached_sr: f32,
    // Mallet excitation state
    exc_pos: usize,
    exc_len: usize,
    exc_norm: f32,
    noise_amt: f32,
    rng: Rng,
    // Optional tremolo (e.g. vibraphone motor)
    tremolo_lfo: PolyBlepOsc,
    tremolo_hz: f32,
    tremolo_depth: f32,
}

impl ModalBank {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        modes: &[ModeSpec],
        freq: f32,
        sr: f32,
        velocity: f32,
        mallet_hardness: f32,
        noise_amt: f32,
        tremolo_hz: f32,
        tremolo_depth: f32,
        seed: u64,
    ) -> Self {
        let mut specs = [ModeSpec::new(1.0, 0.0, 0.1); MAX_MODES];
        let count = modes.len().min(MAX_MODES);
        specs[..count].copy_from_slice(&modes[..count]);

        let hardness = (mallet_hardness * (0.55 + 0.45 * velocity)).clamp(0.05, 1.0);
        // Softer mallet -> longer contact time -> warm low-passed attack
        let contact_sec = 0.00035 + (1.0 - hardness) * 0.0045;
        let exc_len = ((contact_sec * sr).round() as usize).max(4);
        // Normalize half-sine pulse integral to 1.0 so modal peak amplitudes equal `spec.amp`
        let mut pulse_sum = 0.0_f32;
        for i in 0..exc_len {
            let t = (i as f32 + 0.5) / exc_len as f32;
            pulse_sum += (PI * t).sin();
        }
        let exc_norm = if pulse_sum > 1e-6 {
            velocity / pulse_sum
        } else {
            velocity
        };

        let mut bank = Self {
            specs,
            mode_count: count,
            g: [0.0; MAX_MODES],
            a1: [0.0; MAX_MODES],
            a2: [0.0; MAX_MODES],
            y1: [0.0; MAX_MODES],
            y2: [0.0; MAX_MODES],
            active: [false; MAX_MODES],
            cached_freq: -1.0,
            cached_sr: -1.0,
            exc_pos: 0,
            exc_len,
            exc_norm,
            noise_amt: noise_amt.clamp(0.0, 1.0),
            rng: Rng::new(seed),
            tremolo_lfo: PolyBlepOsc::new(seed ^ 0xA5A5),
            tremolo_hz,
            tremolo_depth: tremolo_depth.clamp(0.0, 0.95),
        };
        bank.update_coeffs(freq, sr);
        bank
    }

    pub fn update_coeffs(&mut self, freq: f32, sr: f32) {
        let sr = sr.max(8000.0);
        if (freq - self.cached_freq).abs() < 1e-3 && (sr - self.cached_sr).abs() < 1e-2 {
            return;
        }
        self.cached_freq = freq;
        self.cached_sr = sr;

        for i in 0..self.mode_count {
            let spec = self.specs[i];
            let f = freq * spec.ratio;
            if f < 18.0 || f >= 0.45 * sr || spec.amp <= 0.0 {
                self.active[i] = false;
                self.g[i] = 0.0;
                continue;
            }
            self.active[i] = true;
            let w = TAU * f / sr;
            // Slightly shorter decay at very high frequencies to mirror physical air/internal damping
            let freq_damp = (440.0 / f.max(120.0)).powf(0.18).clamp(0.35, 1.35);
            let decay = (spec.decay * freq_damp).max(0.003);
            let r = (-1.0 / (decay * sr)).exp().clamp(0.0, 0.99997);
            self.g[i] = spec.amp * w.sin();
            self.a1[i] = 2.0 * r * w.cos();
            self.a2[i] = r * r;
        }
    }

    /// Re-excites the modal bank (e.g. for roll or rattle bursts).
    pub fn re_excite(&mut self, velocity: f32) {
        self.exc_pos = 0;
        let mut pulse_sum = 0.0_f32;
        for i in 0..self.exc_len {
            let t = (i as f32 + 0.5) / self.exc_len as f32;
            pulse_sum += (PI * t).sin();
        }
        self.exc_norm = if pulse_sum > 1e-6 {
            velocity / pulse_sum
        } else {
            velocity
        };
    }

    /// Renders one mono sample at `freq`.
    #[inline]
    pub fn next_sample(&mut self, freq: f32, sr: f32) -> f32 {
        self.update_coeffs(freq, sr);

        let exc = if self.exc_pos < self.exc_len {
            let t = (self.exc_pos as f32 + 0.5) / self.exc_len as f32;
            let pulse = (PI * t).sin() * self.exc_norm;
            let noise = self.rng.next_f32_bipolar() * self.noise_amt * self.exc_norm * 0.45;
            self.exc_pos += 1;
            pulse + noise
        } else {
            0.0
        };

        let mut sum = 0.0_f32;
        for i in 0..self.mode_count {
            if !self.active[i] {
                continue;
            }
            let y =
                flush_denormal(self.g[i] * exc + self.a1[i] * self.y1[i] - self.a2[i] * self.y2[i]);
            self.y2[i] = self.y1[i];
            self.y1[i] = y;
            sum += y;
        }

        if self.tremolo_hz > 0.01 && self.tremolo_depth > 0.001 {
            let dt = PolyBlepOsc::dt(self.tremolo_hz, sr);
            let lfo = 0.5
                + 0.5
                    * self
                        .tremolo_lfo
                        .next_sample(super::oscillator::Waveform::Sine, dt);
            let mod_gain = 1.0 - self.tremolo_depth * (1.0 - lfo);
            sum * mod_gain
        } else {
            sum
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_bank_rings_at_target_pitch() {
        let modes = [
            ModeSpec::new(1.0, 1.0, 0.35),
            ModeSpec::new(3.98, 0.3, 0.12),
            ModeSpec::new(9.2, 0.1, 0.05),
        ];
        let mut bank = ModalBank::new(&modes, 440.0, 44100.0, 0.9, 0.85, 0.1, 0.0, 0.0, 7);
        let mut max_amp = 0.0_f32;
        for _ in 0..44100 {
            let s = bank.next_sample(440.0, 44100.0);
            assert!(s.is_finite());
            max_amp = max_amp.max(s.abs());
        }
        assert!(max_amp > 0.2, "max_amp={max_amp}");
        let tail = bank.next_sample(440.0, 44100.0).abs();
        assert!(tail < 0.08, "tail should decay, got {tail}");
    }
}
