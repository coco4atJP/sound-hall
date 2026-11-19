/// Standardized MIDI / sequencer events handled by `SoundHallEngine`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MidiEvent {
    NoteOn { channel: u8, note: u8, velocity: u8 },
    NoteOff { channel: u8, note: u8 },
    ProgramChange { channel: u8, program: u8 },
    ControlChange { channel: u8, cc: u8, value: u8 },
    PitchBend { channel: u8, value_14bit: u16 },
    AllNotesOff { channel: u8 },
    AllSoundOff { channel: u8 },
}

/// Time-stamped MIDI event in a parsed song or score.
#[derive(Clone, Debug, PartialEq)]
pub struct TimedEvent {
    pub time_sec: f64,
    pub event: MidiEvent,
}

/// Complete multi-track sequence ready for rendering by `MidiSequencer`.
#[derive(Clone, Debug)]
pub struct MidiSong {
    pub events: Vec<TimedEvent>,
    pub duration_secs: f64,
    pub track_names: Vec<String>,
}

impl MidiSong {
    pub fn new(mut events: Vec<TimedEvent>, track_names: Vec<String>) -> Self {
        events.sort_by(|a, b| a.time_sec.total_cmp(&b.time_sec));
        let duration_secs = events.last().map(|e| e.time_sec).unwrap_or(0.0);
        Self {
            events,
            duration_secs,
            track_names,
        }
    }
}
