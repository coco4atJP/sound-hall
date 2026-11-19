use std::f32::consts::PI;

use crate::dsp::envelope::{Adsr, AdsrParams, Lfo};
use crate::dsp::filter::{FormantBank, OnePoleLp, SvfFilter, SvfMode};
use crate::dsp::math::{flush_denormal, midi_to_hz, soft_clip, Rng};
use crate::dsp::modal::ModalBank;
use crate::dsp::oscillator::{PolyBlepOsc, Waveform};
use crate::dsp::waveguide::{BowedStringModel, PluckedStringModel, WindBoreModel};

use super::patch::{
    BowedParams, FmAlgorithm, FmParams, HammeredParams, ModalParams, OrganParams, PluckedParams,
    PresetMetadata, SubParams, SynthEngineKind, WindParams,
};

const DRAWBAR_RATIOS: [f32; 9] = [0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];

enum EngineRuntime {
    Plucked {
        params: PluckedParams,
        primary: PluckedStringModel,
        secondary: Option<PluckedStringModel>,
        cab_lp: SvfFilter,
        env_hold_level: f32,
        released: bool,
        release_timer: f32,
    },
    Hammered {
        params: HammeredParams,
        strings: Vec<PluckedStringModel>,
        thump_rng: Rng,
        thump_lp: OnePoleLp,
        thump_remaining: usize,
        thump_total: usize,
        elapsed: f32,
        released: bool,
        release_timer: f32,
    },
    Bowed {
        params: BowedParams,
        model: BowedStringModel,
        adsr: Adsr,
        vib_lfo: Lfo,
        trem_lfo: PolyBlepOsc,
    },
    Wind {
        params: WindParams,
        model: WindBoreModel,
        drone: Option<WindBoreModel>,
        adsr: Adsr,
        vib_lfo: Lfo,
    },
    Modal {
        params: ModalParams,
        bank: ModalBank,
        adsr: Adsr,
        elapsed: f32,
        re_struck: bool,
    },
    Fm {
        params: FmParams,
        oscs: [PolyBlepOsc; 4],
        op_envs: [f32; 4],
        op3_prev: f32,
        adsr: Adsr,
        vib_lfo: Lfo,
    },
    Organ {
        params: OrganParams,
        oscs: [PolyBlepOsc; 9],
        perc_osc: PolyBlepOsc,
        perc_env: f32,
        click_rng: Rng,
        click_lp: OnePoleLp,
        click_remaining: usize,
        rotary_lfo: PolyBlepOsc,
        adsr: Adsr,
    },
    Sub {
        params: SubParams,
        unison_oscs: Vec<PolyBlepOsc>,
        osc2: PolyBlepOsc,
        sub_osc: PolyBlepOsc,
        noise_rng: Rng,
        filter: SvfFilter,
        formants: Option<FormantBank>,
        lfo: PolyBlepOsc,
        adsr: Adsr,
        filter_env: f32,
        pitch_env: f32,
    },
}

/// Active polyphonic voice rendering one MIDI note.
pub struct Voice {
    pub channel: u8,
    pub note: u8,
    pub velocity: f32,
    pub base_hz: f32,
    pub gain: f32,
    pub reverb_send: f32,
    pub chorus_send: f32,
    pub age: u64,
    pub sustained: bool,
    pub stealing: bool,
    steal_gain: f32,
    finished: bool,
    last_level: f32,
    runtime: EngineRuntime,
}

