use std::fmt;

use crate::synth::engine::SoundHallEngine;

use super::event::{MidiEvent, MidiSong, TimedEvent};

/// Errors produced when parsing a Standard MIDI File (`.mid`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SmfError {
    UnexpectedEof,
    InvalidHeader,
    UnsupportedDivision,
    InvalidTrackHeader,
    InvalidVarLen,
}

impl fmt::Display for SmfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SmfError::UnexpectedEof => write!(f, "unexpected end of MIDI file"),
            SmfError::InvalidHeader => write!(f, "invalid MThd header"),
            SmfError::UnsupportedDivision => write!(f, "unsupported MIDI time division"),
            SmfError::InvalidTrackHeader => write!(f, "invalid MTrk track chunk"),
            SmfError::InvalidVarLen => write!(f, "invalid VLQ delta time"),
        }
    }
}

impl std::error::Error for SmfError {}

#[derive(Clone, Debug)]
struct RawTickEvent {
    tick: u64,
    order: usize,
    kind: RawKind,
}

#[derive(Clone, Debug)]
enum RawKind {
    Tempo(u32),
    Midi(MidiEvent),
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_u8(&mut self) -> Result<u8, SmfError> {
        if self.pos >= self.data.len() {
            return Err(SmfError::UnexpectedEof);
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_u16_be(&mut self) -> Result<u16, SmfError> {
        let a = self.read_u8()? as u16;
        let b = self.read_u8()? as u16;
        Ok((a << 8) | b)
    }

    fn read_u32_be(&mut self) -> Result<u32, SmfError> {
        let a = self.read_u8()? as u32;
        let b = self.read_u8()? as u32;
        let c = self.read_u8()? as u32;
        let d = self.read_u8()? as u32;
        Ok((a << 24) | (b << 16) | (c << 8) | d)
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], SmfError> {
        if n > self.remaining() {
            return Err(SmfError::UnexpectedEof);
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_vlq(&mut self) -> Result<u32, SmfError> {
        let mut val: u32 = 0;
        for _ in 0..4 {
            let b = self.read_u8()?;
            val = (val << 7) | ((b & 0x7F) as u32);
            if b & 0x80 == 0 {
                return Ok(val);
            }
        }
        Err(SmfError::InvalidVarLen)
    }
}

/// Parses a Standard MIDI File (Type 0 or Type 1) from raw bytes into a `MidiSong`.
pub fn parse_smf(bytes: &[u8]) -> Result<MidiSong, SmfError> {
    let mut r = Reader::new(bytes);
    if r.read_bytes(4)? != b"MThd" {
        return Err(SmfError::InvalidHeader);
    }
    let header_len = r.read_u32_be()? as usize;
    if header_len < 6 {
        return Err(SmfError::InvalidHeader);
    }
    let _format = r.read_u16_be()?;
    let num_tracks = r.read_u16_be()? as usize;
    let division = r.read_u16_be()?;
    if header_len > 6 {
        r.read_bytes(header_len - 6)?;
    }

    // Division: positive means ticks per quarter note; negative is SMPTE FPS * ticks/frame
    let (tpqn, smpte_ticks_per_sec) = if (division & 0x8000) == 0 {
        if division == 0 {
            return Err(SmfError::UnsupportedDivision);
        }
        (division as f64, None)
    } else {
        let fps_code = -(((division >> 8) as i8) as i32);
        let fps = match fps_code {
            24 => 24.0,
            25 => 25.0,
            29 => 29.97,
            30 => 30.0,
            _ => return Err(SmfError::UnsupportedDivision),
        };
        let tpf = (division & 0xFF) as f64;
        if tpf <= 0.0 {
            return Err(SmfError::UnsupportedDivision);
        }
        (480.0, Some(fps * tpf))
    };

    let mut raw_events: Vec<RawTickEvent> = Vec::new();
    let mut track_names: Vec<String> = Vec::new();
    let mut order_counter: usize = 0;
    let mut tracks_parsed: usize = 0;

    while tracks_parsed < num_tracks && r.remaining() >= 8 {
        let chunk_id = r.read_bytes(4)?;
        let chunk_len = r.read_u32_be()? as usize;
        let chunk_slice = r.read_bytes(chunk_len)?;
        if chunk_id != b"MTrk" {
            // Standard MIDI files allow non-MTrk chunks to be skipped cleanly
            continue;
        }
        let track_idx = tracks_parsed;
        tracks_parsed += 1;
        let mut tr = Reader::new(chunk_slice);

        let mut tick: u64 = 0;
        let mut running_status: u8 = 0;
        let mut found_name: Option<String> = None;

        while tr.remaining() > 0 {
            let delta = tr.read_vlq()? as u64;
            tick = tick.saturating_add(delta);

            let peek = tr.read_u8()?;
            let status = if (peek & 0x80) != 0 {
                if peek < 0xF0 {
                    running_status = peek;
                } else if peek == 0xF0 || peek == 0xF7 {
                    running_status = 0;
                }
                peek
            } else {
                if running_status == 0 {
                    return Err(SmfError::InvalidHeader);
                }
                // Rewind 1 byte so data1 reads `peek`
                tr.pos -= 1;
                running_status
            };

            if status == 0xFF {
                let meta_type = tr.read_u8()?;
                let len = tr.read_vlq()? as usize;
                let data = tr.read_bytes(len)?;
                match meta_type {
                    0x51 if data.len() == 3 => {
                        let us_per_qn =
                            ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32);
                        raw_events.push(RawTickEvent {
                            tick,
                            order: order_counter,
                            kind: RawKind::Tempo(us_per_qn.max(1000)),
                        });
                        order_counter += 1;
                    }
                    0x03 if found_name.is_none() => {
                        found_name = Some(String::from_utf8_lossy(data).trim().to_string());
                    }
                    0x2F => break,
                    _ => {}
                }
            } else if status == 0xF0 || status == 0xF7 {
                let len = tr.read_vlq()? as usize;
                let _ = tr.read_bytes(len)?;
            } else {
                let msg_type = status & 0xF0;
                let ch = status & 0x0F;
                match msg_type {
                    0x80 => {
                        let note = tr.read_u8()? & 0x7F;
                        let _vel = tr.read_u8()?;
                        raw_events.push(RawTickEvent {
                            tick,
                            order: order_counter,
                            kind: RawKind::Midi(MidiEvent::NoteOff { channel: ch, note }),
                        });
                        order_counter += 1;
                    }
                    0x90 => {
                        let note = tr.read_u8()? & 0x7F;
                        let velocity = tr.read_u8()? & 0x7F;
                        let ev = if velocity == 0 {
                            MidiEvent::NoteOff { channel: ch, note }
                        } else {
                            MidiEvent::NoteOn {
                                channel: ch,
                                note,
                                velocity,
                            }
                        };
                        raw_events.push(RawTickEvent {
                            tick,
                            order: order_counter,
                            kind: RawKind::Midi(ev),
                        });
                        order_counter += 1;
                    }
                    0xA0 => {
                        let _ = tr.read_bytes(2)?;
                    }
                    0xB0 => {
                        let cc = tr.read_u8()? & 0x7F;
                        let value = tr.read_u8()? & 0x7F;
                        raw_events.push(RawTickEvent {
                            tick,
                            order: order_counter,
                            kind: RawKind::Midi(MidiEvent::ControlChange {
                                channel: ch,
                                cc,
                                value,
                            }),
                        });
                        order_counter += 1;
                    }
                    0xC0 => {
                        let program = tr.read_u8()? & 0x7F;
                        raw_events.push(RawTickEvent {
                            tick,
                            order: order_counter,
                            kind: RawKind::Midi(MidiEvent::ProgramChange {
                                channel: ch,
                                program,
                            }),
                        });
                        order_counter += 1;
                    }
                    0xD0 => {
                        let _ = tr.read_u8()?;
                    }
                    0xE0 => {
                        let lsb = (tr.read_u8()? & 0x7F) as u16;
                        let msb = (tr.read_u8()? & 0x7F) as u16;
                        let value_14bit = (msb << 7) | lsb;
                        raw_events.push(RawTickEvent {
                            tick,
                            order: order_counter,
                            kind: RawKind::Midi(MidiEvent::PitchBend {
                                channel: ch,
                                value_14bit,
                            }),
                        });
                        order_counter += 1;
                    }
                    _ => {}
                }
            }
        }
        track_names.push(found_name.unwrap_or_else(|| format!("Track {}", track_idx + 1)));
    }

    if num_tracks > 0 && tracks_parsed == 0 {
        return Err(SmfError::InvalidTrackHeader);
    }

    // Sort globally by tick, keeping tempo changes before note events at the same tick
    raw_events.sort_by(|a, b| {
        a.tick.cmp(&b.tick).then_with(|| {
            let ap = matches!(a.kind, RawKind::Tempo(_));
            let bp = matches!(b.kind, RawKind::Tempo(_));
            bp.cmp(&ap).then_with(|| a.order.cmp(&b.order))
        })
    });

    // Convert ticks to exact seconds using global tempo map (default 120 BPM = 500_000 us/qn)
    let mut current_tick: u64 = 0;
    let mut current_time: f64 = 0.0;
    let mut us_per_qn: f64 = 500_000.0;
    let mut timed = Vec::with_capacity(raw_events.len());

    for raw in raw_events {
        let delta_ticks = raw.tick.saturating_sub(current_tick) as f64;
        if delta_ticks > 0.0 {
            if let Some(tps) = smpte_ticks_per_sec {
                current_time += delta_ticks / tps;
            } else {
                current_time += delta_ticks * (us_per_qn / 1_000_000.0) / tpqn;
            }
            current_tick = raw.tick;
        }
        match raw.kind {
            RawKind::Tempo(us) => us_per_qn = us as f64,
            RawKind::Midi(event) => timed.push(TimedEvent {
                time_sec: current_time,
                event,
            }),
        }
    }

    Ok(MidiSong::new(timed, track_names))
}

/// Renders a `MidiSong` into stereo interleaved f32 audio using `SoundHallEngine`.
pub struct MidiSequencer;

impl MidiSequencer {
    /// Maximum audio duration (30 minutes) rendered by default to bound memory allocation.
    pub const MAX_RENDER_SECONDS: f64 = 1800.0;

    pub fn render_to_buffer(
        song: &MidiSong,
        engine: &mut SoundHallEngine,
        tail_seconds: f64,
    ) -> Vec<f32> {
        Self::render_to_buffer_with_max_secs(song, engine, tail_seconds, Self::MAX_RENDER_SECONDS)
    }

    pub fn render_to_buffer_with_max_secs(
        song: &MidiSong,
        engine: &mut SoundHallEngine,
        tail_seconds: f64,
        max_secs: f64,
    ) -> Vec<f32> {
        let sr = engine.sample_rate() as f64;
        let dur = if song.duration_secs.is_finite() && song.duration_secs >= 0.0 {
            song.duration_secs
        } else {
            0.0
        };
        let tail = if tail_seconds.is_finite() {
            tail_seconds.clamp(0.1, 60.0)
        } else {
            0.5
        };
        let cap = if max_secs.is_finite() && max_secs > 0.0 {
            max_secs.min(Self::MAX_RENDER_SECONDS)
        } else {
            Self::MAX_RENDER_SECONDS
        };
        let total_secs = (dur + tail).min(cap);
        let total_frames = ((total_secs * sr).ceil() as usize).max(1);
        let mut out = vec![0.0_f32; total_frames * 2];

        let mut event_idx = 0;
        let total_events = song.events.len();
        let mut cursor_frame: usize = 0;

        while cursor_frame < total_frames {
            while event_idx < total_events {
                let ev_sec = song.events[event_idx].time_sec;
                if !ev_sec.is_finite() || ev_sec < 0.0 {
                    event_idx += 1;
                    continue;
                }
                let ev_frame = (ev_sec * sr).round();
                if ev_frame <= cursor_frame as f64 {
                    engine.handle_event(&song.events[event_idx].event);
                    event_idx += 1;
                } else {
                    break;
                }
            }

            let next_ev_frame = if event_idx < total_events {
                ((song.events[event_idx].time_sec * sr)
                    .round()
                    .clamp(0.0, total_frames as f64)) as usize
            } else {
                total_frames
            };

            let chunk_frames = next_ev_frame
                .saturating_sub(cursor_frame)
                .min(total_frames - cursor_frame)
                .clamp(1, 256);
            let start = cursor_frame * 2;
            let end = (cursor_frame + chunk_frames) * 2;
            engine.render_stereo(&mut out[start..end]);
            cursor_frame += chunk_frames;
        }

        out
    }
}
