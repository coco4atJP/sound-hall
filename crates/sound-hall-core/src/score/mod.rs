pub mod demos;

use std::fmt;

use crate::midi::event::{MidiEvent, MidiSong, TimedEvent};
use crate::presets::find_preset;

/// Errors raised while parsing a `.shs` (SoundHall Score) text file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoreError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ScoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "score parse error at line {}: {}",
            self.line, self.message
        )
    }
}

impl std::error::Error for ScoreError {}

impl ScoreError {
    fn new(line: usize, msg: impl Into<String>) -> Self {
        Self {
            line,
            message: msg.into(),
        }
    }
}

struct TrackState {
    _name: String,
    channel: u8,
    octave: i32,
    default_beats: f64,
    velocity: u8,
    gate: f64,
    beat_pos: f64,
}

impl TrackState {
    fn new(name: String, channel: u8) -> Self {
        Self {
            _name: name,
            channel,
            octave: 4,
            default_beats: 1.0, // quarter note
            velocity: 96,
            gate: 7.0 / 8.0,
            beat_pos: 0.0,
        }
    }
}

const MAX_REPEAT_DEPTH: usize = 16;
const MAX_EXPANDED_BYTES: usize = 1_048_576;

fn expand_repeats(input: &str) -> Result<String, ScoreError> {
    if input.len() > MAX_EXPANDED_BYTES {
        return Err(ScoreError::new(1, "score input exceeds 1 MiB limit"));
    }
    expand_repeats_inner(input, 0)
}

fn expand_repeats_inner(input: &str, depth: usize) -> Result<String, ScoreError> {
    if depth > MAX_REPEAT_DEPTH {
        return Err(ScoreError::new(
            1,
            format!("repeat group nesting exceeds maximum depth ({MAX_REPEAT_DEPTH})"),
        ));
    }
    let mut out = String::with_capacity(input.len().min(65536));
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '#' && (i == 0 || chars[i - 1].is_whitespace()) {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            if i < chars.len() {
                if out.len() + 1 > MAX_EXPANDED_BYTES {
                    return Err(ScoreError::new(1, "expanded score exceeds size limit"));
                }
                out.push('\n');
                i += 1;
            }
            continue;
        }
        if chars[i] == '(' {
            let mut paren_depth = 1usize;
            let start = i + 1;
            i += 1;
            while i < chars.len() && paren_depth > 0 {
                if chars[i] == '#' && (i == 0 || chars[i - 1].is_whitespace()) {
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                    }
                    continue;
                }
                if chars[i] == '(' {
                    paren_depth += 1;
                } else if chars[i] == ')' {
                    paren_depth -= 1;
                }
                i += 1;
            }
            if paren_depth != 0 {
                return Err(ScoreError::new(1, "unclosed '(' repeat group"));
            }
            let inner_raw: String = chars[start..i - 1].iter().collect();
            let inner = expand_repeats_inner(&inner_raw, depth + 1)?;
            let mut count = 1usize;
            if i < chars.len() && chars[i] == '*' {
                i += 1;
                let num_start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if i > num_start {
                    let s: String = chars[num_start..i].iter().collect();
                    count = s.parse::<usize>().unwrap_or(1).clamp(1, 64);
                }
            }
            let per_iter = inner.len() + 2;
            if per_iter.saturating_mul(count).saturating_add(out.len()) > MAX_EXPANDED_BYTES {
                return Err(ScoreError::new(
                    1,
                    "expanded repeat group exceeds 1 MiB safety limit",
                ));
            }
            for _ in 0..count {
                out.push(' ');
                out.push_str(&inner);
                out.push(' ');
            }
        } else {
            if out.len() + chars[i].len_utf8() > MAX_EXPANDED_BYTES {
                return Err(ScoreError::new(1, "expanded score exceeds size limit"));
            }
            out.push(chars[i]);
            i += 1;
        }
    }
    Ok(out)
}

fn tokenize_line(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_whitespace() || ch == '|' {
            i += 1;
            continue;
        }
        if ch == '>' || ch == '<' {
            tokens.push(ch.to_string());
            i += 1;
            continue;
        }
        if ch == '[' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != ']' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            // Capture trailing length/dots like [c e g]2.
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
            continue;
        }
        let start = i;
        while i < chars.len()
            && !chars[i].is_whitespace()
            && chars[i] != '|'
            && chars[i] != '['
            && chars[i] != '>'
            && chars[i] != '<'
        {
            i += 1;
        }
        tokens.push(chars[start..i].iter().collect());
    }
    tokens
}

