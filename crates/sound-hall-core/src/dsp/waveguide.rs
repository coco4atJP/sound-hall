use std::f32::consts::TAU;

use super::envelope::HumanVibrato;
use super::filter::{
    Allpass1, BodyResonator, DcBlocker, FormantBank, FormantPreset, InstrumentBody, OnePoleLp,
};
use super::math::{flush_denormal, soft_limit, wrap_phase, Rng};
use super::oscillator::{PolyBlepOsc, Waveform};

/// Fractional delay line using 4-point Catmull-Rom / Hermite cubic interpolation.
#[derive(Clone, Debug)]
pub struct DelayLine {
    buf: Vec<f32>,
    write_idx: usize,
    mask: usize,
}

impl DelayLine {
    pub fn new(max_delay_samples: usize) -> Self {
        let size = (max_delay_samples.max(16) + 16).next_power_of_two();
        Self {
            buf: vec![0.0; size],
            write_idx: 0,
            mask: size - 1,
        }
    }

    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.write_idx = 0;
    }

    #[inline]
    pub fn max_delay(&self) -> usize {
        self.buf.len().saturating_sub(4)
    }

    #[inline]
    pub fn write(&mut self, sample: f32) {
        self.buf[self.write_idx] = flush_denormal(sample);
        self.write_idx = (self.write_idx + 1) & self.mask;
    }

    /// Reads at integer delay `delay_samples >= 1`.
    #[inline]
    pub fn read_int(&self, delay_samples: usize) -> f32 {
        let d = delay_samples.clamp(1, self.max_delay());
        self.buf[self.write_idx.wrapping_sub(d) & self.mask]
    }

    /// Reads at fractional delay `delay_samples >= 2.0` with 4-point Hermite cubic interpolation.
    #[inline]
    pub fn read_frac(&self, delay_samples: f32) -> f32 {
        let max_d = (self.max_delay() as f32).max(2.0);
        let delay = if delay_samples.is_finite() {
            delay_samples.clamp(2.0, max_d)
        } else {
            2.0
        };
        let idx_f = delay.floor();
        let t = delay - idx_f;
        let d = (idx_f as usize).max(2);

        let ym1 = self.buf[self.write_idx.wrapping_sub(d.saturating_sub(1)) & self.mask];
        let y0 = self.buf[self.write_idx.wrapping_sub(d) & self.mask];
        let y1 = self.buf[self.write_idx.wrapping_sub(d + 1) & self.mask];
        let y2 = self.buf[self.write_idx.wrapping_sub(d + 2) & self.mask];

        let c0 = y0;
        let c1 = 0.5 * (y1 - ym1);
        let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
        let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
        ((c3 * t + c2) * t + c1) * t + c0
    }
}

/// Extended Karplus-Strong plucked string waveguide with pick-comb, stiffness dispersion,
/// optional buzz nonlinearity (Sitar/Shamisen), and wooden body resonance.
///
/// The loop honours the requested RT60 exactly (no hidden per-period loss and no `tanh`
/// compression inside the feedback path), and the excitation is DC-free so the energy of the
/// pluck goes into the musical partials instead of a slowly decaying DC offset.
#[derive(Clone, Debug)]
pub struct PluckedStringModel {
    delay: DelayLine,
    pick_comb_buf: Vec<f32>,
    pick_comb_idx: usize,
    prev_loop: f32,
    dispersion: [Allpass1; MAX_DISPERSION_SECTIONS],
    ap_sections: usize,
    ap_coeff: f32,
    buzz_dc: DcBlocker,
    out_dc: DcBlocker,
    exc_lp: OnePoleLp,
    exc_hp: OnePoleLp,
    exc_lp_pole: f32,
    exc_hp_pole: f32,
    body: BodyResonator,
    rng: Rng,
    exc_remaining: usize,
    exc_total: usize,
    hardness: f32,
    pick_pos: f32,
    brightness: f32,
    rt60: f32,
    release_damping: f32,
    buzz: f32,
    velocity: f32,
    released: bool,
    // Loop coefficients cached per (frequency, damper state); recomputed only on change.
    cached_freq: f32,
    cached_released: bool,
    loop_b: f32,
    loop_use_ap: bool,
    loop_delay: f32,
    loop_rho: f32,
}

/// Maximum number of cascaded first-order allpass sections used for string dispersion.
pub const MAX_DISPERSION_SECTIONS: usize = 4;

/// Phase delay (samples) of the first-order allpass `(a + z^-1) / (1 + a z^-1)` at `w` rad.
#[inline]
fn allpass_phase_delay(a: f32, w: f32) -> f32 {
    if w > 1e-4 {
        1.0 - 2.0 * (a * w.sin()).atan2(1.0 + a * w.cos()) / w
    } else {
        (1.0 - a) / (1.0 + a)
    }
}

