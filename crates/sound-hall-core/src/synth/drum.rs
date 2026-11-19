use std::f32::consts::TAU;

use crate::dsp::filter::{SvfFilter, SvfMode};
use crate::dsp::math::{soft_clip, Rng};
use crate::dsp::modal::{ModalBank, ModeSpec};
use crate::dsp::oscillator::{PolyBlepOsc, Waveform};

/// Physical percussion synthesis model kind for GM drums.
#[derive(Clone, Copy, Debug)]
pub enum DrumModelKind {
    Kick {
        f_start: f32,
        f_end: f32,
        pitch_decay: f32,
        body_decay: f32,
        click_amt: f32,
    },
    Snare {
        body_hz: f32,
        body_decay: f32,
        wire_hz: f32,
        wire_q: f32,
        wire_decay: f32,
        snap_amt: f32,
    },
    Clap {
        filter_hz: f32,
        decay: f32,
        spread_sec: f32,
    },
    Tom {
        f_start: f32,
        f_end: f32,
        pitch_decay: f32,
        decay: f32,
        overtone_amp: f32,
    },
    MetallicCymbal {
        base_hz: f32,
        hp_hz: f32,
        bp_hz: f32,
        decay: f32,
        noise_mix: f32,
        bell_mix: f32,
    },
    ModalDrum {
        base_hz: f32,
        modes: &'static [ModeSpec],
        hardness: f32,
        noise_amt: f32,
        gain: f32,
    },
    ShakerScrape {
        hp_hz: f32,
        bp_hz: f32,
        q: f32,
        duration: f32,
        ratchet_hz: f32,
        attack_ramp: f32,
    },
    Vibraslap {
        body_hz: f32,
        rattle_hz: f32,
        decay: f32,
    },
    WhistleCuica {
        f_start: f32,
        f_end: f32,
        duration: f32,
        vibrato_hz: f32,
        breath: f32,
    },
}

/// General MIDI Standard Drum Kit instrument specification (notes 35..=81).
#[derive(Clone, Copy, Debug)]
pub struct DrumPatch {
    pub note: u8,
    pub name_en: &'static str,
    pub name_ja: &'static str,
    /// Choke group ID (0 = none; 1 = hi-hat, 2 = whistle, 3 = guiro, 4 = triangle).
    pub choke_group: u8,
    /// Default stereo pan [0.0..1.0].
    pub default_pan: f32,
    pub gain: f32,
    pub model: DrumModelKind,
}

const CYMBAL_RATIOS: [f32; 6] = [1.0, 1.483, 1.932, 2.546, 2.631, 3.897];

/// Active drum voice instance.
pub struct DrumVoice {
    pub note: u8,
    pub choke_group: u8,
    pub pan: f32,
    pub gain: f32,
    pub age: u64,
    velocity: f32,
    elapsed: f32,
    choking: bool,
    choke_gain: f32,
    finished: bool,
    model: DrumModelKind,
    oscs: [PolyBlepOsc; 6],
    rng: Rng,
    filter1: SvfFilter,
    filter2: SvfFilter,
    modal: Option<ModalBank>,
}

impl DrumVoice {
    pub fn new(patch: &DrumPatch, velocity_u8: u8, sr: f32, age: u64) -> Self {
        let velocity = (velocity_u8.max(1) as f32 / 127.0).clamp(0.05, 1.0);
        let seed = age.wrapping_mul(0x517C_C1B7_2722_0A95) ^ (patch.note as u64);
        let modal = match patch.model {
            DrumModelKind::ModalDrum {
                base_hz,
                modes,
                hardness,
                noise_amt,
                ..
            } => Some(ModalBank::new(
                modes, base_hz, sr, velocity, hardness, noise_amt, 0.0, 0.0, seed,
            )),
            _ => None,
        };

        Self {
            note: patch.note,
            choke_group: patch.choke_group,
            pan: patch.default_pan,
            gain: patch.gain,
            age,
            velocity,
            elapsed: 0.0,
            choking: false,
            choke_gain: 1.0,
            finished: false,
            model: patch.model,
            oscs: core::array::from_fn(|i| PolyBlepOsc::new(seed ^ ((i as u64 + 1) * 31))),
            rng: Rng::new(seed ^ 0xF00D),
            filter1: SvfFilter::new(),
            filter2: SvfFilter::new(),
            modal,
        }
    }

