//! Zero-dependency WebAssembly C-ABI bindings for `sound-hall-core`.
//!
//! Includes live MIDI synthesis, sample-accurate sequencer playback for built-in demos,
//! `.shs` scores, and Standard MIDI Files (`.mid`), real-time keyboard/channel telemetry,
//! and in-memory 16-bit PCM WAV rendering.

use std::alloc::{alloc, dealloc, Layout};
use std::slice;

use sound_hall_core::{
    all_drums, all_presets, list_demos, parse_demo, parse_score, parse_smf, write_wav_i16,
    MidiEvent, MidiSequencer, MidiSong, SoundHallEngine,
};

/// Opaque studio state wrapping `SoundHallEngine` with a real-time sequencer,
/// visual telemetry, error reporting, and WAV export buffer.
pub struct WasmStudio {
    engine: SoundHallEngine,
    song: Option<MidiSong>,
    seq_playing: bool,
    seq_cursor_frame: usize,
    seq_event_idx: usize,
    seq_total_frames: usize,
    active_notes_bitmap: [u64; 2],
    channel_programs: [u8; 16],
    channel_last_note: [u8; 16],
    channel_gate_frames: [u32; 16],
    gate_max_frames: u32,
    last_error: Vec<u8>,
    wav_buffer: Vec<u8>,
}

impl WasmStudio {
    pub fn new(sample_rate: f32, max_voices: usize) -> Self {
        let engine = SoundHallEngine::with_polyphony(sample_rate, max_voices);
        let sr = engine.sample_rate();
        let gate_max_frames = ((0.22 * sr).round() as u32).max(1);
        Self {
            engine,
            song: None,
            seq_playing: false,
            seq_cursor_frame: 0,
            seq_event_idx: 0,
            seq_total_frames: 0,
            active_notes_bitmap: [0; 2],
            channel_programs: [0; 16],
            channel_last_note: [60; 16],
            channel_gate_frames: [0; 16],
            gate_max_frames,
            last_error: Vec::new(),
            wav_buffer: Vec::new(),
        }
    }

    #[inline]
    fn set_note_bit(&mut self, note: u8, active: bool) {
        let n = (note & 0x7F) as usize;
        let word = n >> 6;
        let bit = n & 63;
        if active {
            self.active_notes_bitmap[word] |= 1u64 << bit;
        } else {
            self.active_notes_bitmap[word] &= !(1u64 << bit);
        }
    }

    fn clear_telemetry_notes(&mut self) {
        self.active_notes_bitmap = [0; 2];
        self.channel_gate_frames = [0; 16];
    }

    pub fn note_on(&mut self, ch: u8, note: u8, vel: u8) {
        let c = (ch & 0x0F) as usize;
        let n = note & 0x7F;
        if vel == 0 {
            self.note_off(ch, n);
            return;
        }
        self.set_note_bit(n, true);
        self.channel_last_note[c] = n;
        let scaled_gate =
            ((self.gate_max_frames as f32) * (0.4 + 0.6 * (vel.min(127) as f32 / 127.0))) as u32;
        self.channel_gate_frames[c] = scaled_gate.max(1);
        self.engine.note_on(ch, n, vel);
    }

    pub fn note_off(&mut self, ch: u8, note: u8) {
        let n = note & 0x7F;
        self.set_note_bit(n, false);
        self.engine.note_off(ch, n);
    }

    pub fn program_change(&mut self, ch: u8, program: u8) {
        let c = (ch & 0x0F) as usize;
        self.channel_programs[c] = program & 0x7F;
        self.engine.program_change(ch, program);
    }

    pub fn control_change(&mut self, ch: u8, cc: u8, value: u8) {
        if matches!(cc, 120 | 123..=127) {
            self.active_notes_bitmap = [0; 2];
        }
        self.engine.control_change(ch, cc, value);
    }

    pub fn pitch_bend(&mut self, ch: u8, value_14bit: u16) {
        self.engine.pitch_bend(ch, value_14bit);
    }

    pub fn all_notes_off(&mut self) {
        self.active_notes_bitmap = [0; 2];
        self.engine.all_notes_off();
    }