/// Phase delay (samples) of the 2-point loop FIR `(1 - b) + b z^-1` at `w` rad.
#[inline]
fn fir_phase_delay(b: f32, w: f32) -> f32 {
    if w > 1e-4 {
        (b * w.sin()).atan2((1.0 - b) + b * w.cos()) / w
    } else {
        b
    }
}

/// Designs the allpass coefficient that makes a string loop reproduce stiff-string
/// inharmonicity `f_n = n f0 sqrt(1 + B n^2) / sqrt(1 + B)` (partial 1 stays in tune).
///
/// Golden-section least-squares fit of the loop's frequency-dependent delay to the target
/// partial delays (weighted toward the perceptually dominant low partials, up to ~5 kHz).
pub fn design_dispersion(f0: f32, sr: f32, fir_b: f32, inharm_b: f32, sections: usize) -> f32 {
    let sections = sections.clamp(1, MAX_DISPERSION_SECTIONS) as f32;
    let f_max = (0.4 * sr).min(5000.0);
    let norm = (1.0 + inharm_b).sqrt();
    let partial = |n: f32| n * f0 * (1.0 + inharm_b * n * n).sqrt() / norm;
    let mut k_max = 1;
    while k_max < 16 && partial((k_max + 1) as f32) < f_max {
        k_max += 1;
    }
    if k_max < 2 || inharm_b <= 0.0 {
        return 0.0;
    }
    let w = |n: f32| TAU * partial(n) / sr;
    let target = |n: f32| n * sr / partial(n);
    let model =
        |a: f32, n: f32| fir_phase_delay(fir_b, w(n)) + sections * allpass_phase_delay(a, w(n));
    let error = |a: f32| {
        let (m1, t1) = (model(a, 1.0), target(1.0));
        let mut e = 0.0_f32;
        for k in 2..=k_max {
            let n = k as f32;
            let d = (model(a, n) - m1) - (target(n) - t1);
            e += d * d / n;
        }
        e
    };
    // Golden-section search over a in [-0.94, 0]
    let (mut lo, mut hi) = (-0.94_f32, 0.0_f32);
    let g = 0.618_034_f32;
    let mut x1 = hi - g * (hi - lo);
    let mut x2 = lo + g * (hi - lo);
    let (mut e1, mut e2) = (error(x1), error(x2));
    for _ in 0..28 {
        if e1 < e2 {
            hi = x2;
            x2 = x1;
            e2 = e1;
            x1 = hi - g * (hi - lo);
            e1 = error(x1);
        } else {
            lo = x1;
            x1 = x2;
            e1 = e2;
            x2 = lo + g * (hi - lo);
            e2 = error(x2);
        }
    }
    0.5 * (lo + hi)
}

