//! # SoundHall Core (`sound-hall-core`)
//!
//! Lightweight, zero-dependency, zero-sample physical modeling & hybrid General MIDI
//! synthesis engine in pure Rust.

pub mod dsp;
pub mod midi;
pub mod presets;
pub mod score;
pub mod synth;
pub mod wav;

pub use midi::event::{MidiEvent, MidiSong, TimedEvent};
pub use midi::smf::{parse_smf, MidiSequencer, SmfError};
pub use presets::{
    all_bank_presets, all_drums, all_presets, bank_presets, find_preset, get_bank_preset,
    get_drum_info, get_drum_patch, get_preset, search_presets, wagakki_presets, WAGAKKI_BANK,
    WAGAKKI_COUNT,
};
pub use score::demos::{get_demo, list_demos, parse_demo, DemoSong};
pub use score::{parse_score, ScoreError};
pub use synth::drum::DrumPatch;
pub use synth::engine::SoundHallEngine;
pub use synth::patch::{GmFamily, PresetMetadata, SynthEngineKind};
pub use wav::{write_wav_f32, write_wav_i16};