    pub fn reset(&mut self) {
        self.seq_playing = false;
        self.seq_cursor_frame = 0;
        self.seq_event_idx = 0;
        self.clear_telemetry_notes();
        self.channel_programs = [0; 16];
        self.engine.reset();
    }

    fn apply_midi_event(&mut self, ev: &MidiEvent) {
        match *ev {
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

    fn set_loaded_song(&mut self, song: MidiSong) -> f32 {
        let sr = self.engine.sample_rate() as f64;
        let total_secs = (song.duration_secs + 1.2).clamp(0.2, MidiSequencer::MAX_RENDER_SECONDS);
        self.seq_total_frames = ((total_secs * sr).ceil() as usize).max(1);
        self.seq_cursor_frame = 0;
        self.seq_event_idx = 0;
        self.seq_playing = false;
        self.clear_telemetry_notes();
        self.engine.all_notes_off();

        // Prime initial program changes at t == 0.0 for immediate UI display
        for ev in &song.events {
            if ev.time_sec > 0.0001 {
                break;
            }
            if let MidiEvent::ProgramChange { channel, program } = ev.event {
                self.channel_programs[(channel & 0x0F) as usize] = program & 0x7F;
                self.engine.program_change(channel, program);
            }
        }

        let dur = song.duration_secs as f32;
        self.song = Some(song);
        self.last_error.clear();
        dur
    }

    fn decay_gates(&mut self, frames: usize) {
        let step = frames as u32;
        for g in &mut self.channel_gate_frames {
            *g = g.saturating_sub(step);
        }
    }

    fn render_planar_internal(&mut self, left: &mut [f32], right: &mut [f32]) {
        let total_frames = left.len().min(right.len());
        if !self.seq_playing || self.song.is_none() {
            self.engine
                .render_planar(&mut left[..total_frames], &mut right[..total_frames]);
            self.decay_gates(total_frames);
            return;
        }

        let sr = self.engine.sample_rate() as f64;
        let mut offset = 0usize;
        while offset < total_frames {
            if !self.seq_playing {
                self.engine.render_planar(
                    &mut left[offset..total_frames],
                    &mut right[offset..total_frames],
                );
                self.decay_gates(total_frames - offset);
                break;
            }

            // Fire any due events at or before current seq_cursor_frame
            loop {
                let maybe_ev = self.song.as_ref().and_then(|s| {
                    if self.seq_event_idx < s.events.len() {
                        let ev_sec = s.events[self.seq_event_idx].time_sec;
                        let ev_frame = (ev_sec.max(0.0) * sr).round() as usize;
                        if ev_frame <= self.seq_cursor_frame {
                            Some(s.events[self.seq_event_idx].event.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                });
                if let Some(ev) = maybe_ev {
                    self.seq_event_idx += 1;
                    self.apply_midi_event(&ev);
                } else {
                    break;
                }
            }

            let next_ev_frame = self
                .song
                .as_ref()
                .and_then(|s| {
                    s.events.get(self.seq_event_idx).map(|e| {
                        ((e.time_sec.max(0.0) * sr).round() as usize).min(self.seq_total_frames)
                    })
                })
                .unwrap_or(self.seq_total_frames);

            let remaining_in_buf = total_frames - offset;
            let until_next = next_ev_frame
                .saturating_sub(self.seq_cursor_frame)
                .clamp(1, 64);
            let step = remaining_in_buf.min(until_next);

            self.engine.render_planar(
                &mut left[offset..offset + step],
                &mut right[offset..offset + step],
            );
            self.decay_gates(step);
            offset += step;
            self.seq_cursor_frame += step;

            if self.seq_cursor_frame >= self.seq_total_frames {
                self.seq_playing = false;
                self.seq_cursor_frame = 0;
                self.seq_event_idx = 0;
                self.all_notes_off();
            }
        }
    }

    fn render_stereo_internal(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        if frames == 0 {
            return;
        }
        let mut left = vec![0.0_f32; frames];
        let mut right = vec![0.0_f32; frames];
        self.render_planar_internal(&mut left, &mut right);
        for (i, pair) in out.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            pair[0] = left[i];
            pair[1] = right[i];
        }
    }

    pub fn seek_sec(&mut self, target_sec: f32) {
        let Some(ref song) = self.song else {
            return;
        };
        let sr = self.engine.sample_rate() as f64;
        let clamped_sec = if target_sec.is_finite() {
            (target_sec as f64).clamp(0.0, song.duration_secs)
        } else {
            0.0
        };
        let target_frame = (clamped_sec * sr).round() as usize;
        self.engine.all_notes_off();
        self.active_notes_bitmap = [0; 2];
        self.channel_gate_frames = [0; 16];

        let mut idx = 0usize;
        while idx < song.events.len() {
            let ev_frame = (song.events[idx].time_sec.max(0.0) * sr).round() as usize;
            if ev_frame < target_frame {
                // Apply non-note state changes (ProgramChange, ControlChange, PitchBend) up to seek target
                match song.events[idx].event {
                    MidiEvent::ProgramChange { channel, program } => {
                        self.channel_programs[(channel & 0x0F) as usize] = program & 0x7F;
                        self.engine.program_change(channel, program);
                    }
                    MidiEvent::ControlChange { channel, cc, value } => {
                        self.engine.control_change(channel, cc, value);
                    }
                    MidiEvent::PitchBend {
                        channel,
                        value_14bit,
                    } => {
                        self.engine.pitch_bend(channel, value_14bit);
                    }
                    _ => {}
                }
                idx += 1;
            } else {
                break;
            }
        }
        self.seq_cursor_frame = target_frame.min(self.seq_total_frames);
        self.seq_event_idx = idx;
    }
}

/// Allocates `len` bytes in WASM linear memory for host-side buffers.
#[no_mangle]
pub extern "C" fn sh_alloc(len: usize) -> *mut u8 {
    let size = len.max(1);
    let layout = match Layout::from_size_align(size, 8) {
        Ok(l) => l,
        Err(_) => return core::ptr::null_mut(),
    };
    unsafe { alloc(layout) }
}

/// Frees `len` bytes previously allocated with `sh_alloc`.
///
/// # Safety
/// `ptr` must have been allocated by `sh_alloc(len)` and not already freed.
#[no_mangle]
pub unsafe extern "C" fn sh_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    let size = len.max(1);
    if let Ok(layout) = Layout::from_size_align(size, 8) {
        dealloc(ptr, layout);
    }
}

/// Instantiates a new `WasmStudio` on the heap and returns its opaque pointer.
#[no_mangle]
pub extern "C" fn sh_engine_new(sample_rate: f32, max_voices: u32) -> *mut WasmStudio {
    let voices = if max_voices == 0 {
        64
    } else {
        max_voices as usize
    };
    let studio = WasmStudio::new(sample_rate, voices);
    Box::into_raw(Box::new(studio))
}

/// Destroys a `WasmStudio` instance created with `sh_engine_new`.
///
/// # Safety
/// `ptr` must be a valid non-null pointer from `sh_engine_new` or null.
#[no_mangle]
pub unsafe extern "C" fn sh_engine_free(ptr: *mut WasmStudio) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

/// Triggers MIDI Note On (`velocity == 0` acts as Note Off).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_note_on(ptr: *mut WasmStudio, ch: u8, note: u8, vel: u8) {
    if let Some(studio) = ptr.as_mut() {
        studio.note_on(ch, note, vel);
    }
}

/// Triggers MIDI Note Off.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_note_off(ptr: *mut WasmStudio, ch: u8, note: u8) {
    if let Some(studio) = ptr.as_mut() {
        studio.note_off(ch, note);
    }
}

/// Changes General MIDI program (`0..=127`) on channel `0..=15`.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_program_change(ptr: *mut WasmStudio, ch: u8, program: u8) {
    if let Some(studio) = ptr.as_mut() {
        studio.program_change(ch, program);
    }
}