impl PluckedStringModel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        freq: f32,
        sr: f32,
        velocity: f32,
        pick_pos: f32,
        brightness: f32,
        rt60: f32,
        release_damping: f32,
        stiffness: f32,
        buzz: f32,
        body_specs: [(f32, f32, f32); 3],
        body_dry: f32,
        body_wet: f32,
        seed: u64,
    ) -> Self {
        let sr = sr.max(8000.0);
        let freq = freq.clamp(16.0, 0.42 * sr);
        let max_delay = ((sr / 15.0) as usize).max(512);
        let period = (sr / freq).clamp(4.0, max_delay as f32);
        // Excite for exactly one string period so the loop is filled with one clean cycle
        let exc_total = period.round().clamp(4.0, max_delay as f32 - 4.0) as usize;
        let pick_comb_len = (exc_total + 16).next_power_of_two();
        let brightness = brightness.clamp(0.05, 0.99);
        let velocity = velocity.clamp(0.05, 1.0);
        // Pluck/hammer hardness: harder (and brighter) attacks put more energy in the upper
        // partials — the main timbral cue of playing dynamics on any struck/plucked string.
        let hardness = (brightness * (0.35 + 0.65 * velocity)).clamp(0.05, 1.0);
        let exc_fc = 600.0 + hardness * hardness.sqrt() * 11000.0;

        Self {
            delay: DelayLine::new(max_delay),
            pick_comb_buf: vec![0.0; pick_comb_len],
            pick_comb_idx: 0,
            prev_loop: 0.0,
            dispersion: core::array::from_fn(|_| Allpass1::new()),
            // Default: one gentle section for steel-string stiffness
            ap_sections: if stiffness > 0.02 { 1 } else { 0 },
            ap_coeff: -0.32 * stiffness.clamp(0.0, 0.95),
            // In-loop DC blocker: corner far below any string pitch so it adds no damping
            buzz_dc: DcBlocker::with_cutoff(4.0, sr),
            out_dc: DcBlocker::with_cutoff(12.0, sr),
            exc_lp: OnePoleLp::new(),
            exc_hp: OnePoleLp::new(),
            exc_lp_pole: OnePoleLp::coeff_from_cutoff(exc_fc, sr),
            // Remove DC / sub-fundamental thump from the pluck (first-order HP at f0 / 2)
            exc_hp_pole: OnePoleLp::coeff_from_cutoff(freq * 0.5, sr),
            body: BodyResonator::new(body_specs, body_dry, body_wet, sr),
            rng: Rng::new(seed),
            exc_remaining: exc_total,
            exc_total,
            hardness,
            pick_pos: pick_pos.clamp(0.04, 0.5),
            brightness,
            rt60: rt60.max(0.05),
            release_damping: release_damping.clamp(0.01, 10.0),
            buzz: buzz.clamp(0.0, 1.0),
            velocity,
            released: false,
            cached_freq: -1.0,
            cached_released: false,
            loop_b: 0.0,
            loop_use_ap: false,
            loop_delay: period,
            loop_rho: 0.99,
        }
    }

    pub fn release(&mut self) {
        self.released = true;
    }

    /// Makes the string stiff like a real piano string: designs `sections` cascaded allpasses
    /// so partials follow `f_n = n f0 sqrt(1 + B n^2)` for inharmonicity coefficient `inharm_b`.
    /// Returns the designed allpass coefficient (reusable for unison strings via
    /// [`Self::set_dispersion`]).
    pub fn set_inharmonicity(&mut self, freq: f32, sr: f32, inharm_b: f32, sections: usize) -> f32 {
        let sr = sr.max(8000.0);
        let freq = freq.clamp(16.0, 0.42 * sr);
        let b = self.loop_fir_b(sr / freq);
        let a = design_dispersion(freq, sr, b, inharm_b.clamp(0.0, 0.05), sections);
        self.set_dispersion(sections, a);
        a
    }

    /// Sets the dispersion allpass cascade directly (`sections` × coefficient `a`).
    pub fn set_dispersion(&mut self, sections: usize, a: f32) {
        self.ap_sections = sections.min(MAX_DISPERSION_SECTIONS);
        self.ap_coeff = a.clamp(-0.94, 0.0);
        self.cached_freq = -1.0;
    }

    /// Loop filter damping parameter `b` (0 = bright, 0.46 = warm).
    fn loop_fir_b(&self, period: f32) -> f32 {
        let eff_bright = (self.brightness * (0.65 + 0.35 * self.velocity)).clamp(0.05, 0.98);
        let mut b = (1.0 - eff_bright) * 0.46;
        if period < 12.0 {
            b *= (period / 12.0).clamp(0.1, 1.0);
        }
        b
    }

    /// Recomputes loop filter, dispersion, fractional delay and loss for `freq`.
    fn update_loop(&mut self, freq: f32, sr: f32) {
        self.cached_freq = freq;
        self.cached_released = self.released;
        let period = sr / freq;
        let w = TAU * freq / sr;

        let b = self.loop_fir_b(period);
        let d_fir = fir_phase_delay(b, w);

        // Dispersion allpass cascade (string stiffness)
        let use_ap = self.ap_sections > 0 && self.ap_coeff < -1e-4 && period > 10.0;
        let d_ap = if use_ap {
            self.ap_sections as f32 * allpass_phase_delay(self.ap_coeff, w)
        } else {
            0.0
        };

        // RT60 loss per period (bass rings longer, treble decays faster); the damper
        // (note-off) shortens it to `release_damping`.
        let pitch_scale = (261.63 / freq.max(40.0)).powf(0.32).clamp(0.45, 2.2);
        let target_rt60 = if self.released {
            self.rt60.min(self.release_damping)
        } else {
            self.rt60 * pitch_scale
        };

        self.loop_b = b;
        self.loop_use_ap = use_ap;
        self.loop_delay = (period - d_fir - d_ap).max(2.0);
        self.loop_rho = 0.001_f32
            .powf(1.0 / (target_rt60.max(0.015) * freq))
            .clamp(0.65, 0.99985);
    }

    #[inline]
    pub fn next_sample(&mut self, freq: f32, sr: f32) -> f32 {
        let sr = sr.max(8000.0);
        let freq = freq.clamp(16.0, 0.42 * sr);
        if (freq - self.cached_freq).abs() > 1e-3 || self.released != self.cached_released {
            self.update_loop(freq, sr);
        }

        // Excitation: one period of the string's initial state, blending
        //  - displacement: triangle with apex at the pick point (partials ~ sin(n*pi*b)/n^2,
        //    soft finger / felt character), and
        //  - velocity: zero-mean pulse of duty b (partials ~ sin(n*pi*b)/n, pick / hard
        //    hammer character),
        // by attack hardness, plus a pick-point-combed noise burst for grit. Everything is
        // hardness low-passed and DC-free; pick-position notches come out naturally.
        let mut exc = 0.0_f32;
        if self.exc_remaining > 0 {
            let beta = self.pick_pos;
            let prog = 1.0 - (self.exc_remaining as f32 / self.exc_total as f32);
            // Start at the triangle's rising zero crossing so the onset is click-free
            let x = wrap_phase(prog + beta * 0.5);
            let tri = if x < beta {
                x / beta
            } else {
                (1.0 - x) / (1.0 - beta)
            };
            let pulse = if x < beta { 1.0 - beta } else { -beta };
            let h = self.hardness;
            let shape = (2.0 * tri - 1.0) * (1.0 - h) + pulse * 2.4 * h;

            let noise = self.rng.next_f32_bipolar() * 0.55 + self.rng.next_pink() * 0.38;
            let mask = self.pick_comb_buf.len() - 1;
            let pick_d = ((sr / freq * beta).round() as usize).clamp(1, mask);
            self.pick_comb_buf[self.pick_comb_idx] = noise;
            let delayed_pick = self.pick_comb_buf[self.pick_comb_idx.wrapping_sub(pick_d) & mask];
            self.pick_comb_idx = (self.pick_comb_idx + 1) & mask;
            let combed_noise = (noise - delayed_pick) * 0.5;

            let grit = 0.12 + 0.28 * self.brightness * self.velocity;
            let raw = shape * (1.0 - grit) + combed_noise * grit;
            let lp = self.exc_lp.process_pole(raw, self.exc_lp_pole);
            let shaped = lp - self.exc_hp.process_pole(lp, self.exc_hp_pole);
            exc = shaped * self.velocity * 0.55;
            self.exc_remaining -= 1;
        }

        let delayed = self.delay.read_frac(self.loop_delay);

        // 2-point FIR lowpass
        let b = self.loop_b;
        let lp = (1.0 - b) * delayed + b * self.prev_loop;
        self.prev_loop = delayed;

        // Dispersion allpass cascade
        let mut filtered = lp;
        if self.loop_use_ap {
            let a = self.ap_coeff;
            for ap in &mut self.dispersion[..self.ap_sections] {
                filtered = ap.process(filtered, a);
            }
        }

        // Optional buzz bridge (Sitar jawari / Shamisen sawari): the string grazes a curved
        // bridge on one side. Modelled as a *passive* one-sided soft contact (|y| <= |x|), so
        // it spreads energy into bright buzzing partials without ever adding energy — the
        // string still decays naturally and the buzz grows with playing strength.
        if self.buzz > 0.01 {
            let t = 0.10 * (1.0 - 0.6 * self.buzz);
            if filtered < -t {
                filtered = -t + (filtered + t) * (1.0 - 0.5 * self.buzz);
            }
            filtered = self.buzz_dc.process(filtered);
        }

        let loop_val = soft_limit(filtered * self.loop_rho + exc);
        self.delay.write(loop_val);

        self.out_dc.process(self.body.process(loop_val))
    }
}

