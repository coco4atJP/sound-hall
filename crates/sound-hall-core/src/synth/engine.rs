use crate::dsp::effects::{HallReverb, MasterLimiter, StereoChorus};
use crate::dsp::filter::{SvfFilter, SvfMode};
use crate::dsp::math::equal_power_pan;
use crate::midi::event::MidiEvent;
use crate::presets::{get_bank_preset, get_drum_patch};

use super::channel::ChannelState;
use super::drum::DrumVoice;
use super::voice::Voice;

const SQRT2: f32 = core::f32::consts::SQRT_2;
/// Default user-facing master gain.
const DEFAULT_MASTER_GAIN: f32 = 0.85;
/// Fixed mix-bus headroom (-7 dB) so dense arrangements only touch the limiter on peaks.
const BUS_HEADROOM: f32 = 0.45;
/// Channel gain smoothing time constant (seconds) — removes zipper noise on CC 7/10/11.
const CHANNEL_SMOOTH_SEC: f32 = 0.008;
/// Subsonic master high-pass corner (Hz): removes DC and inaudible rumble that eats headroom.
const MASTER_HPF_HZ: f32 = 18.0;

/// Smoothed per-channel output gains (pan law × volume × expression).
#[derive(Clone, Copy, Debug)]
struct ChannelMix {
    gain_l: f32,
    gain_r: f32,
    level: f32,
    target_l: f32,
    target_r: f32,
    target_level: f32,
}

impl ChannelMix {
    fn from_state(ch: &ChannelState) -> Self {
        let mut m = Self {
            gain_l: 0.0,
            gain_r: 0.0,
            level: 0.0,
            target_l: 0.0,
            target_r: 0.0,
            target_level: 0.0,
        };
        m.retarget(ch);
        m.gain_l = m.target_l;
        m.gain_r = m.target_r;
        m.level = m.target_level;
        m
    }

    fn retarget(&mut self, ch: &ChannelState) {
        let (pan_l, pan_r) = equal_power_pan(ch.pan);
        let level = ch.level();
        self.target_l = pan_l * SQRT2 * level;
        self.target_r = pan_r * SQRT2 * level;
        self.target_level = level;
    }

    #[inline]
    fn tick(&mut self, k: f32) {
        self.gain_l += (self.target_l - self.gain_l) * k;
        self.gain_r += (self.target_r - self.gain_r) * k;
        self.level += (self.target_level - self.level) * k;
    }
}

/// Zero-sample physical modeling & hybrid General MIDI synthesis engine.
pub struct SoundHallEngine {
    sample_rate: f32,
    max_voices: usize,
    channels: [ChannelState; 16],
    mix: [ChannelMix; 16],
    smooth_k: f32,
    voices: Vec<Voice>,
    drums: Vec<DrumVoice>,
    chorus: StereoChorus,
    reverb: HallReverb,
    hpf_l: SvfFilter,
    hpf_r: SvfFilter,
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
        let channels: [ChannelState; 16] = core::array::from_fn(|i| ChannelState::new(i as u8));
        let mix = core::array::from_fn(|i| ChannelMix::from_state(&channels[i]));
        Self {
            sample_rate: sr,
            max_voices,
            channels,
            mix,
            smooth_k: 1.0 - (-1.0 / (CHANNEL_SMOOTH_SEC * sr)).exp(),
            voices: Vec::with_capacity(max_voices),
            drums: Vec::with_capacity(32),
            chorus: StereoChorus::new(sr),
            reverb: HallReverb::new(sr),
            hpf_l: SvfFilter::new(),
            hpf_r: SvfFilter::new(),
            limiter: MasterLimiter::new(sr),
            master_gain: DEFAULT_MASTER_GAIN,
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
            DEFAULT_MASTER_GAIN
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
        let preset = get_bank_preset(ch.bank_msb, ch.program);
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

    /// Bank Select (CC 0) followed by Program Change: selects any preset, e.g. one from the
    /// 和楽器 bank (`bank = 1`).
    pub fn select_bank_program(&mut self, channel: u8, bank: u8, program: u8) {
        self.control_change(channel, 0, bank);
        self.program_change(channel, program);
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
                self.mix[ch_idx].retarget(&self.channels[ch_idx]);
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

        let pedal = self.channels[ch_idx].apply_cc(cc, value);
        if matches!(cc, 7 | 10 | 11) {
            self.mix[ch_idx].retarget(&self.channels[ch_idx]);
        }
        if let Some(false) = pedal {
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
        self.hpf_l.reset();
        self.hpf_r.reset();
        self.limiter.reset();
    }

    pub fn reset(&mut self) {
        self.all_sound_off();
        for (i, ch) in self.channels.iter_mut().enumerate() {
            *ch = ChannelState::new(i as u8);
            self.mix[i] = ChannelMix::from_state(ch);
        }
    }

    /// Current master limiter gain (1.0 = not limiting); useful for metering.
    pub fn limiter_gain(&self) -> f32 {
        self.limiter.gain()
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
        let k = self.smooth_k;
        for m in &mut self.mix {
            m.tick(k);
        }

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
            let c = voice.channel as usize;
            let ch = &self.channels[c];
            let (vl, vr) = voice.render_sample(sr, ch.pitch_bend_ratio(), ch.mod_wheel, 1.0);
            let mix = &self.mix[c];
            let sl = vl * mix.gain_l;
            let sr_out = vr * mix.gain_r;

            dry_l += sl;
            dry_r += sr_out;
            cho_in_l += sl * voice.chorus_send;
            cho_in_r += sr_out * voice.chorus_send;
            rev_in_l += sl * voice.reverb_send;
            rev_in_r += sr_out * voice.reverb_send;
        }

        let drum_ch = &self.channels[9];
        let drum_level = self.mix[9].level * SQRT2;
        for drum in &mut self.drums {
            if drum.is_finished() {
                continue;
            }
            let d = drum.render_sample(sr);
            let (pan_l, pan_r) = drum.pan_gains(drum_ch.pan);
            let sl = d * pan_l * drum_level;
            let sr_out = d * pan_r * drum_level;

            dry_l += sl;
            dry_r += sr_out;
            let rev_amt = drum_ch.reverb_send * 0.65 * drum.reverb_scale;
            rev_in_l += sl * rev_amt;
            rev_in_r += sr_out * rev_amt;
        }

        let (cho_l, cho_r) = self.chorus.process(cho_in_l, cho_in_r, sr);
        let (rev_l, rev_r) = self.reverb.process_send(rev_in_l, rev_in_r);

        let bus = self.master_gain * BUS_HEADROOM;
        let mix_l = (dry_l + cho_l + rev_l) * bus;
        let mix_r = (dry_r + cho_r + rev_r) * bus;

        let hp_l = self
            .hpf_l
            .process(mix_l, MASTER_HPF_HZ, 0.707, sr, SvfMode::HighPass);
        let hp_r = self
            .hpf_r
            .process(mix_r, MASTER_HPF_HZ, 0.707, sr, SvfMode::HighPass);

        self.limiter.process(hp_l, hp_r)
    }

    fn prune_finished(&mut self) {
        self.voices.retain(|v| !v.is_finished());
        self.drums.retain(|d| !d.is_finished());
    }
}