/// Sends a MIDI Control Change message.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_control_change(ptr: *mut WasmStudio, ch: u8, cc: u8, value: u8) {
    if let Some(studio) = ptr.as_mut() {
        studio.control_change(ch, cc, value);
    }
}

/// Sets 14-bit Pitch Bend (`8192` = center).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_pitch_bend(ptr: *mut WasmStudio, ch: u8, value_14bit: u16) {
    if let Some(studio) = ptr.as_mut() {
        studio.pitch_bend(ch, value_14bit);
    }
}

/// Releases all active notes gracefully.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_all_notes_off(ptr: *mut WasmStudio) {
    if let Some(studio) = ptr.as_mut() {
        studio.all_notes_off();
    }
}

/// Resets channels, stops sequencer, and kills all sounding voices immediately.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_reset(ptr: *mut WasmStudio) {
    if let Some(studio) = ptr.as_mut() {
        studio.reset();
    }
}

/// Sets master output gain (default `0.85`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_set_master_gain(ptr: *mut WasmStudio, gain: f32) {
    if let Some(studio) = ptr.as_mut() {
        studio.engine.set_master_gain(gain);
    }
}

/// Sets global hall reverb wet mix (`0.0..=1.0`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_set_reverb_mix(ptr: *mut WasmStudio, mix: f32) {
    if let Some(studio) = ptr.as_mut() {
        studio.engine.set_reverb_mix(mix);
    }
}

