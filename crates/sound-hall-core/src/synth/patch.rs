use crate::dsp::envelope::AdsrParams;
use crate::dsp::filter::{FormantPreset, SvfMode};
use crate::dsp::modal::ModeSpec;
use crate::dsp::oscillator::Waveform;
use crate::dsp::waveguide::{WindType, WindVoicing};

/// General MIDI instrument family (16 standard groups of 8 instruments each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GmFamily {
    Piano,
    ChromaticPercussion,
    Organ,
    Guitar,
    Bass,
    Strings,
    Ensemble,
    Brass,
    Reed,
    Pipe,
    SynthLead,
    SynthPad,
    SynthEffects,
    Ethnic,
    Percussive,
    SoundEffects,
}

impl GmFamily {
    pub const ALL: [GmFamily; 16] = [
        GmFamily::Piano,
        GmFamily::ChromaticPercussion,
        GmFamily::Organ,
        GmFamily::Guitar,
        GmFamily::Bass,
        GmFamily::Strings,
        GmFamily::Ensemble,
        GmFamily::Brass,
        GmFamily::Reed,
        GmFamily::Pipe,
        GmFamily::SynthLead,
        GmFamily::SynthPad,
        GmFamily::SynthEffects,
        GmFamily::Ethnic,
        GmFamily::Percussive,
        GmFamily::SoundEffects,
    ];

    pub const fn name_en(self) -> &'static str {
        match self {
            GmFamily::Piano => "Piano",
            GmFamily::ChromaticPercussion => "Chromatic Percussion",
            GmFamily::Organ => "Organ",
            GmFamily::Guitar => "Guitar",
            GmFamily::Bass => "Bass",
            GmFamily::Strings => "Strings",
            GmFamily::Ensemble => "Ensemble",
            GmFamily::Brass => "Brass",
            GmFamily::Reed => "Reed",
            GmFamily::Pipe => "Pipe",
            GmFamily::SynthLead => "Synth Lead",
            GmFamily::SynthPad => "Synth Pad",
            GmFamily::SynthEffects => "Synth Effects",
            GmFamily::Ethnic => "Ethnic",
            GmFamily::Percussive => "Percussive",
            GmFamily::SoundEffects => "Sound Effects",
        }
    }

    pub const fn name_ja(self) -> &'static str {
        match self {
            GmFamily::Piano => "ピアノ",
            GmFamily::ChromaticPercussion => "体鳴・鍵盤打楽器",
            GmFamily::Organ => "オルガン・リード鍵盤",
            GmFamily::Guitar => "ギター",
            GmFamily::Bass => "ベース",
            GmFamily::Strings => "ストリングス",
            GmFamily::Ensemble => "アンサンブル・合唱",
            GmFamily::Brass => "ブラス",
            GmFamily::Reed => "リード木管",
            GmFamily::Pipe => "エアリード・笛",
            GmFamily::SynthLead => "シンセ・リード",
            GmFamily::SynthPad => "シンセ・パッド",
            GmFamily::SynthEffects => "シンセ・FX",
            GmFamily::Ethnic => "民族楽器",
            GmFamily::Percussive => "パーカッシブ",
            GmFamily::SoundEffects => "効果音",
        }
    }
}

/// Parameters for Extended Karplus-Strong plucked string synthesis.
#[derive(Clone, Copy, Debug)]
pub struct PluckedParams {
    pub pick_pos: f32,
    pub brightness: f32,
    pub rt60: f32,
    pub release_damping: f32,
    pub stiffness: f32,
    pub buzz: f32,
    pub body_specs: [(f32, f32, f32); 3],
    pub body_dry: f32,
    pub body_wet: f32,
    pub distortion: f32,
    pub palm_mute: bool,
    pub octave_double: bool,
    pub harmonic_bell: bool,
}

/// Parameters for hammered acoustic/electric grand piano strings.
#[derive(Clone, Copy, Debug)]
pub struct HammeredParams {
    pub unison_count: u8,
    pub detune_cents: f32,
    pub brightness: f32,
    pub stiffness: f32,
    pub prompt_decay: f32,
    pub after_decay: f32,
    pub release_damping: f32,
    pub hammer_thump: f32,
    pub body_specs: [(f32, f32, f32); 3],
    pub stereo_width: f32,
}

/// Parameters for slip-stick bowed strings.
#[derive(Clone, Copy, Debug)]
pub struct BowedParams {
    pub bow_pressure: f32,
    pub brightness: f32,
    pub rosin_noise: f32,
    /// Section pitch spread (frequency ratio). Non-zero turns the voice into a 4-player
    /// section with individual tuning, vibrato and bow timing.
    pub ensemble_detune: f32,
    /// Violin-family body size relative to a violin (violin 1.0, viola ~0.82, cello ~0.37,
    /// contrabass ~0.22): scales the measured body-mode template.
    pub body_scale: f32,
    pub adsr: AdsrParams,
    pub vibrato_rate: f32,
    pub vibrato_depth_cents: f32,
    pub tremolo_hz: f32,
}

/// Parameters for physical wind bore models (Flutes, Reeds, Brass).
#[derive(Clone, Copy, Debug)]
pub struct WindParams {
    pub wind_type: WindType,
    pub cylindrical_odd: bool,
    pub breath_noise: f32,
    pub brightness: f32,
    pub scoop_cents: f32,
    pub scoop_time: f32,
    pub formant: Option<FormantPreset>,
    pub formant_mix: f32,
    pub adsr: AdsrParams,
    pub vibrato_rate: f32,
    pub vibrato_depth_cents: f32,
    pub drone_fifth: bool,
    /// Instrument voicing: spectral roll-off, dynamic brightness, radiation and formants.
    pub voicing: WindVoicing,
}