/// Violin body template: `(frequency Hz, Q, gain dB)` of the main radiating features —
/// A0 air (Helmholtz) mode, CBR, the strong B1-/B1+ corpus modes, the ~1.2 kHz dip and the
/// 2-3 kHz "bridge hill". Lower-family instruments scale it by their `body_scale`.
const VIOLIN_BODY: [(f32, f32, f32); 8] = [
    (275.0, 9.0, 6.0),
    (405.0, 7.0, 2.0),
    (465.0, 11.0, 6.0),
    (545.0, 9.0, 6.0),
    (720.0, 6.0, 2.0),
    (1250.0, 1.8, -1.5),
    (2700.0, 0.9, 12.0),
    (3700.0, 3.0, 3.0),
];

/// Builds a violin-family body: modes up to ~1 kHz scale with body size, the bridge hill and
/// the radiation roll-off scale more gently (bridge and string mass scale less than the box).
fn violin_family_body(scale: f32, section: bool, sr: f32) -> InstrumentBody {
    let s = scale.clamp(0.15, 1.5);
    let mut peaks = [(0.0_f32, 1.0_f32, 0.0_f32); 8];
    for (dst, &(f, q, g)) in peaks.iter_mut().zip(VIOLIN_BODY.iter()) {
        let fs = if f < 1000.0 { f * s } else { f * s.sqrt() };
        // A section averages many bodies: broader, shallower resonances
        *dst = if section {
            (fs, q * 0.55, g * 0.7)
        } else {
            (fs, q, g)
        };
    }
    InstrumentBody::new(230.0 * s, 5500.0 * s.powf(0.4), &peaks, sr)
}

/// One player of a bowed-string voice: band-limited Helmholtz sawtooth with its own tuning,
/// human vibrato, bow-change timing and stage position.
#[derive(Clone, Debug)]
struct BowPlayer {
    osc: PolyBlepOsc,
    corner_lp: OnePoleLp,
    corner_lp2: OnePoleLp,
    vibrato: HumanVibrato,
    detune: f32,
    onset_samples: f32,
    elapsed_samples: f32,
    gain_l: f32,
    gain_r: f32,
}

