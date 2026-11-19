use crate::dsp::effects::{HallReverb, MasterLimiter, StereoChorus};
use crate::dsp::math::equal_power_pan;
use crate::midi::event::MidiEvent;
use crate::presets::{get_drum_patch, get_preset};

use super::channel::ChannelState;
use super::drum::DrumVoice;
use super::voice::Voice;

const SQRT2: f32 = core::f32::consts::SQRT_2;

/// Zero-sample physical modeling & hybrid General MIDI synthesis engine.
pub struct SoundHallEngine {
    sample_rate: f32,
    max_voices: usize,
    channels: [ChannelState; 16],
    voices: Vec<Voice>,
    drums: Vec<DrumVoice>,
    chorus: StereoChorus,
    reverb: HallReverb,
    limiter: MasterLimiter,
    master_gain: f32,
    age_counter: u64,
}

impl SoundHallEngine {
    /// Creates a new engine with default 64-voice polyphony.
    pub fn new(sample_rate: f32) -> Self {
        Self::with_polyphony(sample_rate, 64)
    }

    /// Creates a new engine with configurable sample rate and maximum polyphony.
    pub fn with_polyphony(sample_rate: f32, max_voices: usize) -> Self {
        let sr = if sample_rate.is_finite() {
            sample_rate.clamp(8000.0, 192_000.0)
        } else {
            44100.0
        };
        let max_voices = max_voices.clamp(4, 256);
        let channels = core::array::from_fn(|i| ChannelState::new(i as u8));
        Self {
            sample_rate: sr,
            max_voices,
            channels,
            voices: Vec::with_capacity(max_voices),
            drums: Vec::with_capacity(32),
            chorus: StereoChorus::new(sr),
            reverb: HallReverb::new(sr),
            limiter: MasterLimiter::new(),
            master_gain: 0.85,
            age_counter: 1,
        }
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn max_voices(&self) -> usize {
        self.max_voices
    }

    pub fn set_master_gain(&mut self, gain: f32) {
        self.master_gain = if gain.is_finite() {
            gain.clamp(0.0, 2.5)
        } else {
            0.85
        };
    }

    pub fn master_gain(&self) -> f32 {
        self.master_gain
    }

    pub fn set_reverb_mix(&mut self, mix: f32) {
        self.reverb.set_mix(mix);
    }

    pub fn reverb_mix(&self) -> f32 {
        self.reverb.mix()
    }

    pub fn set_chorus_mix(&mut self, mix: f32) {
        self.chorus.set_mix(mix);
    }

    pub fn chorus_mix(&self) -> f32 {
        self.chorus.mix()
    }

    pub fn channel_state(&self, channel: u8) -> &ChannelState {
        &self.channels[(channel & 0x0F) as usize]
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| !v.is_finished()).count()
            + self.drums.iter().filter(|d| !d.is_finished()).count()
    }

    /// Triggers a Note On event (velocity 0 is treated as Note Off per MIDI spec).
    pub fn note_on(&mut self, channel: u8, note: u8, velocity: u8) {
        let ch_idx = (channel & 0x0F) as usize;
        let note = note & 0x7F;
        if velocity == 0 {
            self.note_off(channel, note);
            return;
        }

        self.age_counter = self.age_counter.wrapping_add(1);
        let age = self.age_counter;

        if self.channels[ch_idx].is_drum {
            let patch = get_drum_patch(note);
            if patch.choke_group != 0 {
                for d in &mut self.drums {
                    if d.choke_group == patch.choke_group && !d.is_finished() {
                        d.choke();
                    }
                }
            }
            self.ensure_voice_headroom();
            self.drums
                .push(DrumVoice::new(patch, velocity, self.sample_rate, age));
            return;
        }

        // Retrigger same (channel, note): gracefully fade existing voice
        for v in &mut self.voices {
            if v.channel == ch_idx as u8 && v.note == note && !v.is_finished() {
                v.force_steal();
            }
        }

        self.ensure_voice_headroom();

        let ch = &self.channels[ch_idx];
        let preset = get_preset(ch.program);
        let voice = Voice::new(
            ch_idx as u8,
            note,
            velocity,
            preset,
            self.sample_rate,
            age,
            ch.reverb_send,
            ch.chorus_send,
        );
        self.voices.push(voice);
    }

