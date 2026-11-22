use std::f32::consts::{PI, TAU};

use super::math::{db_to_gain, flush_denormal};

/// State-variable filter mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvfMode {
    LowPass,
    HighPass,
    BandPass,
    Notch,
    Peak,
}

/// Cytomic (Andy Simper) trapezoidal zero-delay-feedback State Variable Filter.
#[derive(Clone, Debug)]
pub struct SvfFilter {
    ic1eq: f32,
    ic2eq: f32,
    cached_cutoff: f32,
    cached_q: f32,
    cached_sr: f32,
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl Default for SvfFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl SvfFilter {
    pub fn new() -> Self {
        let mut s = Self {
            ic1eq: 0.0,
            ic2eq: 0.0,
            cached_cutoff: -1.0,
            cached_q: -1.0,
            cached_sr: -1.0,
            g: 0.0,
            k: 1.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
        };
        s.update_coeffs(1000.0, 0.707, 44100.0);
        s
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    #[inline]
    fn update_coeffs(&mut self, cutoff: f32, q: f32, sr: f32) {
        let sr = sr.max(8000.0);
        let cutoff = cutoff.clamp(20.0, 0.45 * sr);
        let q = q.clamp(0.5, 18.0);
        if (cutoff - self.cached_cutoff).abs() < 1e-4
            && (q - self.cached_q).abs() < 1e-5
            && (sr - self.cached_sr).abs() < 1e-3
        {
            return;
        }
        self.cached_cutoff = cutoff;
        self.cached_q = q;
        self.cached_sr = sr;
        self.g = (PI * cutoff / sr).tan();
        self.k = 1.0 / q;
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    #[inline]
    pub fn process(&mut self, input: f32, cutoff: f32, q: f32, sr: f32, mode: SvfMode) -> f32 {
        self.update_coeffs(cutoff, q, sr);
        let x = if input.is_finite() { input } else { 0.0 };
        let v3 = x - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = flush_denormal(2.0 * v1 - self.ic1eq);
        self.ic2eq = flush_denormal(2.0 * v2 - self.ic2eq);

        match mode {
            SvfMode::LowPass => v2,
            SvfMode::BandPass => v1,
            SvfMode::HighPass => x - self.k * v1 - v2,
            SvfMode::Notch => x - self.k * v1,
            SvfMode::Peak => 2.0 * v2 - x + self.k * v1,
        }
    }
}

/// Stereo pair of Cytomic SVFs sharing one coefficient set (one `tan()` per sample for both).
#[derive(Clone, Debug, Default)]
pub struct StereoSvf {
    left: SvfFilter,
    right_ic1eq: f32,
    right_ic2eq: f32,
}

impl StereoSvf {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.left.reset();
        self.right_ic1eq = 0.0;
        self.right_ic2eq = 0.0;
    }

    #[inline]
    pub fn process(
        &mut self,
        left: f32,
        right: f32,
        cutoff: f32,
        q: f32,
        sr: f32,
        mode: SvfMode,
    ) -> (f32, f32) {
        let yl = self.left.process(left, cutoff, q, sr, mode);
        let f = &self.left;
        let x = if right.is_finite() { right } else { 0.0 };
        let v3 = x - self.right_ic2eq;
        let v1 = f.a1 * self.right_ic1eq + f.a2 * v3;
        let v2 = self.right_ic2eq + f.a2 * self.right_ic1eq + f.a3 * v3;
        self.right_ic1eq = flush_denormal(2.0 * v1 - self.right_ic1eq);
        self.right_ic2eq = flush_denormal(2.0 * v2 - self.right_ic2eq);
        let yr = match mode {
            SvfMode::LowPass => v2,
            SvfMode::BandPass => v1,
            SvfMode::HighPass => x - f.k * v1 - v2,
            SvfMode::Notch => x - f.k * v1,
            SvfMode::Peak => 2.0 * v2 - x + f.k * v1,
        };
        (yl, yr)
    }
}

/// Simple one-pole lowpass filter.
#[derive(Clone, Debug, Default)]
pub struct OnePoleLp {
    y: f32,
}

impl OnePoleLp {
    pub fn new() -> Self {
        Self { y: 0.0 }
    }

    pub fn reset(&mut self) {
        self.y = 0.0;
    }

    #[inline]
    pub fn coeff_from_cutoff(cutoff_hz: f32, sr: f32) -> f32 {
        let fc = cutoff_hz.clamp(10.0, 0.48 * sr.max(8000.0));
        (-TAU * fc / sr.max(8000.0)).exp()
    }

    /// Processes sample with pole coefficient `pole` in [0.0, 0.9999] (higher = darker).
    #[inline]
    pub fn process_pole(&mut self, x: f32, pole: f32) -> f32 {
        let p = pole.clamp(0.0, 0.9999);
        self.y = flush_denormal((1.0 - p) * x + p * self.y);
        self.y
    }

    #[inline]
    pub fn process_cutoff(&mut self, x: f32, cutoff_hz: f32, sr: f32) -> f32 {
        let pole = Self::coeff_from_cutoff(cutoff_hz, sr);
        self.process_pole(x, pole)
    }
}

/// First-order DC-blocking highpass filter.
#[derive(Clone, Debug)]
pub struct DcBlocker {
    x1: f32,
    y1: f32,
    r: f32,
}

impl Default for DcBlocker {
    fn default() -> Self {
        Self::new(0.995)
    }
}

impl DcBlocker {
    pub fn new(r: f32) -> Self {
        Self {
            x1: 0.0,
            y1: 0.0,
            r: r.clamp(0.9, 0.9995),
        }
    }

    /// Creates a DC blocker whose -3 dB corner sits at `cutoff_hz` for sample rate `sr`.
    pub fn with_cutoff(cutoff_hz: f32, sr: f32) -> Self {
        let sr = sr.max(8000.0);
        let r = (-TAU * cutoff_hz.max(1.0) / sr).exp();
        Self {
            x1: 0.0,
            y1: 0.0,
            r: r.clamp(0.9, 0.99995),
        }
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = flush_denormal(y);
        self.y1
    }
}

/// First-order allpass filter for string stiffness dispersion and reverb diffusion.
#[derive(Clone, Debug, Default)]
pub struct Allpass1 {
    x1: f32,
    y1: f32,
}

impl Allpass1 {
    pub fn new() -> Self {
        Self { x1: 0.0, y1: 0.0 }
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }

    /// Processes sample with coefficient `a` in (-0.95, 0.95).
    /// `H(z) = (a + z^-1) / (1 + a*z^-1)`
    #[inline]
    pub fn process(&mut self, x: f32, a: f32) -> f32 {
        let a = a.clamp(-0.95, 0.95);
        let y = a * x + self.x1 - a * self.y1;
        self.x1 = x;
        self.y1 = flush_denormal(y);
        self.y1
    }
}

/// Biquad resonator for instrument body acoustics and formants.
#[derive(Clone, Debug)]
pub struct BiquadResonator {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    gain: f32,
}

impl Default for BiquadResonator {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
            gain: 1.0,
        }
    }
}

