use std::f32::consts::TAU;

use super::filter::{Allpass1, BodyResonator, DcBlocker, FormantBank, FormantPreset, OnePoleLp};
use super::math::{flush_denormal, soft_clip, soft_limit, wrap_phase, Rng};

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
    dispersion: Allpass1,
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
    stiffness: f32,
    buzz: f32,
    velocity: f32,
    released: bool,
    // Loop coefficients cached per (frequency, damper state); recomputed only on change.
    cached_freq: f32,
    cached_released: bool,
    loop_b: f32,
    loop_ap: f32,
    loop_use_ap: bool,
    loop_delay: f32,
    loop_rho: f32,
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
            dispersion: Allpass1::new(),
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
            stiffness: stiffness.clamp(0.0, 0.95),
            buzz: buzz.clamp(0.0, 1.0),
            velocity,
            released: false,
            cached_freq: -1.0,
            cached_released: false,
            loop_b: 0.0,
            loop_ap: 0.0,
            loop_use_ap: false,
            loop_delay: period,
            loop_rho: 0.99,
        }
    }

    pub fn release(&mut self) {
        self.released = true;
    }

    /// Recomputes loop filter, dispersion, fractional delay and loss for `freq`.
    fn update_loop(&mut self, freq: f32, sr: f32) {
        self.cached_freq = freq;
        self.cached_released = self.released;
        let period = sr / freq;
        let w = TAU * freq / sr;

        // Loop filter damping parameter `b` (0 = bright, 0.46 = warm)
        let eff_bright = (self.brightness * (0.65 + 0.35 * self.velocity)).clamp(0.05, 0.98);
        let mut b = (1.0 - eff_bright) * 0.46;
        if period < 12.0 {
            b *= (period / 12.0).clamp(0.1, 1.0);
        }
        // Exact phase delay of 2-point FIR (1 - b) + b * z^-1
        let d_fir = if w > 1e-4 {
            (b * w.sin()).atan2((1.0 - b) + b * w.cos()) / w
        } else {
            b
        };

        // Optional dispersion allpass for stiff steel strings
        let use_ap = self.stiffness > 0.02 && period > 10.0;
        let a_ap = if use_ap { -0.32 * self.stiffness } else { 0.0 };
        let d_ap = if use_ap {
            if w > 1e-4 {
                1.0 - 2.0 * (a_ap * w.sin()).atan2(1.0 + a_ap * w.cos()) / w
            } else {
                (1.0 - a_ap) / (1.0 + a_ap)
            }
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
        self.loop_ap = a_ap;
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

        // Dispersion allpass
        let mut filtered = if self.loop_use_ap {
            self.dispersion.process(lp, self.loop_ap)
        } else {
            lp
        };

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

/// Physical bowed string waveguide combining slip-stick Helmholtz bow friction,
/// fractional delay string loop, optional ensemble layer, and wooden string body formants.
#[derive(Clone, Debug)]
pub struct BowedStringModel {
    delay: DelayLine,
    delay2: Option<DelayLine>,
    prev_loop: f32,
    prev_loop2: f32,
    dc_blocker: DcBlocker,
    rosin_lp: OnePoleLp,
    body: BodyResonator,
    rng: Rng,
    drive_phase: f32,
    drive_phase2: f32,
    bow_pressure: f32,
    brightness: f32,
    rosin_noise: f32,
    ensemble_detune: f32,
}

impl BowedStringModel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sr: f32,
        bow_pressure: f32,
        brightness: f32,
        rosin_noise: f32,
        ensemble_detune: f32,
        body_specs: [(f32, f32, f32); 3],
        body_dry: f32,
        body_wet: f32,
        seed: u64,
    ) -> Self {
        let sr = sr.max(8000.0);
        let max_delay = ((sr / 15.0) as usize).max(512);
        let delay2 = if ensemble_detune > 0.001 {
            Some(DelayLine::new(max_delay))
        } else {
            None
        };
        Self {
            delay: DelayLine::new(max_delay),
            delay2,
            prev_loop: 0.0,
            prev_loop2: 0.0,
            dc_blocker: DcBlocker::new(0.995),
            rosin_lp: OnePoleLp::new(),
            body: BodyResonator::new(body_specs, body_dry, body_wet, sr),
            rng: Rng::new(seed),
            drive_phase: 0.0,
            drive_phase2: 0.37,
            bow_pressure: bow_pressure.clamp(0.1, 1.0),
            brightness: brightness.clamp(0.1, 0.98),
            rosin_noise: rosin_noise.clamp(0.0, 0.5),
            ensemble_detune: ensemble_detune.clamp(0.0, 0.05),
        }
    }

    #[inline]
    pub fn next_sample(&mut self, freq: f32, sr: f32, bow_env: f32, velocity: f32) -> f32 {
        let sr = sr.max(8000.0);
        let freq = freq.clamp(18.0, 0.40 * sr);
        let w = TAU * freq / sr;
        let b = (1.0 - self.brightness * (0.65 + 0.35 * velocity)).clamp(0.04, 0.45);
        let d_fir = if w > 1e-4 {
            (b * w.sin()).atan2((1.0 - b) + b * w.cos()) / w
        } else {
            b
        };
        let d1 = (sr / freq - d_fir).max(2.0);
        let str_vel = self.delay.read_frac(d1);

        // Helmholtz stick-slip phase-locked excitation + friction reflection
        self.drive_phase = wrap_phase(self.drive_phase + freq / sr);
        // Falling sawtooth `(1 - 2*phase)` aligns in-phase with `+sin(TAU*phase)` on fundamental
        let saw_slip =
            (1.0 - 2.0 * self.drive_phase) * 0.55 + (self.drive_phase * TAU).sin() * 0.45;
        let rosin = self
            .rosin_lp
            .process_cutoff(self.rng.next_f32_bipolar(), 2800.0, sr)
            * self.rosin_noise;

        let bow_vel = bow_env * (0.35 + 0.65 * velocity);
        let v_diff = -str_vel;
        let mu = 0.25 + 0.75 / (1.0 + 3.2 * v_diff.abs());
        let friction_force = (v_diff * mu * self.bow_pressure * 0.16 * bow_env).clamp(-0.25, 0.25);

        let drive = (saw_slip * (0.16 + 0.18 * self.bow_pressure) + rosin) * bow_vel;
        let lp1 = (1.0 - b) * str_vel + b * self.prev_loop;
        self.prev_loop = str_vel;

        let loop1 = soft_clip(lp1 * 0.85 + drive + friction_force);
        self.delay.write(loop1);

        let mut out = loop1;
        if let Some(ref mut dline2) = self.delay2 {
            let freq2 = freq * (1.0 + self.ensemble_detune);
            let d2 = (sr / freq2 - d_fir).max(2.0);
            let s2 = dline2.read_frac(d2);
            self.drive_phase2 = wrap_phase(self.drive_phase2 + freq2 / sr);
            let slip2 =
                (1.0 - 2.0 * self.drive_phase2) * 0.5 + (self.drive_phase2 * TAU).sin() * 0.5;
            let lp2 = (1.0 - b) * s2 + b * self.prev_loop2;
            self.prev_loop2 = s2;
            let loop2 = soft_clip(lp2 * 0.85 + slip2 * 0.20 * bow_vel);
            dline2.write(loop2);
            out = (out + loop2 * 0.85) * 0.62;
        }

        let clean = self.dc_blocker.process(out);
        self.body.process(clean)
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

/// Physical wind instrument waveguide model covering flutes, reeds, and brass.
#[derive(Clone, Debug)]
pub struct WindBoreModel {
    wind_type: WindType,
    bore_delay: DelayLine,
    jet_delay: DelayLine,
    refl_lp: OnePoleLp,
    breath_lp: OnePoleLp,
    dc_blocker: DcBlocker,
    formants: Option<FormantBank>,
    rng: Rng,
    phase: f32,
    elapsed: f32,
    cylindrical_odd: bool,
    breath_noise: f32,
    brightness: f32,
    scoop_cents: f32,
    scoop_time: f32,
    formant_mix: f32,
}

impl WindBoreModel {
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
        seed: u64,
    ) -> Self {
        let sr = sr.max(8000.0);
        let max_delay = ((sr / 15.0) as usize).max(512);
        let formants = formant.map(|f| FormantBank::new(f, sr));
        Self {
            wind_type,
            bore_delay: DelayLine::new(max_delay),
            jet_delay: DelayLine::new(max_delay / 2 + 16),
            refl_lp: OnePoleLp::new(),
            breath_lp: OnePoleLp::new(),
            dc_blocker: DcBlocker::new(0.995),
            formants,
            rng: Rng::new(seed),
            phase: 0.0,
            elapsed: 0.0,
            cylindrical_odd,
            breath_noise: breath_noise.clamp(0.0, 0.8),
            brightness: brightness.clamp(0.1, 1.0),
            scoop_cents: scoop_cents.clamp(-200.0, 200.0),
            scoop_time: scoop_time.max(0.005),
            formant_mix: formant_mix.clamp(0.0, 1.0),
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
        let w = TAU * freq / sr;

        // Reflection filter cutoff scales with brightness and velocity
        let lp_hz = match self.wind_type {
            WindType::FluteJet => 1800.0 + self.brightness * 4200.0,
            WindType::SingleReed => 1400.0 + self.brightness * 5500.0 + velocity * 1500.0,
            WindType::DoubleReed => 1800.0 + self.brightness * 5200.0,
            WindType::LipReed => 900.0 + (self.brightness * 0.5 + velocity * 0.5) * 6500.0,
        };
        let lp_pole = OnePoleLp::coeff_from_cutoff(lp_hz, sr);
        let d_lp = if w > 1e-4 {
            (lp_pole * w.sin()).atan2(1.0 - lp_pole * w.cos()) / w
        } else {
            lp_pole / (1.0 - lp_pole).max(1e-4)
        };

        self.phase = wrap_phase(self.phase + freq / sr);
        let breath_raw = self.rng.next_f32_bipolar();
        let breath_filt =
            self.breath_lp
                .process_cutoff(breath_raw, 2200.0 + self.brightness * 3500.0, sr);
        let drive_amp = breath_env * (0.35 + 0.65 * velocity);

        let out = match self.wind_type {
            WindType::FluteJet => {
                let d_bore = (sr / freq - d_lp).max(2.0);
                let bore = self.bore_delay.read_frac(d_bore);
                let refl = self.refl_lp.process_pole(bore, lp_pole);

                let d_jet = (d_bore * 0.44).max(2.0);
                let jet_in = soft_clip((drive_amp * 0.72 + refl * 0.55) * 1.1);
                self.jet_delay.write(jet_in);
                let j = self.jet_delay.read_frac(d_jet);
                // Cubic edge-tone nonlinearity
                let jet_nl = (j - j * j * j).clamp(-0.9, 0.9);

                // Gentle phase-locked air oscillation ensures instant, stable start at exact pitch
                let seed = (self.phase * TAU).sin() * 0.24 * drive_amp;
                let air_turbulence =
                    breath_filt * self.breath_noise * (0.28 + 0.55 * (1.0 - breath_env).max(0.15));
                let next_bore =
                    soft_clip(refl * 0.84 + jet_nl * 0.22 + seed + air_turbulence * drive_amp);
                self.bore_delay.write(next_bore);
                next_bore + air_turbulence * drive_amp * 0.45
            }
            WindType::SingleReed => {
                let period_scale = if self.cylindrical_odd { 0.5 } else { 1.0 };
                let refl_sign = if self.cylindrical_odd { -1.0 } else { 1.0 };
                let d_bore = ((sr / freq) * period_scale - d_lp).max(2.0);
                let bore = self.bore_delay.read_frac(d_bore);
                let refl = self.refl_lp.process_pole(bore, lp_pole) * refl_sign;

                // Single-reed pressure saturation + harmonic reed pulse
                let p_diff = drive_amp * 0.7 - refl;
                let reed_curve = soft_clip(p_diff * 1.6) * 0.20;
                let harmonic_seed = if self.cylindrical_odd {
                    // Odd-harmonic pulse
                    let s1 = (self.phase * TAU).sin();
                    let s3 = (self.phase * TAU * 3.0).sin() * 0.30;
                    (s1 + s3) * 0.22 * drive_amp
                } else {
                    // Conical saxophone pulse (all harmonics, in-phase with fundamental)
                    let saw = 1.0 - 2.0 * self.phase;
                    let sin = (self.phase * TAU).sin();
                    (sin * 0.6 + saw * 0.4) * 0.22 * drive_amp
                };
                let breath = breath_filt * self.breath_noise * 0.25 * drive_amp;
                let next_bore = soft_clip(refl * 0.84 + reed_curve + harmonic_seed + breath);
                self.bore_delay.write(next_bore);
                next_bore
            }
            WindType::DoubleReed => {
                let d_bore = (sr / freq - d_lp).max(2.0);
                let bore = self.bore_delay.read_frac(d_bore);
                let refl = self.refl_lp.process_pole(bore, lp_pole);

                // Pinched double-reed asymmetric pulse rich in fundamental & nasal partials
                let reed_pulse = (self.phase * TAU).sin() * 0.78
                    + (self.phase * TAU * 2.0).sin() * 0.36
                    + (self.phase * TAU * 3.0).sin() * 0.28
                    + (self.phase * TAU * 4.0).sin() * 0.16;
                let asym = soft_clip((drive_amp * 0.6 - refl) * 1.8) * 0.15;
                let breath = breath_filt * self.breath_noise * 0.18 * drive_amp;
                let next_bore =
                    soft_clip(refl * 0.80 + reed_pulse * 0.17 * drive_amp + asym + breath);
                self.bore_delay.write(next_bore);
                next_bore
            }
            WindType::LipReed => {
                let d_bore = (sr / freq - d_lp).max(2.0);
                let bore = self.bore_delay.read_frac(d_bore);
                let refl = self.refl_lp.process_pole(bore, lp_pole);

                // Brass lip buzz: harmonic flare intensifies with velocity & attack
                let lip_buzz = if self.elapsed < 0.035 {
                    (1.0 - self.elapsed / 0.035) * 0.22 * (self.phase * TAU * 2.0).sin()
                } else {
                    0.0
                };
                let flare = 1.0 + velocity * 2.2 * self.brightness;
                let raw_lip = (self.phase * TAU).sin() * 0.65 + (1.0 - 2.0 * self.phase) * 0.35;
                let shaped_lip = (raw_lip * flare).tanh() * 0.24 * drive_amp;
                let breath = breath_filt * self.breath_noise * 0.12 * drive_amp;

                let next_bore = soft_clip(refl * 0.82 + shaped_lip + lip_buzz * drive_amp + breath);
                self.bore_delay.write(next_bore);
                next_bore
            }
        };

        let dc_clean = self.dc_blocker.process(out);
        if let Some(ref mut fmt) = self.formants {
            let f_out = fmt.process(dc_clean);
            dc_clean * (1.0 - self.formant_mix) + f_out * self.formant_mix * 1.4
        } else {
            dc_clean
        }
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