    /// Triggers a Note Off event (respects Sustain Pedal CC 64).
    pub fn note_off(&mut self, channel: u8, note: u8) {
        let ch_idx = (channel & 0x0F) as usize;
        let note = note & 0x7F;
        if self.channels[ch_idx].is_drum {
            return;
        }
        let sustain_held = self.channels[ch_idx].sustain;
        for v in &mut self.voices {
            if v.channel == ch_idx as u8 && v.note == note && !v.is_finished() && !v.stealing {
                if sustain_held {
                    v.sustained = true;
                } else {
                    v.release();
                }
            }
        }
    }

    pub fn program_change(&mut self, channel: u8, program: u8) {
        let ch_idx = (channel & 0x0F) as usize;
        self.channels[ch_idx].program = program & 0x7F;
    }

    pub fn control_change(&mut self, channel: u8, cc: u8, value: u8) {
        let ch_idx = (channel & 0x0F) as usize;
        match cc {
            120 => {
                // All Sound Off on channel
                self.voices.retain(|v| v.channel != ch_idx as u8);
                if self.channels[ch_idx].is_drum {
                    self.drums.clear();
                }
                return;
            }
            121 => {
                // Reset All Controllers also releases any sustained pedal notes on channel
                self.channels[ch_idx].reset_controllers();
                for v in &mut self.voices {
                    if v.channel == ch_idx as u8 && v.sustained && !v.is_finished() {
                        v.release();
                    }
                }
                return;
            }
            123..=127 => {
                // All Notes Off on channel
                self.channels[ch_idx].sustain = false;
                for v in &mut self.voices {
                    if v.channel == ch_idx as u8 && !v.is_finished() {
                        v.release();
                    }
                }
                return;
            }
            _ => {}
        }

        if let Some(false) = self.channels[ch_idx].apply_cc(cc, value) {
            // Sustain pedal released: release all sustained notes on this channel
            for v in &mut self.voices {
                if v.channel == ch_idx as u8 && v.sustained && !v.is_finished() {
                    v.release();
                }
            }
        }
    }

    pub fn pitch_bend(&mut self, channel: u8, value_14bit: u16) {
        let ch_idx = (channel & 0x0F) as usize;
        self.channels[ch_idx].set_pitch_bend_14bit(value_14bit);
    }

    pub fn handle_event(&mut self, event: &MidiEvent) {
        match *event {
            MidiEvent::NoteOn {
                channel,
                note,
                velocity,
            } => self.note_on(channel, note, velocity),
            MidiEvent::NoteOff { channel, note } => self.note_off(channel, note),
            MidiEvent::ProgramChange { channel, program } => self.program_change(channel, program),
            MidiEvent::ControlChange { channel, cc, value } => {
                self.control_change(channel, cc, value)
            }
            MidiEvent::PitchBend {
                channel,
                value_14bit,
            } => self.pitch_bend(channel, value_14bit),
            MidiEvent::AllNotesOff { channel } => self.control_change(channel, 123, 0),
            MidiEvent::AllSoundOff { channel } => self.control_change(channel, 120, 0),
        }
    }

    pub fn all_notes_off(&mut self) {
        for ch in &mut self.channels {
            ch.sustain = false;
        }
        for v in &mut self.voices {
            if !v.is_finished() {
                v.release();
            }
        }
    }

    pub fn all_sound_off(&mut self) {
        self.voices.clear();
        self.drums.clear();
        self.chorus.reset();
        self.reverb.reset();
    }

    pub fn reset(&mut self) {
        self.all_sound_off();
        for (i, ch) in self.channels.iter_mut().enumerate() {
            *ch = ChannelState::new(i as u8);
        }
    }