/// Bowed string: Helmholtz stick-slip motion makes the bridge force a sawtooth whose corners
/// round off with lighter bowing. A band-limited sawtooth (bow force/speed controlling the
/// corner rounding, slip-synchronous rosin noise) drives a resonant string loop (solo) and a
/// measured violin-family body (A0/B1 modes, bridge hill) — or, for ensembles, a section of
/// four independently tuned, vibrato'd and timed players spread across the stage.
#[derive(Clone, Debug)]
pub struct BowedStringModel {
    delay: DelayLine,
    prev_loop: f32,
    players: Vec<BowPlayer>,
    dc_l: DcBlocker,
    dc_r: DcBlocker,
    rosin_lp: OnePoleLp,
    body_l: InstrumentBody,
    body_r: Option<InstrumentBody>,
    rng: Rng,
    bow_pressure: f32,
    tilt_hz: f32,
    rosin_noise: f32,
}

impl BowedStringModel {
    /// Compensates the body's resonance gain (bridge hill ~ +12 dB) so the raw output stays
    /// well inside the voice's linear range.
    const BODY_TRIM: f32 = 0.22;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sr: f32,
        bow_pressure: f32,
        brightness: f32,
        rosin_noise: f32,
        ensemble_detune: f32,
        body_scale: f32,
        vibrato_rate: f32,
        seed: u64,
    ) -> Self {
        let sr = sr.max(8000.0);
        let max_delay = ((sr / 15.0) as usize).max(512);
        let mut rng = Rng::new(seed);
        let section = ensemble_detune > 0.0005;
        let ensemble_detune = ensemble_detune.clamp(0.0, 0.05);

        // (detune, onset ms, pan) per desk; random jitter keeps every note different
        const DESKS: [(f32, f32, f32); 4] = [
            (0.0, 0.0, -0.35),
            (0.7, 9.0, 0.30),
            (-0.9, 17.0, -0.10),
            (0.4, 26.0, 0.45),
        ];
        let count = if section { 4 } else { 1 };
        let mut players = Vec::with_capacity(count);
        for (i, &(det, onset_ms, pan)) in DESKS.iter().enumerate().take(count) {
            let jitter = 0.7 + 0.6 * rng.next_f32_unipolar();
            let detune = if section {
                1.0 + det * ensemble_detune * jitter
            } else {
                1.0
            };
            let onset = if section {
                (onset_ms + 5.0 * rng.next_f32_bipolar()).max(0.0) * 0.001 * sr
            } else {
                0.0
            };
            let pan = if section { pan } else { 0.0 };
            let vib_rate = vibrato_rate * (0.93 + 0.14 * rng.next_f32_unipolar());
            let vib_delay = 0.16 + 0.18 * rng.next_f32_unipolar();
            players.push(BowPlayer {
                osc: PolyBlepOsc::new(seed ^ ((i as u64 + 1) * 0x2545_F491))
                    .with_phase(if i == 0 { 0.0 } else { rng.next_f32_unipolar() }),
                corner_lp: OnePoleLp::new(),
                corner_lp2: OnePoleLp::new(),
                vibrato: HumanVibrato::new(
                    vib_rate,
                    vib_delay,
                    0.35,
                    seed ^ ((i as u64 + 7) * 977),
                ),
                detune,
                onset_samples: onset,
                elapsed_samples: 0.0,
                gain_l: 1.0 - pan * 0.6,
                gain_r: 1.0 + pan * 0.6,
            });
        }

        Self {
            delay: DelayLine::new(max_delay),
            prev_loop: 0.0,
            players,
            dc_l: DcBlocker::with_cutoff(15.0, sr),
            dc_r: DcBlocker::with_cutoff(15.0, sr),
            rosin_lp: OnePoleLp::new(),
            body_l: violin_family_body(body_scale, section, sr),
            body_r: if section {
                Some(violin_family_body(body_scale, true, sr))
            } else {
                None
            },
            rng,
            bow_pressure: bow_pressure.clamp(0.1, 1.0),
            tilt_hz: 1500.0 + 5000.0 * brightness.clamp(0.1, 0.98),
            rosin_noise: rosin_noise.clamp(0.0, 0.5),
        }
    }

    /// Renders one stereo sample. `vib_depth_cents` is the (mod-wheel adjusted) vibrato depth.
    #[inline]
    pub fn next_sample(
        &mut self,
        freq: f32,
        sr: f32,
        bow_env: f32,
        velocity: f32,
        vib_depth_cents: f32,
    ) -> (f32, f32) {
        let sr = sr.max(8000.0);
        let freq = freq.clamp(18.0, 0.40 * sr);

        // Bow force/speed: harder bowing sharpens the Helmholtz corners (more brilliance)
        let dynamics = bow_env * (0.35 + 0.65 * velocity) * (0.6 + 0.4 * self.bow_pressure);
        let k = ((dynamics - 0.2) / 0.6).clamp(0.0, 1.0);
        let corner_hz = (self.tilt_hz * (0.3 + 1.2 * k)).clamp(300.0, 0.45 * sr);
        let corner_pole = OnePoleLp::coeff_from_cutoff(corner_hz, sr);

        let white = self.rng.next_f32_bipolar();
        let rosin_raw = self.rosin_lp.process_cutoff(white, 3200.0, sr);
        // More scratch while the Helmholtz motion is still establishing itself
        let scratch =
            self.rosin_noise * (0.5 + 0.5 * velocity) * (1.0 + 2.5 * (1.0 - bow_env).max(0.0));

        let solo = self.players.len() == 1;
        let mut sum_l = 0.0_f32;
        let mut sum_r = 0.0_f32;
        let mut lead_freq = freq;
        for (i, p) in self.players.iter_mut().enumerate() {
            p.elapsed_samples += 1.0;
            let vib = p.vibrato.next(sr) * vib_depth_cents;
            let gate = if solo {
                1.0
            } else {
                ((p.elapsed_samples - p.onset_samples) / (0.02 * sr)).clamp(0.0, 1.0)
            };
            if gate <= 0.0 {
                continue;
            }
            let f = freq * p.detune * (vib * (core::f32::consts::LN_2 / 1200.0)).exp();
            if i == 0 {
                lead_freq = f;
            }
            let dt = PolyBlepOsc::dt(f, sr);
            // Falling sawtooth = Helmholtz bridge force
            let saw = -p.osc.next_sample(Waveform::Saw, dt);
            // Rosin noise bursts at each slip (phase ~0), plus steady bow-hair hiss
            let ph = p.osc.phase();
            let slip_dist = if ph > 0.5 { 1.0 - ph } else { ph };
            let slip_win = (-(slip_dist * 12.0) * (slip_dist * 12.0)).exp();
            let noise = rosin_raw * scratch * (0.3 + 1.4 * slip_win);
            let rounded = p.corner_lp.process_pole(saw, corner_pole);
            let drive = p.corner_lp2.process_pole(rounded, corner_pole) + noise;
            let d = drive * gate * (0.35 + 0.65 * velocity) * bow_env;
            sum_l += d * p.gain_l;
            sum_r += d * p.gain_r;
        }

        if solo {
            // Resonant string loop tracking the played (vibrato) pitch; bow damping keeps it
            // well below self-oscillation
            let w = TAU * lead_freq / sr;
            let b = 0.08;
            let d1 = (sr / lead_freq - fir_phase_delay(b, w)).max(2.0);
            let str_vel = self.delay.read_frac(d1);
            let lp = (1.0 - b) * str_vel + b * self.prev_loop;
            self.prev_loop = str_vel;
            let loop_val = soft_limit(lp * 0.6 + sum_l * 0.25);
            self.delay.write(loop_val);
            let y = self.body_l.process(self.dc_l.process(loop_val)) * Self::BODY_TRIM;
            (y, y)
        } else {
            let norm = 0.5 * Self::BODY_TRIM; // 1 / sqrt(4 players)
            let l = self.body_l.process(self.dc_l.process(sum_l * norm));
            let r = match self.body_r.as_mut() {
                Some(body) => body.process(self.dc_r.process(sum_r * norm)),
                None => l,
            };
            (l, r)
        }
    }
}