/// Sets global stereo chorus wet mix (`0.0..=1.0`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_set_chorus_mix(ptr: *mut WasmStudio, mix: f32) {
    if let Some(studio) = ptr.as_mut() {
        studio.engine.set_chorus_mix(mix);
    }
}

/// Loads one of the 5 built-in demo songs (`0..=4`) into the sequencer.
/// Returns song duration in seconds, or `-1.0` if `demo_index` is invalid.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_load_demo(ptr: *mut WasmStudio, demo_index: u32) -> f32 {
    let Some(studio) = ptr.as_mut() else {
        return -1.0;
    };
    let demos = list_demos();
    let Some(demo) = demos.get(demo_index as usize) else {
        studio.last_error = b"invalid demo index".to_vec();
        return -1.0;
    };
    match parse_demo(demo.id) {
        Some(song) => studio.set_loaded_song(song),
        None => {
            studio.last_error = b"failed to parse built-in demo".to_vec();
            -1.0
        }
    }
}

/// Parses and loads a UTF-8 `.shs` text score from WASM memory into the sequencer.
/// Returns duration in seconds on success, or `-1.0` on parse error (inspect via `sh_last_error_*`).
///
/// # Safety
/// `ptr` must be valid, and `text_ptr` must reference `text_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn sh_load_score(
    ptr: *mut WasmStudio,
    text_ptr: *const u8,
    text_len: usize,
) -> f32 {
    let Some(studio) = ptr.as_mut() else {
        return -1.0;
    };
    if text_ptr.is_null() || text_len == 0 || text_len > isize::MAX as usize {
        studio.last_error = b"empty score input".to_vec();
        return -1.0;
    }
    let raw = slice::from_raw_parts(text_ptr, text_len);
    let text = match core::str::from_utf8(raw) {
        Ok(s) => s,
        Err(e) => {
            studio.last_error = format!("UTF-8 error: {e}").into_bytes();
            return -1.0;
        }
    };
    match parse_score(text) {
        Ok(song) => studio.set_loaded_song(song),
        Err(e) => {
            studio.last_error = e.to_string().into_bytes();
            -1.0
        }
    }
}

/// Parses and loads a Standard MIDI File (`.mid`) from WASM memory into the sequencer.
/// Returns duration in seconds on success, or `-1.0` on parse error.
///
/// # Safety
/// `ptr` must be valid, and `bytes_ptr` must reference `bytes_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn sh_load_smf(
    ptr: *mut WasmStudio,
    bytes_ptr: *const u8,
    bytes_len: usize,
) -> f32 {
    let Some(studio) = ptr.as_mut() else {
        return -1.0;
    };
    if bytes_ptr.is_null() || bytes_len == 0 || bytes_len > isize::MAX as usize {
        studio.last_error = b"empty MIDI file".to_vec();
        return -1.0;
    }
    let raw = slice::from_raw_parts(bytes_ptr, bytes_len);
    match parse_smf(raw) {
        Ok(song) => studio.set_loaded_song(song),
        Err(e) => {
            studio.last_error = e.to_string().into_bytes();
            -1.0
        }
    }
}

/// Returns pointer to the last parse error UTF-8 bytes.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_last_error_ptr(ptr: *const WasmStudio) -> *const u8 {
    ptr.as_ref()
        .map(|s| s.last_error.as_ptr())
        .unwrap_or(core::ptr::null())
}