    fn ensure_voice_headroom(&mut self) {
        self.voices.retain(|v| !v.is_finished());
        self.drums.retain(|d| !d.is_finished());

        let total = self.voices.len() + self.drums.len();
        if total < self.max_voices {
            return;
        }

        // First try stealing the oldest releasing melodic voice
        if let Some((idx, _)) = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_releasing() && !v.stealing)
            .min_by_key(|(_, v)| v.age)
        {
            self.voices[idx].force_steal();
        } else if let Some((idx, _)) = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.stealing)
            .min_by(|(_, a), (_, b)| {
                a.estimated_level()
                    .total_cmp(&b.estimated_level())
                    .then_with(|| a.age.cmp(&b.age))
            })
        {
            self.voices[idx].force_steal();
        } else if !self.drums.is_empty() {
            if let Some((idx, _)) = self.drums.iter().enumerate().min_by_key(|(_, d)| d.age) {
                self.drums.swap_remove(idx);
            }
        }

        // Hard cap if oversaturated by rapid events in same sample block
        if self.voices.len() + self.drums.len() >= self.max_voices + 8 {
            if let Some((idx, _)) = self.voices.iter().enumerate().min_by_key(|(_, v)| v.age) {
                self.voices.swap_remove(idx);
            }
        }
    }

    /// Renders stereo interleaved `[L0, R0, L1, R1, ...]` samples.
    pub fn render_stereo(&mut self, out_interleaved: &mut [f32]) {
        for frame in out_interleaved.as_chunks_mut::<2>().0 {
            let (l, r) = self.render_frame();
            frame[0] = l;
            frame[1] = r;
        }
        self.prune_finished();
    }

    /// Renders stereo planar `(left, right)` buffers of matching frame count.
    pub fn render_planar(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        for i in 0..frames {
            let (l, r) = self.render_frame();
            left[i] = l;
            right[i] = r;
        }
        self.prune_finished();
    }

    #[inline]
    fn render_frame(&mut self) -> (f32, f32) {
        let sr = self.sample_rate;
        let mut dry_l = 0.0_f32;
        let mut dry_r = 0.0_f32;
        let mut cho_in_l = 0.0_f32;
        let mut cho_in_r = 0.0_f32;
        let mut rev_in_l = 0.0_f32;
        let mut rev_in_r = 0.0_f32;

        for voice in &mut self.voices {
            if voice.is_finished() {
                continue;
            }
            let ch = &self.channels[voice.channel as usize];
            let bend_ratio = ch.pitch_bend_ratio();
            let (vl, vr) = voice.render_sample(sr, bend_ratio, ch.mod_wheel, ch.expression);
            let (pan_l, pan_r) = equal_power_pan(ch.pan);
            let gain_l = pan_l * SQRT2 * ch.volume;
            let gain_r = pan_r * SQRT2 * ch.volume;
            let sl = vl * gain_l;
            let sr_out = vr * gain_r;

            dry_l += sl;
            dry_r += sr_out;
            cho_in_l += sl * voice.chorus_send;
            cho_in_r += sr_out * voice.chorus_send;
            rev_in_l += sl * voice.reverb_send;
            rev_in_r += sr_out * voice.reverb_send;
        }

        for drum in &mut self.drums {
            if drum.is_finished() {
                continue;
            }
            let ch = &self.channels[9];
            let d = drum.render_sample(sr);
            let drum_pan = (drum.pan + (ch.pan - 0.5)).clamp(0.0, 1.0);
            let (pan_l, pan_r) = equal_power_pan(drum_pan);
            let amp = ch.volume * ch.expression * SQRT2;
            let sl = d * pan_l * amp;
            let sr_out = d * pan_r * amp;

            dry_l += sl;
            dry_r += sr_out;
            let rev_amt = ch.reverb_send.max(0.22) * 0.65;
            rev_in_l += sl * rev_amt;
            rev_in_r += sr_out * rev_amt;
        }

        let (cho_l, cho_r) = self.chorus.process(cho_in_l, cho_in_r, sr);
        let (rev_l, rev_r) = self.reverb.process_send(rev_in_l, rev_in_r);

        let mix_l = (dry_l + cho_l + rev_l) * self.master_gain;
        let mix_r = (dry_r + cho_r + rev_r) * self.master_gain;

        (self.limiter.process(mix_l), self.limiter.process(mix_r))
    }

    fn prune_finished(&mut self) {
        self.voices.retain(|v| !v.is_finished());
        self.drums.retain(|d| !d.is_finished());
    }
}