/// Aerodynamic / reed / brass lip wind bore type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindType {
    /// Edge-tone air jet (Flute, Piccolo, Shakuhachi, Pan Flute, Ocarina)
    FluteJet,
    /// Cylindrical or conical single reed (Clarinet odd-harmonic / Saxophone full-harmonic)
    SingleReed,
    /// Pinched nasal double reed (Oboe, English Horn, Bassoon, Shanai, Bagpipe)
    DoubleReed,
    /// Flared brass lip reed (Trumpet, Trombone, French Horn, Tuba, Muted Trumpet)
    LipReed,
}

/// Instrument-specific voicing of a wind source: spectral roll-off, its dependence on
/// playing dynamics, bell/tone-hole radiation and the fixed formants of mouthpiece, bore and
/// bell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindVoicing {
    /// Spectral roll-off corner (Hz) of the excitation at mezzo-forte.
    pub tilt_hz: f32,
    /// How strongly dynamics open up the spectrum: 0 = constant timbre, 1 = brass "blare"
    /// (roll-off corner ~0.2x at pp to ~1.8x at ff).
    pub dynamic_tilt: f32,
    /// Radiation high-pass (Hz): bells and tone holes radiate low fundamentals weakly.
    pub radiation_hp_hz: f32,
    /// Fixed formant resonances `(Hz, Q, gain dB)`; zero-gain entries are ignored.
    pub formants: [(f32, f32, f32); 3],
}

impl WindVoicing {
    pub const fn new(
        tilt_hz: f32,
        dynamic_tilt: f32,
        radiation_hp_hz: f32,
        formants: [(f32, f32, f32); 3],
    ) -> Self {
        Self {
            tilt_hz,
            dynamic_tilt,
            radiation_hp_hz,
            formants,
        }
    }
}