/// Returns byte length of the last parse error string.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_last_error_len(ptr: *const WasmStudio) -> usize {
    ptr.as_ref().map(|s| s.last_error.len()).unwrap_or(0)
}

/// Starts or resumes real-time sequencer playback of the loaded song.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_seq_play(ptr: *mut WasmStudio) {
    if let Some(studio) = ptr.as_mut() {
        if studio.song.is_some() {
            if studio.seq_cursor_frame >= studio.seq_total_frames {
                studio.seq_cursor_frame = 0;
                studio.seq_event_idx = 0;
            }
            studio.seq_playing = true;
        }
    }
}

/// Pauses sequencer playback and releases held notes.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_seq_pause(ptr: *mut WasmStudio) {
    if let Some(studio) = ptr.as_mut() {
        studio.seq_playing = false;
        studio.all_notes_off();
    }
}

/// Stops sequencer playback, rewinds cursor to `0.0`, and releases all notes.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_seq_stop(ptr: *mut WasmStudio) {
    if let Some(studio) = ptr.as_mut() {
        studio.seq_playing = false;
        studio.seq_cursor_frame = 0;
        studio.seq_event_idx = 0;
        studio.all_notes_off();
    }
}

/// Seeks sequencer playback to `time_sec` seconds.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_seq_seek(ptr: *mut WasmStudio, time_sec: f32) {
    if let Some(studio) = ptr.as_mut() {
        studio.seek_sec(time_sec);
    }
}

/// Returns `1` if the sequencer is currently playing, `0` otherwise.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_seq_is_playing(ptr: *const WasmStudio) -> u32 {
    ptr.as_ref().map(|s| u32::from(s.seq_playing)).unwrap_or(0)
}

/// Returns current sequencer playback position in seconds.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_seq_position_sec(ptr: *const WasmStudio) -> f32 {
    ptr.as_ref()
        .map(|s| (s.seq_cursor_frame as f32) / s.engine.sample_rate())
        .unwrap_or(0.0)
}

/// Returns total duration of the loaded song in seconds.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_seq_duration_sec(ptr: *const WasmStudio) -> f32 {
    ptr.as_ref()
        .and_then(|s| s.song.as_ref().map(|m| m.duration_secs as f32))
        .unwrap_or(0.0)
}

/// Returns low 64-bit bitmask of currently active MIDI notes (`0..=63`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_active_notes_low(ptr: *const WasmStudio) -> u64 {
    ptr.as_ref().map(|s| s.active_notes_bitmap[0]).unwrap_or(0)
}

/// Returns high 64-bit bitmask of currently active MIDI notes (`64..=127`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_active_notes_high(ptr: *const WasmStudio) -> u64 {
    ptr.as_ref().map(|s| s.active_notes_bitmap[1]).unwrap_or(0)
}

/// Returns current GM program number (`0..=127`) on channel `ch` (`0..=15`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_channel_program(ptr: *const WasmStudio, ch: u8) -> u8 {
    ptr.as_ref()
        .map(|s| s.channel_programs[(ch & 0x0F) as usize])
        .unwrap_or(0)
}

/// Returns normalized recent note activity (`0.0..=1.0`) on channel `ch` (`0..=15`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_channel_activity(ptr: *const WasmStudio, ch: u8) -> f32 {
    ptr.as_ref()
        .map(|s| {
            let g = s.channel_gate_frames[(ch & 0x0F) as usize] as f32;
            (g / s.gate_max_frames as f32).clamp(0.0, 1.0)
        })
        .unwrap_or(0.0)
}

/// Returns last triggered MIDI note (`0..=127`) on channel `ch` (`0..=15`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_channel_last_note(ptr: *const WasmStudio, ch: u8) -> u8 {
    ptr.as_ref()
        .map(|s| s.channel_last_note[(ch & 0x0F) as usize])
        .unwrap_or(60)
}