/// Parameters for damped modal percussion resonators.
#[derive(Clone, Copy, Debug)]
pub struct ModalParams {
    pub modes: &'static [ModeSpec],
    pub mallet_hardness: f32,
    pub noise_amt: f32,
    pub tremolo_hz: f32,
    pub tremolo_depth: f32,
    pub release_time: f32,
    pub re_strike_delay: f32,
}

/// 4-operator FM routing algorithm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FmAlgorithm {
    /// op3 -> op2 -> op1 -> op0
    Stack4,
    /// (op3 -> op2) + (op1 -> op0)
    Dual2x2,
    /// (op3 + op2 + op1) -> op0
    ThreeToOne,
    /// op3 + op2 + op1 + op0 (additive)
    Parallel,
}

/// Single FM operator configuration.
#[derive(Clone, Copy, Debug)]
pub struct FmOperator {
    pub ratio: f32,
    pub detune_cents: f32,
    pub level: f32,
    pub decay: f32,
    pub sustain: f32,
    pub vel_sens: f32,
    pub fixed_hz: f32,
}

impl FmOperator {
    pub const fn new(
        ratio: f32,
        detune_cents: f32,
        level: f32,
        decay: f32,
        sustain: f32,
        vel_sens: f32,
    ) -> Self {
        Self {
            ratio,
            detune_cents,
            level,
            decay,
            sustain,
            vel_sens,
            fixed_hz: 0.0,
        }
    }
}

/// Parameters for 4-operator FM synthesis.
#[derive(Clone, Copy, Debug)]
pub struct FmParams {
    pub algorithm: FmAlgorithm,
    pub ops: [FmOperator; 4],
    pub feedback: f32,
    pub adsr: AdsrParams,
    pub vibrato_rate: f32,
    pub vibrato_depth_cents: f32,
}

/// Parameters for 9-drawbar additive organ / free-reed keyboard synthesis.
#[derive(Clone, Copy, Debug)]
pub struct OrganParams {
    pub drawbars: [f32; 9],
    pub key_click: f32,
    pub percussion_level: f32,
    pub percussion_harmonic: f32,
    pub rotary_speed: f32,
    pub rotary_depth: f32,
    pub overdrive: f32,
    pub breathiness: f32,
    pub adsr: AdsrParams,
}

/// Parameters for subtractive / SuperSaw / formant / pitch-sweep synthesis.
#[derive(Clone, Copy, Debug)]
pub struct SubParams {
    pub osc1_wave: Waveform,
    pub osc2_wave: Waveform,
    pub osc2_semitones: f32,
    pub osc2_detune_cents: f32,
    pub osc2_level: f32,
    pub unison_voices: u8,
    pub unison_detune_cents: f32,
    pub stereo_spread: f32,
    pub sub_level: f32,
    pub noise_level: f32,
    pub filter_mode: SvfMode,
    pub cutoff_hz: f32,
    pub resonance_q: f32,
    pub filter_env_amt: f32,
    pub filter_env_decay: f32,
    pub keytrack: f32,
    pub formant: Option<FormantPreset>,
    pub formant_mix: f32,
    pub pitch_env_semitones: f32,
    pub pitch_env_decay: f32,
    pub adsr: AdsrParams,
    pub lfo_rate: f32,
    pub lfo_depth_cents: f32,
    pub lfo_filter_amt: f32,
}

/// Unified synthesis patch enum covering all 8 acoustic & hybrid synthesis engines.
#[derive(Clone, Copy, Debug)]
pub enum SynthEngineKind {
    PluckedString(PluckedParams),
    HammeredString(HammeredParams),
    BowedString(BowedParams),
    WindBore(WindParams),
    ModalPercussion(ModalParams),
    Fm4Op(FmParams),
    AdditiveOrgan(OrganParams),
    FormantSubtractive(SubParams),
}

impl SynthEngineKind {
    pub const fn engine_name(&self) -> &'static str {
        match self {
            SynthEngineKind::PluckedString(_) => "PluckedString (Waveguide)",
            SynthEngineKind::HammeredString(_) => "HammeredString (Unison Piano Waveguide)",
            SynthEngineKind::BowedString(_) => "BowedString (Slip-Stick Waveguide)",
            SynthEngineKind::WindBore(_) => "WindBore (Aerodynamic/Reed Waveguide)",
            SynthEngineKind::ModalPercussion(_) => "ModalPercussion (2-Pole Modal Bank)",
            SynthEngineKind::Fm4Op(_) => "Fm4Op (4-Operator Phase Modulation)",
            SynthEngineKind::AdditiveOrgan(_) => "AdditiveOrgan (9-Drawbar + Rotary)",
            SynthEngineKind::FormantSubtractive(_) => "FormantSubtractive (PolyBLEP + ZDF SVF)",
        }
    }
}

/// Complete General MIDI melodic preset metadata and synthesis patch.
#[derive(Clone, Copy, Debug)]
pub struct PresetMetadata {
    pub program: u8,
    /// MIDI bank (CC 0 / Bank Select MSB): 0 = General MIDI, 1 = SoundHall 和楽器 bank.
    pub bank: u8,
    pub name_en: &'static str,
    pub name_ja: &'static str,
    pub family: GmFamily,
    pub gain: f32,
    pub default_reverb: f32,
    pub default_chorus: f32,
    pub pitch_range: (u8, u8),
    pub patch: SynthEngineKind,
}