/// Wind instrument model covering flutes, reeds and brass.
///
/// A band-limited excitation matching the generator physics — lip-reed and conical-reed
/// flow pulses (all harmonics), cylindrical clarinet bore (odd harmonics), pinched double reed
/// (narrow pulse), flute jet (soft, few harmonics) — has a spectral roll-off that opens up
/// with breath pressure (the brass "blare", reed edge). It excites a light resonant bore loop
/// (attack build-up, odd-mode reinforcement for the clarinet), then the instrument's
/// radiation filter and formants. Breath turbulence, attack chiff/lip buzz and pitch scoops
/// complete the articulation.
#[derive(Clone, Debug)]
pub struct WindBoreModel {
    wind_type: WindType,
    bore_delay: DelayLine,
    refl_lp: OnePoleLp,
    tilt_lp1: OnePoleLp,
    tilt_lp2: OnePoleLp,
    breath_lp: OnePoleLp,
    dc_blocker: DcBlocker,
    formants: Option<FormantBank>,
    voicing_body: InstrumentBody,
    osc: PolyBlepOsc,
    even_osc: PolyBlepOsc,
    even_hp: OnePoleLp,
    rng: Rng,
    elapsed: f32,
    cylindrical_odd: bool,
    breath_noise: f32,
    brightness: f32,
    scoop_cents: f32,
    scoop_time: f32,
    formant_mix: f32,
    tilt_hz: f32,
    dynamic_tilt: f32,
}