fn parse_length_suffix(suffix: &str, default_beats: f64) -> Result<f64, String> {
    let mut dots = 0usize;
    let trimmed = suffix.trim_end_matches(|c| {
        if c == '.' {
            dots += 1;
            true
        } else {
            false
        }
    });

    let base = if trimmed.is_empty() {
        default_beats
    } else {
        let div: f64 = trimmed
            .parse::<f64>()
            .map_err(|_| format!("invalid note length '{suffix}'"))?;
        if !div.is_finite() || div <= 0.0 || div > 128.0 {
            return Err(format!("note length out of range: '{suffix}'"));
        }
        4.0 / div
    };

    let mut total = base;
    let mut add = base * 0.5;
    for _ in 0..dots {
        total += add;
        add *= 0.5;
    }
    Ok(total)
}

fn parse_pitch(token: &str, octave: i32) -> Result<(u8, &str), String> {
    let bytes = token.as_bytes();
    if bytes.is_empty() {
        return Err("empty pitch".to_string());
    }
    if bytes[0] == b'n' || bytes[0] == b'N' {
        let rest = &token[1..];
        let split_at = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let num_str = &rest[..split_at];
        let note: u8 = num_str
            .parse()
            .map_err(|_| format!("invalid numeric MIDI note '{token}'"))?;
        let mut suffix = &rest[split_at..];
        if let Some(stripped) = suffix.strip_prefix(':') {
            suffix = stripped;
        }
        return Ok((note.min(127), suffix));
    }

    if bytes[0].is_ascii_digit() {
        let split_at = token
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(token.len());
        let note: u8 = token[..split_at]
            .parse()
            .map_err(|_| format!("invalid MIDI number '{token}'"))?;
        let mut suffix = &token[split_at..];
        if let Some(stripped) = suffix.strip_prefix(':') {
            suffix = stripped;
        }
        return Ok((note.min(127), suffix));
    }

    let base_semi: i32 = match bytes[0].to_ascii_lowercase() {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        b'b' => 11,
        _ => return Err(format!("unknown note token '{token}'")),
    };

    let mut idx = 1usize;
    let mut accidental = 0i32;
    while idx < bytes.len() {
        match bytes[idx] {
            b'#' | b'+' => {
                accidental += 1;
                idx += 1;
            }
            b'b' | b'-' => {
                accidental -= 1;
                idx += 1;
            }
            _ => break,
        }
    }

    let midi = ((octave + 1) * 12 + base_semi + accidental).clamp(0, 127) as u8;
    Ok((midi, &token[idx..]))
}

fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b == b'#' && (idx == 0 || bytes[idx - 1].is_ascii_whitespace()) {
            return &line[..idx];
        }
    }
    line
}