/// Renders the currently loaded song offline into an internal 16-bit PCM WAV buffer.
/// Returns the byte length of the WAV file (0 if no song is loaded).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_render_song_wav(ptr: *mut WasmStudio, normalize: u32) -> usize {
    let Some(studio) = ptr.as_mut() else {
        return 0;
    };
    let Some(ref song) = studio.song else {
        return 0;
    };
    let sr = studio.engine.sample_rate();
    let mut offline = SoundHallEngine::with_polyphony(sr, studio.engine.max_voices());
    offline.set_master_gain(studio.engine.master_gain());
    offline.set_reverb_mix(studio.engine.reverb_mix());
    offline.set_chorus_mix(studio.engine.chorus_mix());

    let samples = MidiSequencer::render_to_buffer_with_max_secs(song, &mut offline, 1.6, 300.0);
    studio.wav_buffer.clear();
    if write_wav_i16(
        &mut studio.wav_buffer,
        sr.round() as u32,
        2,
        &samples,
        normalize != 0,
    )
    .is_ok()
    {
        studio.wav_buffer.len()
    } else {
        0
    }
}

/// Returns pointer to the rendered WAV byte buffer.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_wav_ptr(ptr: *const WasmStudio) -> *const u8 {
    ptr.as_ref()
        .map(|s| s.wav_buffer.as_ptr())
        .unwrap_or(core::ptr::null())
}

/// Returns byte length of the rendered WAV buffer.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_wav_len(ptr: *const WasmStudio) -> usize {
    ptr.as_ref().map(|s| s.wav_buffer.len()).unwrap_or(0)
}

/// Renders `frames` interleaved stereo f32 frames (`frames * 2` floats) to `out_ptr`.
///
/// # Safety
/// `out_ptr` must be valid, 4-byte aligned, and writable for `frames * 2` contiguous `f32` elements.
#[no_mangle]
pub unsafe extern "C" fn sh_render_stereo(ptr: *mut WasmStudio, out_ptr: *mut f32, frames: usize) {
    if out_ptr.is_null() || frames == 0 {
        return;
    }
    if !(out_ptr as usize).is_multiple_of(core::mem::align_of::<f32>()) {
        return;
    }
    let Some(sample_count) = frames.checked_mul(2) else {
        return;
    };
    let Some(byte_len) = sample_count.checked_mul(core::mem::size_of::<f32>()) else {
        return;
    };
    if byte_len > isize::MAX as usize {
        return;
    }
    if let Some(studio) = ptr.as_mut() {
        let out = slice::from_raw_parts_mut(out_ptr, sample_count);
        studio.render_stereo_internal(out);
    }
}

/// Renders `frames` planar stereo f32 samples directly into `left_ptr` and `right_ptr`
/// (ideal for Web Audio `AudioWorklet` outputs without de-interleaving).
///
/// # Safety
/// `left_ptr` and `right_ptr` must each be valid, 4-byte aligned, non-overlapping, and writable
/// for `frames` contiguous `f32` elements.
#[no_mangle]
pub unsafe extern "C" fn sh_render_planar(
    ptr: *mut WasmStudio,
    left_ptr: *mut f32,
    right_ptr: *mut f32,
    frames: usize,
) {
    if left_ptr.is_null() || right_ptr.is_null() || frames == 0 {
        return;
    }
    let align = core::mem::align_of::<f32>();
    let l_start = left_ptr as usize;
    let r_start = right_ptr as usize;
    if !l_start.is_multiple_of(align) || !r_start.is_multiple_of(align) {
        return;
    }
    let Some(byte_len) = frames.checked_mul(core::mem::size_of::<f32>()) else {
        return;
    };
    if byte_len > isize::MAX as usize {
        return;
    }
    let (Some(l_end), Some(r_end)) = (l_start.checked_add(byte_len), r_start.checked_add(byte_len))
    else {
        return;
    };
    if !(l_end <= r_start || r_end <= l_start) {
        return;
    }
    if let Some(studio) = ptr.as_mut() {
        let left = slice::from_raw_parts_mut(left_ptr, frames);
        let right = slice::from_raw_parts_mut(right_ptr, frames);
        studio.render_planar_internal(left, right);
    }
}

/// Returns the number of currently active sounding voices.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_active_voices(ptr: *const WasmStudio) -> u32 {
    ptr.as_ref()
        .map(|s| s.engine.active_voice_count() as u32)
        .unwrap_or(0)
}

/// Returns the total number of built-in melodic General MIDI presets (128).
#[no_mangle]
pub extern "C" fn sh_preset_count() -> u32 {
    all_presets().len() as u32
}