impl BiquadResonator {
    pub fn set_bandpass(&mut self, freq: f32, q: f32, gain: f32, sr: f32) {
        let sr = sr.max(8000.0);
        let f = freq.clamp(20.0, 0.45 * sr);
        let q = q.clamp(0.4, 24.0);
        let w0 = TAU * f / sr;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.b1 = 0.0;
        self.b2 = -alpha / a0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha) / a0;
        self.gain = gain;
    }

    pub fn set_peaking(&mut self, freq: f32, q: f32, gain_db: f32, sr: f32) {
        let sr = sr.max(8000.0);
        let f = freq.clamp(20.0, 0.45 * sr);
        let q = q.clamp(0.4, 20.0);
        let a = db_to_gain(gain_db * 0.5);
        let w0 = TAU * f / sr;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha / a;
        self.b0 = (1.0 + alpha * a) / a0;
        self.b1 = (-2.0 * cos_w0) / a0;
        self.b2 = (1.0 - alpha * a) / a0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha / a) / a0;
        self.gain = 1.0;
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = flush_denormal(self.b1 * x - self.a1 * y + self.z2);
        self.z2 = flush_denormal(self.b2 * x - self.a2 * y);
        y * self.gain
    }
}

/// 3-band parallel body resonator with dry signal blend.
#[derive(Clone, Debug)]
pub struct BodyResonator {
    bands: [BiquadResonator; 3],
    dry_mix: f32,
    wet_mix: f32,
    active: bool,
}

impl BodyResonator {
    pub fn new(specs: [(f32, f32, f32); 3], dry_mix: f32, wet_mix: f32, sr: f32) -> Self {
        let mut bands = [
            BiquadResonator::default(),
            BiquadResonator::default(),
            BiquadResonator::default(),
        ];
        for (i, &(f, q, g)) in specs.iter().enumerate() {
            if f > 0.0 && g != 0.0 {
                bands[i].set_bandpass(f, q, g, sr);
            } else {
                bands[i].gain = 0.0;
            }
        }
        Self {
            bands,
            dry_mix,
            wet_mix,
            active: wet_mix != 0.0,
        }
    }