/// Parses a SoundHall Score (`.shs`) document into a `MidiSong`.
pub fn parse_score(source: &str) -> Result<MidiSong, ScoreError> {
    let expanded = expand_repeats(source)?;
    let mut tempo_bpm: f64 = 120.0;
    let mut events: Vec<TimedEvent> = Vec::new();
    let mut track_names: Vec<String> = Vec::new();
    let mut current_track: Option<TrackState> = None;

    for (line_idx, raw_line) in expanded.lines().enumerate() {
        let line_no = line_idx + 1;
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        let bytes = line.as_bytes();
        if bytes.len() >= 6
            && bytes[..5].eq_ignore_ascii_case(b"tempo")
            && matches!(bytes[5], b' ' | b'=' | b'\t')
        {
            let val_str = line[5..].trim().trim_start_matches('=').trim();
            let bpm: f64 = val_str
                .parse()
                .map_err(|_| ScoreError::new(line_no, format!("invalid tempo '{val_str}'")))?;
            if !bpm.is_finite() {
                return Err(ScoreError::new(
                    line_no,
                    format!("non-finite tempo '{val_str}'"),
                ));
            }
            tempo_bpm = bpm.clamp(20.0, 400.0);
            continue;
        }

        if bytes.len() >= 6
            && bytes[..5].eq_ignore_ascii_case(b"track")
            && matches!(bytes[5], b' ' | b'\t')
        {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let raw_name = parts.get(1).copied().unwrap_or("Track");
            let mut channel: u8 = (track_names.len() as u8) % 16;
            let mut program: Option<u8> = None;
            let mut bank: Option<u8> = None;
            let mut vol: Option<u8> = None;
            let mut pan: Option<u8> = None;
            let mut rev: Option<u8> = None;
            let mut cho: Option<u8> = None;

            for kv in parts.iter().skip(1) {
                if let Some((k, v)) = kv.split_once('=') {
                    let k_low = k.to_ascii_lowercase();
                    match k_low.as_str() {
                        "ch" | "channel" => {
                            channel = v.parse::<u8>().map_err(|_| {
                                ScoreError::new(line_no, format!("invalid channel '{v}'"))
                            })? & 0x0F;
                        }
                        "bank" => {
                            bank = Some(
                                v.parse::<u8>().map_err(|_| {
                                    ScoreError::new(line_no, format!("invalid bank '{v}'"))
                                })? & 0x7F,
                            );
                        }
                        "prog" | "program" => {
                            if let Ok(p) = v.parse::<u8>() {
                                program = Some(p & 0x7F);
                            } else if let Some(preset) = find_preset(v) {
                                program = Some(preset.program);
                                bank.get_or_insert(preset.bank);
                            } else {
                                return Err(ScoreError::new(
                                    line_no,
                                    format!("unknown program '{v}'"),
                                ));
                            }
                        }
                        "vol" | "volume" => {
                            vol = Some(v.parse::<u8>().unwrap_or(100).min(127));
                        }
                        "pan" => {
                            pan = Some(v.parse::<u8>().unwrap_or(64).min(127));
                        }
                        "rev" | "reverb" => {
                            rev = Some(v.parse::<u8>().unwrap_or(36).min(127));
                        }
                        "cho" | "chorus" => {
                            cho = Some(v.parse::<u8>().unwrap_or(0).min(127));
                        }
                        _ => {}
                    }
                }
            }

            let name = raw_name.trim_matches('"').to_string();
            track_names.push(name.clone());
            if let Some(p) = program {
                if channel != 9 {
                    // Bank Select MSB first (0 = General MIDI, 1 = 和楽器)
                    events.push(TimedEvent {
                        time_sec: 0.0,
                        event: MidiEvent::ControlChange {
                            channel,
                            cc: 0,
                            value: bank.unwrap_or(0),
                        },
                    });
                    events.push(TimedEvent {
                        time_sec: 0.0,
                        event: MidiEvent::ProgramChange {
                            channel,
                            program: p,
                        },
                    });
                }
            }
            if let Some(v) = vol {
                events.push(TimedEvent {
                    time_sec: 0.0,
                    event: MidiEvent::ControlChange {
                        channel,
                        cc: 7,
                        value: v,
                    },
                });
            }
            if let Some(p) = pan {
                events.push(TimedEvent {
                    time_sec: 0.0,
                    event: MidiEvent::ControlChange {
                        channel,
                        cc: 10,
                        value: p,
                    },
                });
            }
            if let Some(r) = rev {
                events.push(TimedEvent {
                    time_sec: 0.0,
                    event: MidiEvent::ControlChange {
                        channel,
                        cc: 91,
                        value: r,
                    },
                });
            }
            if let Some(c) = cho {
                events.push(TimedEvent {
                    time_sec: 0.0,
                    event: MidiEvent::ControlChange {
                        channel,
                        cc: 93,
                        value: c,
                    },
                });
            }
            current_track = Some(TrackState::new(name, channel));
            continue;
        }

        let tr = match current_track.as_mut() {
            Some(t) => t,
            None => {
                track_names.push("Track 1".to_string());
                current_track = Some(TrackState::new("Track 1".to_string(), 0));
                current_track.as_mut().unwrap()
            }
        };

        let sec_per_beat = 60.0 / tempo_bpm;
        for tok in tokenize_line(line) {
            if tok == ">" {
                tr.octave = (tr.octave + 1).min(8);
                continue;
            }
            if tok == "<" {
                tr.octave = (tr.octave - 1).max(0);
                continue;
            }
            if tok.starts_with('[') {
                let close_idx = tok
                    .find(']')
                    .ok_or_else(|| ScoreError::new(line_no, format!("unclosed chord '{tok}'")))?;
                let inner = &tok[1..close_idx];
                let len_suffix = &tok[close_idx + 1..];
                let beats = parse_length_suffix(len_suffix, tr.default_beats)
                    .map_err(|e| ScoreError::new(line_no, e))?;
                let on_sec = tr.beat_pos * sec_per_beat;
                let off_sec = (tr.beat_pos + beats * tr.gate) * sec_per_beat;

                for part in inner.split(|c: char| c.is_whitespace() || c == ',') {
                    let mut p = part.trim();
                    if p.is_empty() {
                        continue;
                    }
                    while let Some(rest) = p.strip_prefix('>') {
                        tr.octave = (tr.octave + 1).min(8);
                        p = rest;
                    }
                    while let Some(rest) = p.strip_prefix('<') {
                        tr.octave = (tr.octave - 1).max(0);
                        p = rest;
                    }
                    let mut post_oct = 0i32;
                    while let Some(rest) = p.strip_suffix('<') {
                        post_oct -= 1;
                        p = rest;
                    }
                    while let Some(rest) = p.strip_suffix('>') {
                        post_oct += 1;
                        p = rest;
                    }
                    if p.is_empty() {
                        tr.octave = (tr.octave + post_oct).clamp(0, 8);
                        continue;
                    }
                    let (note, _) =
                        parse_pitch(p, tr.octave).map_err(|e| ScoreError::new(line_no, e))?;
                    tr.octave = (tr.octave + post_oct).clamp(0, 8);
                    events.push(TimedEvent {
                        time_sec: on_sec,
                        event: MidiEvent::NoteOn {
                            channel: tr.channel,
                            note,
                            velocity: tr.velocity,
                        },
                    });
                    if tr.channel != 9 {
                        events.push(TimedEvent {
                            time_sec: off_sec,
                            event: MidiEvent::NoteOff {
                                channel: tr.channel,
                                note,
                            },
                        });
                    }
                }
                tr.beat_pos += beats;
                continue;
            }

            let t_low = tok.to_ascii_lowercase();
            if let Some(rest) = t_low.strip_prefix('o') {
                if let Ok(oct) = rest.parse::<i32>() {
                    tr.octave = oct.clamp(0, 8);
                    continue;
                }
            }
            if let Some(rest) = t_low.strip_prefix('l') {
                tr.default_beats = parse_length_suffix(rest, tr.default_beats)
                    .map_err(|e| ScoreError::new(line_no, e))?;
                continue;
            }
            if let Some(rest) = t_low.strip_prefix('v') {
                if let Ok(v) = rest.parse::<u8>() {
                    tr.velocity = v.min(127);
                    continue;
                }
            }
            if let Some(rest) = t_low.strip_prefix('q') {
                if let Ok(q) = rest.parse::<u8>() {
                    tr.gate = (q.clamp(1, 8) as f64) / 8.0;
                    continue;
                }
            }
            if let Some(rest) = t_low.strip_prefix('r') {
                let beats = parse_length_suffix(rest, tr.default_beats)
                    .map_err(|e| ScoreError::new(line_no, e))?;
                tr.beat_pos += beats;
                continue;
            }

            let (note, suffix) =
                parse_pitch(&tok, tr.octave).map_err(|e| ScoreError::new(line_no, e))?;
            let beats = parse_length_suffix(suffix, tr.default_beats)
                .map_err(|e| ScoreError::new(line_no, e))?;
            let on_sec = tr.beat_pos * sec_per_beat;
            let off_sec = (tr.beat_pos + beats * tr.gate) * sec_per_beat;

            events.push(TimedEvent {
                time_sec: on_sec,
                event: MidiEvent::NoteOn {
                    channel: tr.channel,
                    note,
                    velocity: tr.velocity,
                },
            });
            if tr.channel != 9 {
                events.push(TimedEvent {
                    time_sec: off_sec,
                    event: MidiEvent::NoteOff {
                        channel: tr.channel,
                        note,
                    },
                });
            }
            tr.beat_pos += beats;
        }
    }

    Ok(MidiSong::new(events, track_names))
}