    pub fn choke(&mut self) {
        self.choking = true;
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    #[inline]
    pub fn render_sample(&mut self, sr: f32) -> f32 {
        if self.finished {
            return 0.0;
        }
        let sr = sr.max(8000.0);
        let dt = 1.0 / sr;
        self.elapsed += dt;
        let t = self.elapsed;

        if self.choking {
            self.choke_gain -= dt / 0.008;
            if self.choke_gain <= 0.0 {
                self.finished = true;
                return 0.0;
            }
        }

        let s = match self.model {
            DrumModelKind::Kick {
                f_start,
                f_end,
                pitch_decay,
                body_decay,
                click_amt,
            } => {
                let f = f_end + (f_start - f_end) * (-t / pitch_decay.max(0.002)).exp();
                let osc = self.oscs[0].next_sample(Waveform::Sine, PolyBlepOsc::dt(f, sr));
                let body_env = (-t / body_decay.max(0.03)).exp();
                let click = if t < 0.004 {
                    let c = self.rng.next_f32_bipolar() * (1.0 - t / 0.004);
                    self.filter1.process(c, 2200.0, 0.9, sr, SvfMode::BandPass) * click_amt
                } else {
                    0.0
                };
                if t > body_decay * 6.5 {
                    self.finished = true;
                }
                soft_clip((osc * body_env * 1.25 + click) * 1.2)
            }
            DrumModelKind::Snare {
                body_hz,
                body_decay,
                wire_hz,
                wire_q,
                wire_decay,
                snap_amt,
            } => {
                let f = body_hz * (1.0 + 0.85 * (-t / 0.014).exp());
                let body = self.oscs[0].next_sample(Waveform::Sine, PolyBlepOsc::dt(f, sr))
                    * (-t / body_decay.max(0.02)).exp();
                let noise = self.rng.next_f32_bipolar();
                let hp = self
                    .filter1
                    .process(noise, 1400.0, 0.7, sr, SvfMode::HighPass);
                let wires = self
                    .filter2
                    .process(hp, wire_hz, wire_q, sr, SvfMode::BandPass)
                    * (-t / wire_decay.max(0.03)).exp();
                let snap = if t < 0.006 {
                    (1.0 - t / 0.006) * snap_amt * self.rng.next_f32_bipolar()
                } else {
                    0.0
                };
                let max_d = body_decay.max(wire_decay);
                if t > max_d * 6.5 {
                    self.finished = true;
                }
                soft_clip(body * 0.65 + wires * 1.15 + snap)
            }
            DrumModelKind::Clap {
                filter_hz,
                decay,
                spread_sec,
            } => {
                let n = self.rng.next_f32_bipolar();
                let bp = self
                    .filter1
                    .process(n, filter_hz, 1.4, sr, SvfMode::BandPass);
                // 4 staggered hand taps + reverberant tail
                let mut env = 0.0_f32;
                for i in 0..4 {
                    let onset = i as f32 * spread_sec;
                    if t >= onset {
                        let dt_tap = t - onset;
                        let tap_d = if i == 3 { decay } else { 0.011 };
                        let w = if i == 3 { 0.85 } else { 0.75 };
                        env += w * (-dt_tap / tap_d).exp();
                    }
                }
                if t > spread_sec * 3.0 + decay * 6.5 {
                    self.finished = true;
                }
                soft_clip(bp * env * 1.4)
            }
            DrumModelKind::Tom {
                f_start,
                f_end,
                pitch_decay,
                decay,
                overtone_amp,
            } => {
                let f = f_end + (f_start - f_end) * (-t / pitch_decay.max(0.005)).exp();
                let fund = self.oscs[0].next_sample(Waveform::Sine, PolyBlepOsc::dt(f, sr));
                let over = self.oscs[1].next_sample(Waveform::Sine, PolyBlepOsc::dt(f * 1.59, sr))
                    * overtone_amp
                    * (-t / (decay * 0.45)).exp();
                let stick = if t < 0.004 {
                    self.rng.next_f32_bipolar() * (1.0 - t / 0.004) * 0.35
                } else {
                    0.0
                };
                let env = (-t / decay.max(0.04)).exp();
                if t > decay * 6.5 {
                    self.finished = true;
                }
                soft_clip((fund + over) * env + stick)
            }
            DrumModelKind::MetallicCymbal {
                base_hz,
                hp_hz,
                bp_hz,
                decay,
                noise_mix,
                bell_mix,
            } => {
                let mut metal = 0.0_f32;
                for (i, &ratio) in CYMBAL_RATIOS.iter().enumerate() {
                    let hz = base_hz * ratio;
                    if hz < 0.46 * sr {
                        metal +=
                            self.oscs[i].next_sample(Waveform::Square, PolyBlepOsc::dt(hz, sr));
                    }
                }
                metal *= 1.0 / 6.0;
                let noise = self.rng.next_f32_bipolar();
                let mixed = metal * (1.0 - noise_mix) + noise * noise_mix;
                let hp = self
                    .filter1
                    .process(mixed, hp_hz, 0.85, sr, SvfMode::HighPass);
                let bp = self.filter2.process(hp, bp_hz, 1.1, sr, SvfMode::BandPass);
                let env = (-t / decay.max(0.025)).exp();
                let bell = if bell_mix > 0.01 {
                    let b1 = self.oscs[0]
                        .next_sample(Waveform::Sine, PolyBlepOsc::dt(base_hz * 1.0, sr));
                    b1 * bell_mix * (-t / (decay * 0.9)).exp()
                } else {
                    0.0
                };
                if t > decay * 6.5 {
                    self.finished = true;
                }
                (hp * 0.45 + bp * 0.85) * env + bell
            }
            DrumModelKind::ModalDrum { gain, .. } => {
                let out = if let Some(ref mut m) = self.modal {
                    m.next_sample(
                        match self.model {
                            DrumModelKind::ModalDrum { base_hz, .. } => base_hz,
                            _ => 440.0,
                        },
                        sr,
                    ) * gain
                } else {
                    0.0
                };
                if (t > 0.15 && out.abs() < 4e-5) || t > 4.0 {
                    self.finished = true;
                }
                out
            }
            DrumModelKind::ShakerScrape {
                hp_hz,
                bp_hz,
                q,
                duration,
                ratchet_hz,
                attack_ramp,
            } => {
                let raw = self.rng.next_f32_bipolar();
                let hp = self
                    .filter1
                    .process(raw, hp_hz, 0.75, sr, SvfMode::HighPass);
                let bp = self.filter2.process(hp, bp_hz, q, sr, SvfMode::BandPass);
                let atk = (t / attack_ramp.max(0.002)).clamp(0.0, 1.0);
                let dec = (-t / duration.max(0.02)).exp();
                let mod_ratchet = if ratchet_hz > 1.0 {
                    let pulse = 0.5 + 0.5 * (t * TAU * ratchet_hz).sin();
                    pulse * pulse
                } else {
                    1.0
                };
                if t > duration * 6.0 {
                    self.finished = true;
                }
                bp * atk * dec * mod_ratchet * 1.35
            }
            DrumModelKind::Vibraslap {
                body_hz,
                rattle_hz,
                decay,
            } => {
                let rattle = 0.35 + 0.65 * (0.5 + 0.5 * (t * TAU * rattle_hz).sin());
                let wood_tone =
                    self.oscs[0].next_sample(Waveform::Triangle, PolyBlepOsc::dt(body_hz, sr));
                let noise = self.filter1.process(
                    self.rng.next_f32_bipolar(),
                    1650.0,
                    2.6,
                    sr,
                    SvfMode::BandPass,
                );
                let env = (-t / decay.max(0.08)).exp();
                if t > decay * 6.0 {
                    self.finished = true;
                }
                (wood_tone * 0.45 + noise * 0.95) * rattle * env
            }
            DrumModelKind::WhistleCuica {
                f_start,
                f_end,
                duration,
                vibrato_hz,
                breath,
            } => {
                let progress = (t / duration.max(0.05)).clamp(0.0, 1.0);
                let vib = (t * TAU * vibrato_hz).sin() * 0.015;
                let f = (f_start + (f_end - f_start) * progress) * (1.0 + vib);
                let tone = self.oscs[0].next_sample(Waveform::Sine, PolyBlepOsc::dt(f, sr));
                let air = self.filter1.process(
                    self.rng.next_f32_bipolar(),
                    f.clamp(400.0, 6000.0),
                    1.8,
                    sr,
                    SvfMode::BandPass,
                ) * breath;
                let env = if t < 0.012 {
                    t / 0.012
                } else if t < duration {
                    1.0 - 0.15 * progress
                } else {
                    (-((t - duration) / 0.045)).exp()
                };
                if t > duration + 0.35 {
                    self.finished = true;
                }
                (tone + air) * env * 0.7
            }
        };

        s * self.velocity * self.gain * self.choke_gain
    }
}