    pub fn reset(&mut self) {
        for b in &mut self.bands {
            b.reset();
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if !self.active {
            return x * self.dry_mix;
        }
        let res = self.bands[0].process(x) + self.bands[1].process(x) + self.bands[2].process(x);
        x * self.dry_mix + res * self.wet_mix
    }
}

/// Maximum number of peaking resonances in an [`InstrumentBody`].
pub const MAX_BODY_PEAKS: usize = 8;

/// Multi-resonance instrument body / bell radiation filter: a 2nd-order high-pass (weak
/// radiation below the lowest body mode), up to eight peaking resonances (body modes,
/// formants, bridge hill) and a 2nd-order low-pass, in series.
#[derive(Clone, Debug)]
pub struct InstrumentBody {
    hp: SvfFilter,
    lp: SvfFilter,
    hp_hz: f32,
    lp_hz: f32,
    peaks: [BiquadResonator; MAX_BODY_PEAKS],
    count: usize,
    sr: f32,
}

impl InstrumentBody {
    /// `peaks` are `(frequency Hz, Q, gain dB)`; `hp_hz`/`lp_hz` <= 0 disable those stages.
    pub fn new(hp_hz: f32, lp_hz: f32, peaks: &[(f32, f32, f32)], sr: f32) -> Self {
        let sr = sr.max(8000.0);
        let mut bank: [BiquadResonator; MAX_BODY_PEAKS] =
            core::array::from_fn(|_| BiquadResonator::default());
        let mut count = 0;
        for &(f, q, g) in peaks.iter().take(MAX_BODY_PEAKS) {
            if f > 20.0 && f < 0.45 * sr && g.abs() > 0.05 {
                bank[count].set_peaking(f, q, g, sr);
                count += 1;
            }
        }
        Self {
            hp: SvfFilter::new(),
            lp: SvfFilter::new(),
            hp_hz,
            lp_hz: if lp_hz > 0.0 {
                lp_hz.min(0.45 * sr)
            } else {
                0.0
            },
            peaks: bank,
            count,
            sr,
        }
    }

    pub fn reset(&mut self) {
        self.hp.reset();
        self.lp.reset();
        for p in &mut self.peaks {
            p.reset();
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let mut y = x;
        if self.hp_hz > 0.0 {
            y = self
                .hp
                .process(y, self.hp_hz, 0.75, self.sr, SvfMode::HighPass);
        }
        for p in &mut self.peaks[..self.count] {
            y = p.process(y);
        }
        if self.lp_hz > 0.0 {
            y = self
                .lp
                .process(y, self.lp_hz, 0.7, self.sr, SvfMode::LowPass);
        }
        y
    }
}

/// Vowel/timbre formant presets for `FormantBank`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FormantPreset {
    Aa,
    Oo,
    Ee,
    Choir,
    Nasal,
    BrassMute,
    Custom([(f32, f32, f32); 3]),
}

/// 3-band parallel formant filter bank.
#[derive(Clone, Debug)]
pub struct FormantBank {
    filters: [BiquadResonator; 3],
}

impl FormantBank {
    pub fn new(preset: FormantPreset, sr: f32) -> Self {
        let specs = match preset {
            FormantPreset::Aa => [(730.0, 7.0, 1.0), (1090.0, 9.0, 0.7), (2440.0, 11.0, 0.35)],
            FormantPreset::Oo => [(320.0, 7.0, 1.0), (800.0, 9.0, 0.5), (2250.0, 11.0, 0.2)],
            FormantPreset::Ee => [(280.0, 7.0, 1.0), (2250.0, 10.0, 0.65), (3000.0, 12.0, 0.4)],
            FormantPreset::Choir => [(450.0, 6.0, 1.0), (980.0, 8.0, 0.65), (2550.0, 10.0, 0.35)],
            FormantPreset::Nasal => [(550.0, 5.5, 0.7), (1350.0, 8.0, 1.0), (2650.0, 9.0, 0.5)],
            FormantPreset::BrassMute => {
                [(900.0, 5.0, 0.8), (1950.0, 7.5, 1.0), (3100.0, 8.0, 0.65)]
            }
            FormantPreset::Custom(c) => c,
        };
        let mut filters = [
            BiquadResonator::default(),
            BiquadResonator::default(),
            BiquadResonator::default(),
        ];
        for (i, &(f, q, g)) in specs.iter().enumerate() {
            filters[i].set_bandpass(f, q, g, sr);
        }
        Self { filters }
    }

    pub fn reset(&mut self) {
        for f in &mut self.filters {
            f.reset();
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.filters[0].process(x) + self.filters[1].process(x) + self.filters[2].process(x)
    }
}