impl WindBoreModel {
    /// Keeps the raw output inside the voice's linear range after formant gain.
    const OUTPUT_TRIM: f32 = 0.45;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        wind_type: WindType,
        sr: f32,
        cylindrical_odd: bool,
        breath_noise: f32,
        brightness: f32,
        scoop_cents: f32,
        scoop_time: f32,
        formant: Option<FormantPreset>,
        formant_mix: f32,
        voicing: WindVoicing,
        seed: u64,
    ) -> Self {
        let sr = sr.max(8000.0);
        let max_delay = ((sr / 15.0) as usize).max(512);
        let formants = formant.map(|f| FormantBank::new(f, sr));
        Self {
            wind_type,
            bore_delay: DelayLine::new(max_delay),
            refl_lp: OnePoleLp::new(),
            tilt_lp1: OnePoleLp::new(),
            tilt_lp2: OnePoleLp::new(),
            breath_lp: OnePoleLp::new(),
            dc_blocker: DcBlocker::with_cutoff(15.0, sr),
            formants,
            voicing_body: InstrumentBody::new(voicing.radiation_hp_hz, 0.0, &voicing.formants, sr),
            osc: PolyBlepOsc::new(seed ^ 0x0B0E),
            even_osc: PolyBlepOsc::new(seed ^ 0x0E0E),
            even_hp: OnePoleLp::new(),
            rng: Rng::new(seed),
            elapsed: 0.0,
            cylindrical_odd,
            breath_noise: breath_noise.clamp(0.0, 0.8),
            brightness: brightness.clamp(0.1, 1.0),
            scoop_cents: scoop_cents.clamp(-200.0, 200.0),
            scoop_time: scoop_time.max(0.005),
            formant_mix: formant_mix.clamp(0.0, 1.0),
            tilt_hz: voicing.tilt_hz.max(100.0),
            dynamic_tilt: voicing.dynamic_tilt.clamp(0.0, 1.0),
        }
    }

    #[inline]
    pub fn next_sample(&mut self, base_freq: f32, sr: f32, breath_env: f32, velocity: f32) -> f32 {
        let sr = sr.max(8000.0);
        let dt = 1.0 / sr;
        self.elapsed += dt;

        // Optional expressive pitch scoop on attack (Shakuhachi / Pan Flute / Jazz Brass)
        let freq = if self.scoop_cents.abs() > 0.5 && self.elapsed < self.scoop_time {
            let t = 1.0 - self.elapsed / self.scoop_time;
            let cents = self.scoop_cents * t * t;
            base_freq * 2.0_f32.powf(cents / 1200.0)
        } else {
            base_freq
        };
        let freq = freq.clamp(20.0, 0.40 * sr);

        // Breath pressure / lip tension: higher dynamics open up the spectrum. Within a note
        // this follows the envelope, so attacks bloom from dark to bright like real winds.
        let dynamics = breath_env * (0.35 + 0.65 * velocity);
        let kd = (dynamics / 0.65).max(0.0).powf(1.8);
        let tilt_scale = (1.0 - self.dynamic_tilt + self.dynamic_tilt * kd).clamp(0.15, 2.2);
        let tilt_hz = (self.tilt_hz * tilt_scale).clamp(80.0, 0.45 * sr);
        let tilt_pole = OnePoleLp::coeff_from_cutoff(tilt_hz, sr);

        // Band-limited excitation by generator type
        let osc_dt = PolyBlepOsc::dt(freq, sr);
        let mut even_part = 0.0_f32;
        let raw = match self.wind_type {
            WindType::LipReed => -self.osc.next_sample(Waveform::Saw, osc_dt),
            WindType::SingleReed if self.cylindrical_odd => {
                // Cylindrical bore: odd partials dominate below the tone-hole lattice cutoff
                // (~1.5 kHz); above it the even partials return.
                let odd = self.osc.next_sample(Waveform::Square, osc_dt);
                let saw = -self.even_osc.next_sample(Waveform::Saw, osc_dt);
                let low = self.even_hp.process_cutoff(saw, 1500.0, sr);
                // Even partials bypass the odd-mode bore so they survive above the cutoff
                even_part = (saw - low) * 0.55 + saw * 0.05;
                odd
            }
            WindType::SingleReed => -self.osc.next_sample(Waveform::Saw, osc_dt),
            // Pinched double reed: rich flow pulses; the nasal colour comes from formants
            WindType::DoubleReed => -self.osc.next_sample(Waveform::Saw, osc_dt),
            WindType::FluteJet => -self.osc.next_sample(Waveform::Saw, osc_dt),
        };
        let mut src = self.tilt_lp1.process_pole(raw, tilt_pole);
        if self.wind_type == WindType::FluteJet {
            // Edge-tone jets are nearly sinusoidal: steeper roll-off
            src = self.tilt_lp2.process_pole(src, tilt_pole);
        }

        let breath_raw = self.rng.next_f32_bipolar();
        let breath_filt =
            self.breath_lp
                .process_cutoff(breath_raw, 2200.0 + self.brightness * 3500.0, sr);
        let drive_amp = breath_env * (0.35 + 0.65 * velocity);

        // Attack articulation: flute chiff (turbulence before the jet locks) and lip buzz
        let onset = (1.0 - self.elapsed / 0.04).max(0.0);
        let breath_level = match self.wind_type {
            WindType::FluteJet => 0.55 + 1.2 * onset,
            WindType::LipReed => 0.25 + 0.8 * onset,
            _ => 0.3 + 0.5 * onset,
        };
        let breath = breath_filt * self.breath_noise * breath_level;
        let excitation = (src + breath) * drive_amp;

        // Light resonant bore: reinforces the bore modes and gives a natural build-up.
        // Cylindrical bores reflect with inversion over half a period (odd modes only).
        let lp_hz = match self.wind_type {
            WindType::FluteJet => 1800.0 + self.brightness * 4200.0,
            WindType::SingleReed => 1400.0 + self.brightness * 5500.0 + velocity * 1500.0,
            WindType::DoubleReed => 1800.0 + self.brightness * 5200.0,
            WindType::LipReed => 900.0 + (self.brightness * 0.5 + velocity * 0.5) * 6500.0,
        };
        let lp_pole = OnePoleLp::coeff_from_cutoff(lp_hz, sr);
        let w = TAU * freq / sr;
        let d_lp = if w > 1e-4 {
            (lp_pole * w.sin()).atan2(1.0 - lp_pole * w.cos()) / w
        } else {
            lp_pole / (1.0 - lp_pole).max(1e-4)
        };
        let (period_scale, refl_sign) =
            if self.wind_type == WindType::SingleReed && self.cylindrical_odd {
                (0.5, -1.0)
            } else {
                (1.0, 1.0)
            };
        let d_bore = ((sr / freq) * period_scale - d_lp).max(2.0);
        let bore = self.bore_delay.read_frac(d_bore);
        let refl = self.refl_lp.process_pole(bore, lp_pole) * refl_sign;
        let out = soft_limit(refl * 0.45 + excitation * 0.55);
        self.bore_delay.write(out);

        let lip_buzz = if self.wind_type == WindType::LipReed && self.elapsed < 0.035 {
            (1.0 - self.elapsed / 0.035) * 0.12 * drive_amp * breath_raw
        } else {
            0.0
        };

        let even = self.tilt_lp2.process_pole(even_part, tilt_pole) * drive_amp * 1.8;
        let clean = self.dc_blocker.process(out + lip_buzz + even);
        let voiced = self.voicing_body.process(clean);
        let y = if let Some(ref mut fmt) = self.formants {
            let f_out = fmt.process(voiced);
            voiced * (1.0 - self.formant_mix) + f_out * self.formant_mix * 1.4
        } else {
            voiced
        };
        y * Self::OUTPUT_TRIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plucked_string_is_tuned_and_decays() {
        let sr = 44100.0;
        let mut s = PluckedStringModel::new(
            440.0,
            sr,
            0.9,
            0.22,
            0.78,
            1.8,
            0.08,
            0.2,
            0.0,
            [(200.0, 3.0, 0.3), (450.0, 4.0, 0.2), (0.0, 1.0, 0.0)],
            0.8,
            0.25,
            11,
        );
        let mut peak = 0.0_f32;
        for _ in 0..44100 {
            let y = s.next_sample(440.0, sr);
            assert!(y.is_finite());
            peak = peak.max(y.abs());
        }
        assert!(peak > 0.05);
        s.release();
        for _ in 0..22050 {
            s.next_sample(440.0, sr);
        }
        assert!(s.next_sample(440.0, sr).abs() < 0.02);
    }
}
