use std::f32::consts::{FRAC_PI_2, TAU};

/// Converts a MIDI note number (fractional allowed) to frequency in Hz.
#[inline]
pub fn midi_to_hz(note: f32) -> f32 {
    440.0 * 2.0_f32.powf((note - 69.0) / 12.0)
}

/// Converts decibels to linear gain.
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db * 0.05)
}

/// Smooth soft-clipping transfer function bounded within [-1.0, 1.0].
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    x.tanh()
}

/// Transparent safety saturator: exactly linear for `|x| <= 0.75`, then a C1-continuous
/// soft knee that asymptotically approaches ±1.0.
///
/// Unlike [`soft_clip`] (`tanh`), it adds no compression or harmonic coloration at normal
/// signal levels, so it can sit inside feedback loops and on voice outputs without
/// changing decay rates or timbre.
#[inline]
pub fn soft_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.75;
    const HEADROOM: f32 = 1.0 - KNEE;
    if !x.is_finite() {
        return 0.0;
    }
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        // Rational knee: slope 1 at the knee, asymptote at 1.0
        let over = (a - KNEE) / HEADROOM;
        let shaped = KNEE + HEADROOM * over / (1.0 + over);
        shaped.copysign(x)
    }
}

/// Flushes subnormal/denormal and non-finite floating-point values to zero.
#[inline]
pub fn flush_denormal(x: f32) -> f32 {
    if !x.is_finite() || x.abs() < 1.0e-20 {
        0.0
    } else {
        x
    }
}

/// Equal-power stereo panning for `pan` in [0.0, 1.0] (0.5 = center).
#[inline]
pub fn equal_power_pan(pan: f32) -> (f32, f32) {
    let p = pan.clamp(0.0, 1.0) * FRAC_PI_2;
    (p.cos(), p.sin())
}

/// Linear interpolation helper.
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Wraps a phase value into [0.0, 1.0).
#[inline]
pub fn wrap_phase(phase: f32) -> f32 {
    let p = phase.rem_euclid(1.0);
    if p >= 1.0 {
        0.0
    } else {
        p
    }
}

/// Sine helper taking phase in [0.0, 1.0).
#[inline]
pub fn sin_phase(phase: f32) -> f32 {
    (phase * TAU).sin()
}

/// Lightweight deterministic xorshift64 pseudo-random generator for synthesis noise.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    pink_b0: f32,
    pink_b1: f32,
    pink_b2: f32,
    pink_b3: f32,
    pink_b4: f32,
    pink_b5: f32,
    pink_b6: f32,
}

impl Rng {
    /// Creates a new deterministic PRNG from a seed.
    pub fn new(seed: u64) -> Self {
        let s = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self {
            state: s,
            pink_b0: 0.0,
            pink_b1: 0.0,
            pink_b2: 0.0,
            pink_b3: 0.0,
            pink_b4: 0.0,
            pink_b5: 0.0,
            pink_b6: 0.0,
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Returns a uniform sample in [0.0, 1.0).
    #[inline]
    pub fn next_f32_unipolar(&mut self) -> f32 {
        let bits = (self.next_u64() >> 40) as u32;
        (bits as f32) * (1.0 / 16_777_216.0)
    }

    /// Returns a uniform white-noise sample in [-1.0, 1.0].
    #[inline]
    pub fn next_f32_bipolar(&mut self) -> f32 {
        self.next_f32_unipolar() * 2.0 - 1.0
    }

    /// Returns a pink-noise sample (~-3 dB/octave) roughly in [-1.0, 1.0].
    #[inline]
    pub fn next_pink(&mut self) -> f32 {
        let white = self.next_f32_bipolar();
        self.pink_b0 = 0.99886 * self.pink_b0 + white * 0.0555179;
        self.pink_b1 = 0.99332 * self.pink_b1 + white * 0.0750759;
        self.pink_b2 = 0.96900 * self.pink_b2 + white * 0.153_852;
        self.pink_b3 = 0.86650 * self.pink_b3 + white * 0.3104856;
        self.pink_b4 = 0.55000 * self.pink_b4 + white * 0.5329522;
        self.pink_b5 = -0.7616 * self.pink_b5 - white * 0.0168980;
        let pink = self.pink_b0
            + self.pink_b1
            + self.pink_b2
            + self.pink_b3
            + self.pink_b4
            + self.pink_b5
            + self.pink_b6
            + white * 0.5362;
        self.pink_b6 = white * 0.115926;
        (pink * 0.12).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_conversion_is_accurate() {
        assert!((midi_to_hz(69.0) - 440.0).abs() < 1e-4);
        assert!((midi_to_hz(60.0) - 261.62558).abs() < 1e-3);
        assert!((midi_to_hz(57.0) - 220.0).abs() < 1e-4);
    }

    #[test]
    fn equal_power_pan_preserves_energy() {
        for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (l, r) = equal_power_pan(p);
            assert!((l * l + r * r - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn rng_bounds_and_denormals() {
        let mut rng = Rng::new(42);
        for _ in 0..1000 {
            let w = rng.next_f32_bipolar();
            assert!((-1.0..=1.0).contains(&w));
            let p = rng.next_pink();
            assert!((-1.0..=1.0).contains(&p));
        }
        assert_eq!(flush_denormal(1e-30), 0.0);
        assert_eq!(flush_denormal(f32::NAN), 0.0);
    }

    #[test]
    fn soft_limit_is_transparent_then_bounded() {
        for x in [-0.75_f32, -0.3, 0.0, 0.2, 0.5, 0.75] {
            assert_eq!(soft_limit(x), x);
        }
        let mut prev = 0.75_f32;
        for i in 1..400 {
            let x = 0.75 + i as f32 * 0.05;
            let y = soft_limit(x);
            assert!(y > prev && y < 1.0, "monotonic and bounded: x={x} y={y}");
            assert_eq!(soft_limit(-x), -y);
            prev = y;
        }
        assert_eq!(soft_limit(f32::NAN), 0.0);
    }
}