impl Voice {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        channel: u8,
        note: u8,
        velocity_u8: u8,
        preset: &PresetMetadata,
        sr: f32,
        age: u64,
        reverb_override: f32,
        chorus_override: f32,
    ) -> Self {
        let velocity = (velocity_u8.max(1) as f32 / 127.0).clamp(0.02, 1.0);
        let base_hz = midi_to_hz(note as f32);
        let seed =
            age.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((note as u64) << 8) ^ (channel as u64);

        let runtime =
            match preset.patch {
                SynthEngineKind::PluckedString(p) => {
                    let effective_hz = if p.harmonic_bell {
                        base_hz * 2.0
                    } else {
                        base_hz
                    };
                    let rt60 = if p.palm_mute {
                        p.rt60.min(0.22)
                    } else {
                        p.rt60
                    };
                    let primary = PluckedStringModel::new(
                        effective_hz,
                        sr,
                        velocity,
                        p.pick_pos,
                        p.brightness,
                        rt60,
                        p.release_damping,
                        p.stiffness,
                        p.buzz,
                        p.body_specs,
                        p.body_dry,
                        p.body_wet,
                        seed,
                    );
                    let secondary = if p.octave_double {
                        Some(PluckedStringModel::new(
                            effective_hz * 2.0,
                            sr,
                            velocity * 0.85,
                            p.pick_pos * 1.1,
                            (p.brightness + 0.08).min(0.98),
                            rt60 * 0.85,
                            p.release_damping,
                            p.stiffness,
                            0.0,
                            p.body_specs,
                            p.body_dry,
                            p.body_wet,
                            seed ^ 0x5555_AAAA,
                        ))
                    } else if p.harmonic_bell {
                        // Add gentle fundamental anchor under harmonic bell chime
                        Some(PluckedStringModel::new(
                            base_hz,
                            sr,
                            velocity * 0.55,
                            0.5,
                            p.brightness * 0.6,
                            rt60 * 0.7,
                            p.release_damping,
                            p.stiffness,
                            0.0,
                            p.body_specs,
                            p.body_dry,
                            p.body_wet,
                            seed ^ 0x3333_7777,
                        ))
                    } else {
                        None
                    };
                    EngineRuntime::Plucked {
                        params: p,
                        primary,
                        secondary,
                        cab_lp: SvfFilter::new(),
                        env_hold_level: 1.0,
                        released: false,
                        release_timer: 0.0,
                    }
                }
                SynthEngineKind::HammeredString(p) => {
                    let count = p.unison_count.clamp(1, 3) as usize;
                    let mut strings = Vec::with_capacity(count);
                    let detune_offsets: &[f32] = match count {
                        1 => &[0.0],
                        2 => &[-0.5, 0.5],
                        _ => &[-1.0, 0.0, 1.0],
                    };
                    for (idx, &offset) in detune_offsets.iter().enumerate() {
                        let cents = offset * p.detune_cents;
                        let hz = base_hz * 2.0_f32.powf(cents / 1200.0);
                        // Higher notes naturally have shorter string decay in acoustic pianos
                        let pitch_rt = p.after_decay
                            * (261.63 / base_hz.max(40.0)).powf(0.42).clamp(0.28, 2.1);
                        let bright = (p.brightness * (0.45 + 0.55 * velocity)).clamp(0.12, 0.98);
                        strings.push(PluckedStringModel::new(
                            hz,
                            sr,
                            velocity,
                            0.13 + (idx as f32) * 0.015,
                            bright,
                            pitch_rt,
                            p.release_damping,
                            p.stiffness,
                            0.0,
                            p.body_specs,
                            0.72,
                            0.32,
                            seed ^ ((idx as u64 + 1) * 0x1234_5678),
                        ));
                    }
                    let thump_total = ((0.008 * sr).round() as usize).max(16);
                    EngineRuntime::Hammered {
                        params: p,
                        strings,
                        thump_rng: Rng::new(seed ^ 0xBEEF),
                        thump_lp: OnePoleLp::new(),
                        thump_remaining: thump_total,
                        thump_total,
                        elapsed: 0.0,
                        released: false,
                        release_timer: 0.0,
                    }
                }
                SynthEngineKind::BowedString(p) => {
                    let mut adsr = Adsr::new(p.adsr);
                    adsr.trigger();
                    EngineRuntime::Bowed {
                        params: p,
                        model: BowedStringModel::new(
                            sr,
                            p.bow_pressure,
                            p.brightness,
                            p.rosin_noise,
                            p.ensemble_detune,
                            p.body_specs,
                            p.body_dry,
                            p.body_wet,
                            seed,
                        ),
                        adsr,
                        vib_lfo: Lfo::new(p.vibrato_rate, 0.18, 0.25),
                        trem_lfo: PolyBlepOsc::new(seed ^ 0x9999),
                    }
                }
                SynthEngineKind::WindBore(p) => {
                    let mut adsr = Adsr::new(p.adsr);
                    adsr.trigger();
                    let drone = if p.drone_fifth {
                        Some(WindBoreModel::new(
                            p.wind_type,
                            sr,
                            false,
                            p.breath_noise * 0.3,
                            p.brightness * 0.75,
                            0.0,
                            0.02,
                            p.formant,
                            p.formant_mix,
                            seed ^ 0xD00D,
                        ))
                    } else {
                        None
                    };
                    EngineRuntime::Wind {
                        params: p,
                        model: WindBoreModel::new(
                            p.wind_type,
                            sr,
                            p.cylindrical_odd,
                            p.breath_noise,
                            p.brightness,
                            p.scoop_cents,
                            p.scoop_time,
                            p.formant,
                            p.formant_mix,
                            seed,
                        ),
                        drone,
                        adsr,
                        vib_lfo: Lfo::new(p.vibrato_rate, 0.14, 0.22),
                    }
                }
                SynthEngineKind::ModalPercussion(p) => {
                    let mut adsr =
                        Adsr::new(AdsrParams::new(0.0005, 0.01, 1.0, p.release_time.max(0.04)));
                    adsr.trigger();
                    EngineRuntime::Modal {
                        params: p,
                        bank: ModalBank::new(
                            p.modes,
                            base_hz,
                            sr,
                            velocity,
                            p.mallet_hardness,
                            p.noise_amt,
                            p.tremolo_hz,
                            p.tremolo_depth,
                            seed,
                        ),
                        adsr,
                        elapsed: 0.0,
                        re_struck: false,
                    }
                }
                SynthEngineKind::Fm4Op(p) => {
                    let mut adsr = Adsr::new(p.adsr);
                    adsr.trigger();
                    EngineRuntime::Fm {
                        params: p,
                        oscs: [
                            PolyBlepOsc::new(seed),
                            PolyBlepOsc::new(seed ^ 11),
                            PolyBlepOsc::new(seed ^ 22),
                            PolyBlepOsc::new(seed ^ 33),
                        ],
                        op_envs: [1.0; 4],
                        op3_prev: 0.0,
                        adsr,
                        vib_lfo: Lfo::new(p.vibrato_rate, 0.15, 0.2),
                    }
                }
                SynthEngineKind::AdditiveOrgan(p) => {
                    let mut adsr = Adsr::new(p.adsr);
                    adsr.trigger();
                    let mut rng = Rng::new(seed);
                    let mut oscs = core::array::from_fn(|i| {
                        let phase = rng.next_f32_unipolar() * 0.03; // Slight random phase avoids spike
                        PolyBlepOsc::new(seed ^ (i as u64 + 7)).with_phase(phase)
                    });
                    let _ = &mut oscs;
                    let click_remaining = ((0.003 * sr).round() as usize).max(8);
                    EngineRuntime::Organ {
                        params: p,
                        oscs,
                        perc_osc: PolyBlepOsc::new(seed ^ 0x4444),
                        perc_env: if p.percussion_level > 0.0 { 1.0 } else { 0.0 },
                        click_rng: Rng::new(seed ^ 0x7777),
                        click_lp: OnePoleLp::new(),
                        click_remaining,
                        rotary_lfo: PolyBlepOsc::new(seed ^ 0x8888),
                        adsr,
                    }
                }
                SynthEngineKind::FormantSubtractive(p) => {
                    let mut adsr = Adsr::new(p.adsr);
                    adsr.trigger();
                    let count = p.unison_voices.clamp(1, 5) as usize;
                    let mut rng = Rng::new(seed);
                    let unison_oscs =
                        (0..count)
                            .map(|i| {
                                PolyBlepOsc::new(seed ^ ((i as u64 + 1) * 19))
                                    .with_phase(if i == 0 { 0.0 } else { rng.next_f32_unipolar() })
                            })
                            .collect();
                    let formants = p.formant.map(|fp| FormantBank::new(fp, sr));
                    EngineRuntime::Sub {
                        params: p,
                        unison_oscs,
                        osc2: PolyBlepOsc::new(seed ^ 0xCAFE),
                        sub_osc: PolyBlepOsc::new(seed ^ 0xBABE),
                        noise_rng: Rng::new(seed ^ 0xDEAD),
                        filter: SvfFilter::new(),
                        formants,
                        lfo: PolyBlepOsc::new(seed ^ 0xFACE),
                        adsr,
                        filter_env: 1.0,
                        pitch_env: 1.0,
                    }
                }
            };

        Self {
            channel,
            note,
            velocity,
            base_hz,
            gain: preset.gain,
            reverb_send: (preset.default_reverb * 0.7 + reverb_override * 0.3).clamp(0.0, 1.0),
            chorus_send: (preset.default_chorus.max(chorus_override)).clamp(0.0, 1.0),
            age,
            sustained: false,
            stealing: false,
            steal_gain: 1.0,
            finished: false,
            last_level: 1.0,
            runtime,
        }
    }

    pub fn release(&mut self) {
        self.sustained = false;
        match &mut self.runtime {
            EngineRuntime::Plucked {
                primary,
                secondary,
                released,
                ..
            } => {
                *released = true;
                primary.release();
                if let Some(sec) = secondary {
                    sec.release();
                }
            }
            EngineRuntime::Hammered {
                strings, released, ..
            } => {
                *released = true;
                for s in strings {
                    s.release();
                }
            }
            EngineRuntime::Bowed { adsr, .. }
            | EngineRuntime::Wind { adsr, .. }
            | EngineRuntime::Modal { adsr, .. }
            | EngineRuntime::Fm { adsr, .. }
            | EngineRuntime::Organ { adsr, .. }
            | EngineRuntime::Sub { adsr, .. } => {
                adsr.release();
            }
        }
    }

    pub fn force_steal(&mut self) {
        self.stealing = true;
        self.sustained = false;
        match &mut self.runtime {
            EngineRuntime::Plucked { .. } | EngineRuntime::Hammered { .. } => {}
            EngineRuntime::Bowed { adsr, .. }
            | EngineRuntime::Wind { adsr, .. }
            | EngineRuntime::Modal { adsr, .. }
            | EngineRuntime::Fm { adsr, .. }
            | EngineRuntime::Organ { adsr, .. }
            | EngineRuntime::Sub { adsr, .. } => {
                adsr.force_quick_release();
            }
        }
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    #[inline]
    pub fn is_releasing(&self) -> bool {
        if self.stealing {
            return true;
        }
        match &self.runtime {
            EngineRuntime::Plucked { released, .. } | EngineRuntime::Hammered { released, .. } => {
                *released
            }
            EngineRuntime::Bowed { adsr, .. }
            | EngineRuntime::Wind { adsr, .. }
            | EngineRuntime::Modal { adsr, .. }
            | EngineRuntime::Fm { adsr, .. }
            | EngineRuntime::Organ { adsr, .. }
            | EngineRuntime::Sub { adsr, .. } => adsr.is_releasing(),
        }
    }

    #[inline]
    pub fn estimated_level(&self) -> f32 {
        self.last_level
    }

    /// Renders one stereo sample `(left, right)` for this voice.
    #[inline]
    pub fn render_sample(
        &mut self,
        sr: f32,
        pitch_bend_ratio: f32,
        mod_wheel: f32,
        expression: f32,
    ) -> (f32, f32) {
        if self.finished {
            return (0.0, 0.0);
        }

        if self.stealing {
            self.steal_gain -= 1.0 / (0.005 * sr.max(8000.0));
            if self.steal_gain <= 0.0 {
                self.finished = true;
                return (0.0, 0.0);
            }
        }

        let freq = (self.base_hz * pitch_bend_ratio).clamp(16.0, 0.44 * sr);
        let vel = self.velocity;

        let (mut out_l, mut out_r) = match &mut self.runtime {
            EngineRuntime::Plucked {
                params,
                primary,
                secondary,
                cab_lp,
                env_hold_level,
                released,
                release_timer,
            } => {
                let effective_hz = if params.harmonic_bell {
                    freq * 2.0
                } else {
                    freq
                };
                let mut s = primary.next_sample(effective_hz, sr);
                if let Some(sec) = secondary {
                    let sec_hz = if params.harmonic_bell {
                        freq
                    } else {
                        effective_hz * 2.0
                    };
                    s = s * 0.72 + sec.next_sample(sec_hz, sr) * 0.36;
                }
                if params.distortion > 0.01 {
                    let drive = 1.0 + params.distortion * 11.0;
                    s = (s * drive).tanh() * 0.65;
                    s = cab_lp.process(s, 3600.0, 0.75, sr, SvfMode::LowPass);
                }
                if *released {
                    *release_timer += 1.0 / sr;
                    let damp_sec = (params.release_damping * 4.5).max(0.06);
                    let fade = (1.0 - *release_timer / damp_sec).clamp(0.0, 1.0);
                    *env_hold_level = fade * fade;
                    if fade <= 0.0 || (*release_timer > 0.04 && s.abs() < 1e-4) {
                        self.finished = true;
                    }
                    s *= *env_hold_level;
                } else {
                    *release_timer += 1.0 / sr;
                    if *release_timer > (params.rt60 * 1.4).max(1.2) && s.abs() < 2e-4 {
                        self.finished = true;
                    }
                }
                (s, s)
            }
            EngineRuntime::Hammered {
                params,
                strings,
                thump_rng,
                thump_lp,
                thump_remaining,
                thump_total,
                elapsed,
                released,
                release_timer,
            } => {
                let dt = 1.0 / sr;
                *elapsed += dt;
                // Prompt + aftersound two-stage piano envelope
                let prompt = (-*elapsed / params.prompt_decay.max(0.05)).exp();
                let after = (-*elapsed / params.after_decay.max(0.4)).exp();
                let two_stage = 0.58 * prompt + 0.42 * after;

                let mut sum_l = 0.0_f32;
                let mut sum_r = 0.0_f32;
                let count = strings.len();
                let detune_offsets: &[f32] = match count {
                    1 => &[0.0],
                    2 => &[-0.5, 0.5],
                    _ => &[-1.0, 0.0, 1.0],
                };
                for (idx, s_model) in strings.iter_mut().enumerate() {
                    let cents = detune_offsets[idx] * params.detune_cents;
                    let f = freq * 2.0_f32.powf(cents / 1200.0);
                    let y = s_model.next_sample(f, sr);
                    let pan_offset = if count > 1 {
                        (idx as f32 / (count - 1) as f32 - 0.5) * params.stereo_width
                    } else {
                        0.0
                    };
                    sum_l += y * (1.0 - pan_offset * 0.5);
                    sum_r += y * (1.0 + pan_offset * 0.5);
                }
                let norm = 1.0 / (count as f32).sqrt();
                let mut l = sum_l * norm * (0.45 + 0.55 * two_stage);
                let mut r = sum_r * norm * (0.45 + 0.55 * two_stage);

                // Mechanical hammer-felt thump transient
                if *thump_remaining > 0 {
                    let p = 1.0 - (*thump_remaining as f32 / *thump_total as f32);
                    let win = (PI * p).sin();
                    let raw_thump = thump_rng.next_f32_bipolar() * win;
                    let thump = thump_lp.process_cutoff(raw_thump, 280.0 + vel * 600.0, sr)
                        * params.hammer_thump
                        * vel
                        * 0.55;
                    l += thump;
                    r += thump;
                    *thump_remaining -= 1;
                }

                // Subtle stereo piano soundboard key-position spread (bass left, treble right)
                let kb_pan = ((self.note as f32 - 60.0) / 48.0).clamp(-0.6, 0.6)
                    * params.stereo_width
                    * 0.45;
                let out_l = l * (1.0 - kb_pan * 0.5);
                let out_r = r * (1.0 + kb_pan * 0.5);

                if *released {
                    *release_timer += dt;
                    let damp_sec = (params.release_damping * 3.8).max(0.08);
                    let fade = (1.0 - *release_timer / damp_sec).clamp(0.0, 1.0);
                    let env = fade * fade;
                    if fade <= 0.0 || (*release_timer > 0.05 && out_l.abs() + out_r.abs() < 1e-4) {
                        self.finished = true;
                    }
                    (out_l * env, out_r * env)
                } else {
                    if *elapsed > params.after_decay * 2.5 && out_l.abs() + out_r.abs() < 1e-4 {
                        self.finished = true;
                    }
                    (out_l, out_r)
                }
            }
            EngineRuntime::Bowed {
                params,
                model,
                adsr,
                vib_lfo,
                trem_lfo,
            } => {
                let env = adsr.next(sr);
                if adsr.is_finished() {
                    self.finished = true;
                }
                let vib_cents =
                    vib_lfo.next_sine(sr) * (params.vibrato_depth_cents + mod_wheel * 28.0);
                let f = freq * 2.0_f32.powf(vib_cents / 1200.0);
                let mut s = model.next_sample(f, sr, env, vel) * env;
                if params.tremolo_hz > 0.5 {
                    let dt_t = PolyBlepOsc::dt(params.tremolo_hz, sr);
                    let trem = 0.68 + 0.32 * trem_lfo.next_sample(Waveform::Sine, dt_t);
                    s *= trem;
                }
                (s, s)
            }
            EngineRuntime::Wind {
                params,
                model,
                drone,
                adsr,
                vib_lfo,
            } => {
                let env = adsr.next(sr);
                if adsr.is_finished() {
                    self.finished = true;
                }
                let vib_cents =
                    vib_lfo.next_sine(sr) * (params.vibrato_depth_cents + mod_wheel * 24.0);
                let f = freq * 2.0_f32.powf(vib_cents / 1200.0);
                let mut s = model.next_sample(f, sr, env, vel) * env;
                if let Some(d) = drone {
                    // Bagpipe octave-below / fifth drone
                    let drone_hz = (midi_to_hz(48.0)).min(f * 0.5).max(55.0);
                    let d_out = d.next_sample(drone_hz, sr, env * 0.65, 0.7);
                    s = s * 0.78 + d_out * 0.35 * env;
                }
                (s, s)
            }
            EngineRuntime::Modal {
                params,
                bank,
                adsr,
                elapsed,
                re_struck,
            } => {
                let dt = 1.0 / sr;
                *elapsed += dt;
                if params.re_strike_delay > 0.005
                    && !*re_struck
                    && *elapsed >= params.re_strike_delay
                {
                    *re_struck = true;
                    bank.re_excite(vel * 0.72);
                }
                let env = adsr.next(sr);
                let s = bank.next_sample(freq, sr) * env;
                if (adsr.is_finished() || (*elapsed > 0.25 && s.abs() < 6e-5))
                    && (adsr.is_releasing() || *elapsed > 2.5)
                {
                    self.finished = true;
                }
                (s, s)
            }
            EngineRuntime::Fm {
                params,
                oscs,
                op_envs,
                op3_prev,
                adsr,
                vib_lfo,
            } => {
                let master_env = adsr.next(sr);
                if adsr.is_finished() {
                    self.finished = true;
                }
                let vib_cents =
                    vib_lfo.next_sine(sr) * (params.vibrato_depth_cents + mod_wheel * 25.0);
                let f_note = freq * 2.0_f32.powf(vib_cents / 1200.0);

                let dt_sec = 1.0 / sr;
                let mut op_vals = [0.0_f32; 4];
                for i in 0..4 {
                    let op = &params.ops[i];
                    let coeff = (-4.5 * dt_sec / op.decay.max(0.01)).exp();
                    op_envs[i] = op.sustain + (op_envs[i] - op.sustain) * coeff;
                    let vel_scale = 1.0 - op.vel_sens + op.vel_sens * vel;
                    op_vals[i] = op.level * op_envs[i] * vel_scale;
                }

                let op_dt = |i: usize| -> f32 {
                    let op = &params.ops[i];
                    let hz = if op.fixed_hz > 0.0 {
                        op.fixed_hz
                    } else {
                        f_note * op.ratio * 2.0_f32.powf(op.detune_cents / 1200.0)
                    };
                    PolyBlepOsc::dt(hz, sr)
                };

                let y3 = oscs[3].next_sample_pm(
                    Waveform::Sine,
                    op_dt(3),
                    *op3_prev * params.feedback * PI,
                ) * op_vals[3];
                *op3_prev = flush_denormal(y3);

                let s = match params.algorithm {
                    FmAlgorithm::Stack4 => {
                        let y2 =
                            oscs[2].next_sample_pm(Waveform::Sine, op_dt(2), y3 * PI) * op_vals[2];
                        let y1 =
                            oscs[1].next_sample_pm(Waveform::Sine, op_dt(1), y2 * PI) * op_vals[1];
                        oscs[0].next_sample_pm(Waveform::Sine, op_dt(0), y1 * PI) * op_vals[0]
                    }
                    FmAlgorithm::Dual2x2 => {
                        let y2 =
                            oscs[2].next_sample_pm(Waveform::Sine, op_dt(2), y3 * PI) * op_vals[2];
                        let y1 = oscs[1].next_sample(Waveform::Sine, op_dt(1)) * op_vals[1];
                        let y0 =
                            oscs[0].next_sample_pm(Waveform::Sine, op_dt(0), y1 * PI) * op_vals[0];
                        (y2 + y0) * 0.6
                    }
                    FmAlgorithm::ThreeToOne => {
                        let y2 = oscs[2].next_sample(Waveform::Sine, op_dt(2)) * op_vals[2];
                        let y1 = oscs[1].next_sample(Waveform::Sine, op_dt(1)) * op_vals[1];
                        oscs[0].next_sample_pm(Waveform::Sine, op_dt(0), (y3 + y2 + y1) * PI)
                            * op_vals[0]
                    }
                    FmAlgorithm::Parallel => {
                        let y2 = oscs[2].next_sample(Waveform::Sine, op_dt(2)) * op_vals[2];
                        let y1 = oscs[1].next_sample(Waveform::Sine, op_dt(1)) * op_vals[1];
                        let y0 = oscs[0].next_sample(Waveform::Sine, op_dt(0)) * op_vals[0];
                        (y3 + y2 + y1 + y0) * 0.42
                    }
                };

                let out = s * master_env * vel;
                (out, out)
            }
            EngineRuntime::Organ {
                params,
                oscs,
                perc_osc,
                perc_env,
                click_rng,
                click_lp,
                click_remaining,
                rotary_lfo,
                adsr,
            } => {
                let env = adsr.next(sr);
                if adsr.is_finished() {
                    self.finished = true;
                }
                let mut sum = 0.0_f32;
                let mut weight_sum = 0.0_f32;
                for i in 0..9 {
                    let level = params.drawbars[i];
                    if level > 0.001 {
                        let h_hz = freq * DRAWBAR_RATIOS[i];
                        if h_hz < 0.45 * sr {
                            let dt = PolyBlepOsc::dt(h_hz, sr);
                            sum += oscs[i].next_sample(Waveform::Sine, dt) * level;
                            weight_sum += level;
                        }
                    }
                }
                if weight_sum > 1e-4 {
                    sum /= weight_sum.sqrt().max(1.0);
                }

                // 2nd or 3rd harmonic percussion transient
                if *perc_env > 0.001 && params.percussion_level > 0.001 {
                    let dt_p = PolyBlepOsc::dt(freq * params.percussion_harmonic, sr);
                    sum += perc_osc.next_sample(Waveform::Sine, dt_p)
                        * (*perc_env)
                        * params.percussion_level;
                    *perc_env *= (-1.0 / (0.095 * sr)).exp();
                }

                // Key click transient
                if *click_remaining > 0 && params.key_click > 0.001 {
                    let n = click_rng.next_f32_bipolar();
                    let c = click_lp.process_cutoff(n, 3200.0, sr);
                    sum += c * params.key_click * 0.45;
                    *click_remaining -= 1;
                }

                // Accordion / Harmonica / Pipe breathiness
                if params.breathiness > 0.001 {
                    let n = click_rng.next_pink();
                    sum += n * params.breathiness * 0.14;
                }

                if params.overdrive > 0.01 {
                    let drv = 1.0 + params.overdrive * 3.5;
                    sum = (sum * drv).tanh() * 0.85;
                }

                let base = sum * env * (0.4 + 0.6 * vel);
                // Rotary Leslie stereo effect
                if params.rotary_speed > 0.05 && params.rotary_depth > 0.005 {
                    let speed = params.rotary_speed + mod_wheel * 4.5;
                    let dt_r = PolyBlepOsc::dt(speed, sr);
                    let r_sin = rotary_lfo.next_sample(Waveform::Sine, dt_r);
                    let mod_l = 1.0 - params.rotary_depth * 0.35 * (0.5 + 0.5 * r_sin);
                    let mod_r = 1.0 - params.rotary_depth * 0.35 * (0.5 - 0.5 * r_sin);
                    (base * mod_l, base * mod_r)
                } else {
                    (base, base)
                }
            }
            EngineRuntime::Sub {
                params,
                unison_oscs,
                osc2,
                sub_osc,
                noise_rng,
                filter,
                formants,
                lfo,
                adsr,
                filter_env,
                pitch_env,
            } => {
                let env = adsr.next(sr);
                if adsr.is_finished() {
                    self.finished = true;
                }
                let dt_sec = 1.0 / sr;
                *filter_env *= (-4.5 * dt_sec / params.filter_env_decay.max(0.015)).exp();
                *pitch_env *= (-4.5 * dt_sec / params.pitch_env_decay.max(0.01)).exp();

                let dt_lfo = PolyBlepOsc::dt(params.lfo_rate, sr);
                let lfo_val = lfo.next_sample(Waveform::Sine, dt_lfo);
                let pitch_mod_semi = params.pitch_env_semitones * (*pitch_env)
                    + (lfo_val * (params.lfo_depth_cents + mod_wheel * 28.0)) / 100.0;
                let note_hz = (freq * 2.0_f32.powf(pitch_mod_semi / 12.0)).clamp(16.0, 0.45 * sr);

                let count = unison_oscs.len();
                let mut sum_l = 0.0_f32;
                let mut sum_r = 0.0_f32;
                for (i, uosc) in unison_oscs.iter_mut().enumerate() {
                    let norm_idx = if count > 1 {
                        (i as f32 / (count - 1) as f32) * 2.0 - 1.0
                    } else {
                        0.0
                    };
                    let u_hz =
                        note_hz * 2.0_f32.powf((norm_idx * params.unison_detune_cents) / 1200.0);
                    let u_dt = PolyBlepOsc::dt(u_hz, sr);
                    let s = uosc.next_sample(params.osc1_wave, u_dt);
                    let spread = norm_idx * params.stereo_spread;
                    sum_l += s * (1.0 - spread * 0.45);
                    sum_r += s * (1.0 + spread * 0.45);
                }
                let u_norm = 1.0 / (count as f32).sqrt();
                let mut mono_l = sum_l * u_norm;
                let mut mono_r = sum_r * u_norm;

                if params.osc2_level > 0.001 {
                    let osc2_semi = params.osc2_semitones + params.osc2_detune_cents / 100.0;
                    let hz2 = note_hz * 2.0_f32.powf(osc2_semi / 12.0);
                    let s2 = osc2.next_sample(params.osc2_wave, PolyBlepOsc::dt(hz2, sr))
                        * params.osc2_level;
                    mono_l += s2;
                    mono_r += s2;
                }
                if params.sub_level > 0.001 {
                    let s_sub = sub_osc
                        .next_sample(Waveform::Square, PolyBlepOsc::dt(note_hz * 0.5, sr))
                        * params.sub_level;
                    mono_l += s_sub;
                    mono_r += s_sub;
                }
                if params.noise_level > 0.001 {
                    let n = noise_rng.next_pink() * params.noise_level;
                    mono_l += n;
                    mono_r += n;
                }

                // Filter cutoff with velocity scaling, keytracking, envelope, and LFO
                let key_factor = (note_hz / 261.63).powf(params.keytrack.clamp(0.0, 1.0));
                let cutoff = (params.cutoff_hz * key_factor * (0.55 + 0.45 * vel)
                    + params.filter_env_amt * (*filter_env) * (0.5 + 0.5 * vel)
                    + params.lfo_filter_amt * lfo_val)
                    .clamp(35.0, 0.45 * sr);

                let mono_mix = 0.5 * (mono_l + mono_r);
                let side = 0.5 * (mono_l - mono_r);
                let mut filt_mono =
                    filter.process(mono_mix, cutoff, params.resonance_q, sr, params.filter_mode);

                if let Some(fmt) = formants {
                    let f_sig = fmt.process(filt_mono) * 1.35;
                    filt_mono = filt_mono * (1.0 - params.formant_mix) + f_sig * params.formant_mix;
                }

                let amp = env * (0.35 + 0.65 * vel);
                (
                    (filt_mono + side * 0.65) * amp,
                    (filt_mono - side * 0.65) * amp,
                )
            }
        };

        out_l = soft_clip(out_l) * self.gain * expression * self.steal_gain;
        out_r = soft_clip(out_r) * self.gain * expression * self.steal_gain;
        self.last_level = 0.5 * (out_l.abs() + out_r.abs());
        (out_l, out_r)
    }
}
