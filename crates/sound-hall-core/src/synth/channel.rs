/// Per-channel MIDI state (16 channels).
#[derive(Clone, Debug)]
pub struct ChannelState {
    pub program: u8,
    /// Bank Select MSB (CC 0); selects the preset bank applied at the next note-on.
    pub bank_msb: u8,
    pub is_drum: bool,
    pub volume: f32,
    pub pan: f32,
    pub expression: f32,
    pub mod_wheel: f32,
    pub sustain: bool,
    pub reverb_send: f32,
    pub chorus_send: f32,
    /// Normalized pitch bend in [-1.0, 1.0].
    pub pitch_bend: f32,
    /// Pitch bend sensitivity in semitones (default 2.0).
    pub pitch_bend_range: f32,
    rpn_msb: u8,
    rpn_lsb: u8,
    /// Cached frequency ratio for the current bend (avoids a `powf` per voice per sample).
    bend_ratio: f32,
}

impl ChannelState {
    pub fn new(channel_idx: u8) -> Self {
        Self {
            program: 0,
            bank_msb: 0,
            is_drum: channel_idx == 9,
            volume: 100.0 / 127.0,
            pan: 0.5,
            expression: 1.0,
            mod_wheel: 0.0,
            sustain: false,
            reverb_send: 0.28,
            chorus_send: 0.0,
            pitch_bend: 0.0,
            pitch_bend_range: 2.0,
            rpn_msb: 127,
            rpn_lsb: 127,
            bend_ratio: 1.0,
        }
    }

    pub fn reset_controllers(&mut self) {
        self.volume = 100.0 / 127.0;
        self.pan = 0.5;
        self.expression = 1.0;
        self.mod_wheel = 0.0;
        self.sustain = false;
        self.pitch_bend = 0.0;
        self.pitch_bend_range = 2.0;
        self.rpn_msb = 127;
        self.rpn_lsb = 127;
        self.bend_ratio = 1.0;
    }

    fn refresh_bend_ratio(&mut self) {
        let semitones = self.pitch_bend * self.pitch_bend_range;
        self.bend_ratio = if semitones.abs() < 1e-5 {
            1.0
        } else {
            2.0_f32.powf(semitones / 12.0)
        };
    }

    /// Frequency ratio from current pitch bend and bend range.
    #[inline]
    pub fn pitch_bend_ratio(&self) -> f32 {
        self.bend_ratio
    }

    /// Linear channel level from CC 7 (volume) and CC 11 (expression).
    ///
    /// Uses the GM-recommended squared (≈40·log10) curve so CC sweeps feel even.
    #[inline]
    pub fn level(&self) -> f32 {
        let v = self.volume * self.expression;
        v * v
    }

    /// Applies a MIDI Control Change message.
    /// Returns `Some(false)` when sustain pedal is released (CC 64 < 64).
    pub fn apply_cc(&mut self, cc: u8, value: u8) -> Option<bool> {
        let norm = (value.min(127) as f32) / 127.0;
        match cc {
            0 => self.bank_msb = value.min(127),
            1 => self.mod_wheel = norm,
            7 => self.volume = norm,
            10 => self.pan = (value.min(127) as f32) / 127.0,
            11 => self.expression = norm,
            64 => {
                let next = value >= 64;
                let changed = self.sustain != next;
                self.sustain = next;
                if changed {
                    return Some(next);
                }
            }
            91 => self.reverb_send = norm,
            93 => self.chorus_send = norm,
            100 => self.rpn_lsb = value,
            101 => self.rpn_msb = value,
            6 => {
                // Data Entry MSB: RPN 0/0 sets pitch bend sensitivity in semitones
                if self.rpn_msb == 0 && self.rpn_lsb == 0 {
                    self.pitch_bend_range = (value.min(24) as f32).max(1.0);
                    self.refresh_bend_ratio();
                }
            }
            121 => self.reset_controllers(),
            _ => {}
        }
        None
    }

    pub fn set_pitch_bend_14bit(&mut self, value_14bit: u16) {
        let clamped = value_14bit.min(16383) as f32;
        self.pitch_bend = ((clamped - 8192.0) / 8192.0).clamp(-1.0, 1.0);
        self.refresh_bend_ratio();
    }
}