/// Returns the total number of built-in GM drum kit voices (47).
#[no_mangle]
pub extern "C" fn sh_drum_count() -> u32 {
    all_drums().len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_lifecycle_and_safety_guards() {
        assert_eq!(sh_preset_count(), 128);
        assert_eq!(sh_drum_count(), 47);

        let mem = sh_alloc(64);
        assert!(!mem.is_null());
        unsafe { sh_free(mem, 64) };
        unsafe { sh_free(core::ptr::null_mut(), 64) };

        let engine = sh_engine_new(44100.0, 16);
        assert!(!engine.is_null());
        unsafe {
            sh_program_change(engine, 0, 81);
            assert_eq!(sh_channel_program(engine, 0), 81);
            sh_note_on(engine, 0, 60, 100);
            assert_eq!(sh_active_voices(engine), 1);
            assert_ne!(sh_active_notes_low(engine) & (1u64 << 60), 0);
            assert!(sh_channel_activity(engine, 0) > 0.5);

            let mut stereo = [0.0_f32; 256];
            sh_render_stereo(engine, stereo.as_mut_ptr(), 128);
            assert!(stereo.iter().any(|&s| s.abs() > 1e-4));

            let mut left = [0.0_f32; 128];
            let mut right = [0.0_f32; 128];
            sh_render_planar(engine, left.as_mut_ptr(), right.as_mut_ptr(), 128);
            assert!(left.iter().any(|&s| s.abs() > 1e-4));

            // Null pointer, aliasing, unaligned, and overflow calls must safely no-op
            sh_render_stereo(engine, core::ptr::null_mut(), 128);
            sh_render_stereo(engine, stereo.as_mut_ptr(), usize::MAX);
            let unaligned = (stereo.as_mut_ptr() as *mut u8).add(1) as *mut f32;
            sh_render_stereo(engine, unaligned, 16);

            let mut overlap = [1.23_f32; 8];
            sh_render_planar(engine, overlap.as_mut_ptr(), overlap.as_mut_ptr(), 4);
            sh_render_planar(engine, overlap.as_mut_ptr(), overlap.as_mut_ptr().add(2), 4);
            assert_eq!(overlap, [1.23_f32; 8]);

            sh_render_planar(engine, left.as_mut_ptr(), right.as_mut_ptr(), usize::MAX);
            sh_note_off(engine, 0, 60);
            assert_eq!(sh_active_notes_low(engine) & (1u64 << 60), 0);
            sh_all_notes_off(engine);
            sh_reset(engine);
            assert_eq!(sh_active_voices(engine), 0);
            sh_engine_free(engine);
            sh_engine_free(core::ptr::null_mut());
        }
    }

    #[test]
    fn ffi_sequencer_score_and_wav_export() {
        let studio = sh_engine_new(22050.0, 32);
        unsafe {
            let dur = sh_load_demo(studio, 0);
            assert!(dur > 5.0);
            assert!((sh_seq_duration_sec(studio) - dur).abs() < 1e-3);

            sh_seq_play(studio);
            assert_eq!(sh_seq_is_playing(studio), 1);

            let mut left = [0.0_f32; 512];
            let mut right = [0.0_f32; 512];
            sh_render_planar(studio, left.as_mut_ptr(), right.as_mut_ptr(), 512);
            assert!(left.iter().any(|&s| s.abs() > 1e-4));
            assert!(sh_seq_position_sec(studio) > 0.01);

            // Load custom .shs score and render WAV
            let score = b"tempo 120\ntrack p ch=0 program=0\no4 l4 c e g\n";
            let sdur = sh_load_score(studio, score.as_ptr(), score.len());
            assert!(sdur > 1.0);
            let wav_len = sh_render_song_wav(studio, 1);
            assert!(wav_len > 44);
            let wav_bytes = slice::from_raw_parts(sh_wav_ptr(studio), sh_wav_len(studio));
            assert_eq!(&wav_bytes[0..4], b"RIFF");
            assert_eq!(&wav_bytes[8..12], b"WAVE");

            // Invalid score reports error
            let bad = b"tempo NaN\nc4\n";
            assert!(sh_load_score(studio, bad.as_ptr(), bad.len()) < 0.0);
            assert!(sh_last_error_len(studio) > 0);

            sh_engine_free(studio);
        }
    }
}
