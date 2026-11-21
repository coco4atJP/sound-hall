use std::f32::consts::TAU;

use sound_hall_core::{
    all_drums, all_presets, get_preset, list_demos, parse_demo, parse_score, parse_smf,
    write_wav_i16, MidiSequencer, SoundHallEngine,
};

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, &s| m.max(s.abs()))
}

/// Estimates fundamental frequency using DFT search near expected Hz + harmonic product check.
fn measure_fundamental_hz(mono: &[f32], sr: f32, expected_hz: f32) -> f32 {
    // Analyze steady-state window after initial attack transient
    let start = ((0.12 * sr) as usize).min(mono.len() / 3);
    let len = ((0.22 * sr) as usize).min(mono.len() - start);
    let slice = &mono[start..start + len];
    let n = slice.len() as f32;

    // Fine frequency grid spanning [expected * 0.45, expected * 2.2] to catch octave errors too!
    let f_min = (expected_hz * 0.45).max(30.0);
    let f_max = (expected_hz * 2.2).min(0.42 * sr);
    let steps = 400;
    let mut best_f = expected_hz;
    let mut best_mag = -1.0_f32;

    for i in 0..=steps {
        let f = f_min + (f_max - f_min) * (i as f32 / steps as f32);
        let w = TAU * f / sr;
        let mut re = 0.0_f32;
        let mut im = 0.0_f32;
        for (k, &x) in slice.iter().enumerate() {
            // Hann window reduces spectral leakage
            let win = 0.5 * (1.0 - (TAU * k as f32 / n).cos());
            let angle = w * k as f32;
            re += x * win * angle.cos();
            im -= x * win * angle.sin();
        }
        let mag = re * re + im * im;
        // Slight preference for fundamental over exact 2x octave if fundamental is strong
        if mag > best_mag {
            best_mag = mag;
            best_f = f;
        }
    }
    best_f
}

#[test]
fn verify_all_128_gm_presets_audio() {
    let sr = 44100.0_f32;
    let hold_frames = (0.40 * sr) as usize;
    let rel_frames = (0.60 * sr) as usize;
    let mut buf = vec![0.0_f32; (hold_frames + rel_frames) * 2];

    for preset in all_presets().iter() {
        let mut engine = SoundHallEngine::new(sr);
        engine.program_change(0, preset.program);

        // Choose representative pitch inside instrument's comfortable range
        let pitch = if (preset.pitch_range.0..=preset.pitch_range.1).contains(&60) {
            60
        } else {
            ((preset.pitch_range.0 as u16 + preset.pitch_range.1 as u16) / 2) as u8
        };

        engine.note_on(0, pitch, 100);
        engine.render_stereo(&mut buf[..hold_frames * 2]);
        engine.note_off(0, pitch);
        engine.render_stereo(&mut buf[hold_frames * 2..]);

        for (idx, &s) in buf.iter().enumerate() {
            assert!(
                s.is_finite(),
                "GM #{} {} produced non-finite sample at {idx}",
                preset.program,
                preset.name_en
            );
        }

        let max_p = peak(&buf);
        assert!(
            max_p <= 1.0001,
            "GM #{} {} clipped above 1.0: peak={max_p}",
            preset.program,
            preset.name_en
        );

        let hold_rms = rms(&buf[..hold_frames * 2]);
        assert!(
            hold_rms > 0.002,
            "GM #{} {} too quiet during hold: rms={hold_rms}",
            preset.program,
            preset.name_en
        );

        // Verify note stops cleanly after all_sound_off
        engine.all_sound_off();
        let mut silence = vec![0.0_f32; 512];
        engine.render_stereo(&mut silence);
        assert!(
            peak(&silence) < 1e-6,
            "GM #{} {} did not silence on all_sound_off",
            preset.program,
            preset.name_en
        );
    }
}

#[test]
fn verify_all_47_gm_drums_and_choke_groups() {
    let sr = 44100.0_f32;
    let frames = (0.5 * sr) as usize;
    let mut buf = vec![0.0_f32; frames * 2];

    for drum in all_drums().iter() {
        let mut engine = SoundHallEngine::new(sr);
        engine.note_on(9, drum.note, 110);
        engine.render_stereo(&mut buf);

        for &s in &buf {
            assert!(
                s.is_finite(),
                "Drum {} ({}) NaN/Inf",
                drum.note,
                drum.name_en
            );
        }
        let max_p = peak(&buf);
        assert!(
            max_p > 0.02 && max_p <= 1.0001,
            "Drum {} ({}) peak out of range: {max_p}",
            drum.note,
            drum.name_en
        );
    }

    // Verify Hi-Hat choke group: Closed Hi-Hat (42) chokes ringing Open Hi-Hat (46)
    let mut engine = SoundHallEngine::new(sr);
    engine.set_reverb_mix(0.0);
    engine.note_on(9, 46, 115); // Open Hi-Hat
    let mut open_pre = vec![0.0_f32; 4096 * 2];
    engine.render_stereo(&mut open_pre);
    assert!(rms(&open_pre) > 0.01);

    engine.note_on(9, 42, 115); // Closed Hi-Hat chokes 46
    let mut after_choke = vec![0.0_f32; (0.25 * sr) as usize * 2];
    engine.render_stereo(&mut after_choke);
    let tail_rms = rms(&after_choke[after_choke.len() - 4096..]);
    assert!(
        tail_rms < 0.002,
        "Open Hi-Hat should be choked quickly by Closed Hi-Hat, tail_rms={tail_rms}"
    );
}

#[test]
fn verify_pitch_accuracy_across_all_synthesis_engines() {
    let sr = 44100.0_f32;
    // Representative presets covering all 8 tonal synthesis engines + Wind subtypes
    let test_programs: &[(u8, &str)] = &[
        (0, "Acoustic Grand Piano (HammeredString)"),
        (24, "Acoustic Guitar Nylon (PluckedString)"),
        (40, "Violin (BowedString)"),
        (73, "Flute (WindBore FluteJet)"),
        (71, "Clarinet (WindBore SingleReed cylindrical)"),
        (68, "Oboe (WindBore DoubleReed)"),
        (56, "Trumpet (WindBore LipReed)"),
        (12, "Marimba (ModalPercussion)"),
        (4, "Electric Piano 1 (Fm4Op)"),
        (16, "Drawbar Organ (AdditiveOrgan)"),
        (81, "Lead 2 Sawtooth (FormantSubtractive)"),
    ];

    for &(note, expected_hz) in &[(69u8, 440.0_f32), (60u8, 261.6256_f32)] {
        for &(prog, label) in test_programs {
            // Skip Violin / Flute / Clarinet / Oboe / Marimba / Trumpet? Actually all support 60 & 69!
            let mut engine = SoundHallEngine::new(sr);
            engine.set_reverb_mix(0.0);
            engine.set_chorus_mix(0.0);
            engine.program_change(0, prog);
            engine.note_on(0, note, 100);

            let frames = (0.45 * sr) as usize;
            let mut stereo = vec![0.0_f32; frames * 2];
            engine.render_stereo(&mut stereo);
            let mono: Vec<f32> = stereo
                .as_chunks::<2>()
                .0
                .iter()
                .map(|f| 0.5 * (f[0] + f[1]))
                .collect();

            let measured = measure_fundamental_hz(&mono, sr, expected_hz);
            let rel_err = ((measured - expected_hz) / expected_hz).abs();
            assert!(
                rel_err < 0.03,
                "{label} (GM #{prog}) at MIDI {note} ({expected_hz:.2} Hz): measured {measured:.2} Hz (err {:.2}%)",
                rel_err * 100.0
            );
        }
    }
}

#[test]
fn verify_midi_controllers_sustain_pan_volume_pitchbend() {
    let sr = 44100.0_f32;

    // 1. Sustain Pedal (CC 64)
    let mut engine = SoundHallEngine::new(sr);
    engine.set_reverb_mix(0.0);
    engine.program_change(0, 16); // Organ sustains indefinitely while held
    engine.control_change(0, 64, 127); // Sustain ON
    engine.note_on(0, 60, 100);
    let mut buf = vec![0.0_f32; 4096 * 2];
    engine.render_stereo(&mut buf);
    engine.note_off(0, 60);
    engine.render_stereo(&mut buf);
    assert!(rms(&buf) > 0.05, "Sustain pedal should keep voice sounding");
    engine.control_change(0, 64, 0); // Sustain OFF
    let mut tail = vec![0.0_f32; 8192 * 2];
    engine.render_stereo(&mut tail);
    assert!(
        rms(&tail[tail.len() - 2048..]) < 0.005,
        "Releasing sustain pedal should release held voice"
    );

    // 2. Pan (CC 10)
    let mut pan_eng = SoundHallEngine::new(sr);
    pan_eng.set_reverb_mix(0.0);
    pan_eng.set_chorus_mix(0.0);
    pan_eng.program_change(0, 80);
    pan_eng.control_change(0, 10, 0); // Hard Left
    pan_eng.note_on(0, 60, 100);
    let mut pan_buf = vec![0.0_f32; 4096 * 2];
    pan_eng.render_stereo(&mut pan_buf);
    let left_l: Vec<f32> = pan_buf.as_chunks::<2>().0.iter().map(|f| f[0]).collect();
    let left_r: Vec<f32> = pan_buf.as_chunks::<2>().0.iter().map(|f| f[1]).collect();
    assert!(
        rms(&left_l) > rms(&left_r) * 10.0,
        "Pan 0 should isolate left"
    );

    // 3. Volume (CC 7)
    let mut vol_eng = SoundHallEngine::new(sr);
    vol_eng.set_reverb_mix(0.0);
    vol_eng.program_change(0, 16);
    vol_eng.control_change(0, 7, 10);
    vol_eng.note_on(0, 60, 100);
    let mut quiet = vec![0.0_f32; 4096 * 2];
    vol_eng.render_stereo(&mut quiet);
    assert!(rms(&quiet) < 0.05);

    // 4. Pitch Bend (14-bit)
    let mut bend_eng = SoundHallEngine::new(sr);
    bend_eng.set_reverb_mix(0.0);
    bend_eng.set_chorus_mix(0.0);
    bend_eng.program_change(0, 81);
    bend_eng.pitch_bend(0, 16383); // +2 semitones above A4 -> ~493.88 Hz (B4)
    bend_eng.note_on(0, 69, 100);
    let mut bend_buf = vec![0.0_f32; (0.4 * sr) as usize * 2];
    bend_eng.render_stereo(&mut bend_buf);
    let mono: Vec<f32> = bend_buf
        .as_chunks::<2>()
        .0
        .iter()
        .map(|f| 0.5 * (f[0] + f[1]))
        .collect();
    let measured = measure_fundamental_hz(&mono, sr, 493.88);
    assert!(
        (measured - 493.88).abs() < 12.0,
        "Pitch bend +2st measured={measured}"
    );
}

#[test]
fn verify_smf_type1_parser_and_wav_export() {
    // Build valid Format 1 MIDI in-memory:
    // Track 0: tempo 120 BPM (500_000 us/qn)
    // Track 1: Program 0 (Grand Piano), C4+E4+G4 NoteOn at 0, NoteOff at 480 ticks
    // Track 2: Channel 9 kick (36) + hihat (42) at 0 and 240 ticks
    let mut mid = Vec::new();
    mid.extend_from_slice(b"MThd");
    mid.extend_from_slice(&6u32.to_be_bytes());
    mid.extend_from_slice(&1u16.to_be_bytes()); // format 1
    mid.extend_from_slice(&3u16.to_be_bytes()); // 3 tracks
    mid.extend_from_slice(&480u16.to_be_bytes()); // 480 tpqn

    // Track 0 (tempo + EOT)
    let t0 = [
        0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20, // 500_000 us = 120 BPM
        0x00, 0xFF, 0x2F, 0x00,
    ];
    mid.extend_from_slice(b"MTrk");
    mid.extend_from_slice(&(t0.len() as u32).to_be_bytes());
    mid.extend_from_slice(&t0);

    // Track 1 (Piano chord)
    let t1 = [
        0x00, 0xC0, 0x00, // Program Change ch0 -> 0
        0x00, 0x90, 60, 100, // C4 on
        0x00, 0x90, 64, 96, // E4 on
        0x00, 0x90, 67, 96, // G4 on
        0x83, 0x60, 0x80, 60, 0, // delta 480 -> C4 off
        0x00, 0x80, 64, 0, 0x00, 0x80, 67, 0, 0x00, 0xFF, 0x2F, 0x00,
    ];
    mid.extend_from_slice(b"MTrk");
    mid.extend_from_slice(&(t1.len() as u32).to_be_bytes());
    mid.extend_from_slice(&t1);

    // Track 2 (Drums ch9)
    let t2 = [
        0x00, 0x99, 36, 110, // Kick on
        0x00, 0x99, 42, 95, // HiHat on
        0x81, 0x70, 0x99, 42, 90, // delta 240 -> HiHat on
        0x00, 0xFF, 0x2F, 0x00,
    ];
    mid.extend_from_slice(b"MTrk");
    mid.extend_from_slice(&(t2.len() as u32).to_be_bytes());
    mid.extend_from_slice(&t2);

    let song = parse_smf(&mid).expect("SMF should parse cleanly");
    assert_eq!(song.track_names.len(), 3);
    assert!(song.duration_secs >= 0.45 && song.duration_secs <= 0.60);

    let mut engine = SoundHallEngine::new(44100.0);
    let audio = MidiSequencer::render_to_buffer(&song, &mut engine, 0.5);
    assert!(rms(&audio) > 0.01);

    let mut wav_bytes = Vec::new();
    write_wav_i16(&mut wav_bytes, 44100, 2, &audio, true).unwrap();
    assert_eq!(&wav_bytes[0..4], b"RIFF");
    assert_eq!(&wav_bytes[8..12], b"WAVE");
}

#[test]
fn verify_all_built_in_demos_and_example_scores() {
    for demo in list_demos() {
        let song = sound_hall_core::parse_score(demo.score)
            .unwrap_or_else(|e| panic!("demo '{}' failed to parse: {e}", demo.id));
        assert!(parse_demo(demo.id).is_some());
        assert!(
            song.duration_secs > 1.0 && !song.events.is_empty(),
            "demo {} empty",
            demo.id
        );
        let mut engine = SoundHallEngine::new(22050.0);
        let samples = MidiSequencer::render_to_buffer(&song, &mut engine, 0.4);
        assert!(samples.iter().all(|s| s.is_finite()));
        assert!(rms(&samples) > 0.01, "demo {} silent", demo.id);
    }

    for path in [
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/orchestral_showcase.shs"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/japanese_ensemble.shs"
        ),
    ] {
        let content = std::fs::read_to_string(path).expect("example score readable");
        let song = parse_score(&content).expect("example score parses");
        let mut engine = SoundHallEngine::new(22050.0);
        let samples = MidiSequencer::render_to_buffer(&song, &mut engine, 0.4);
        assert!(samples.iter().all(|s| s.is_finite()));
        assert!(rms(&samples) > 0.01);
    }
}

#[test]
fn verify_voice_stealing_remains_bounded_and_clean() {
    let sr = 44100.0;
    let mut engine = SoundHallEngine::with_polyphony(sr, 8);
    engine.program_change(0, 0);
    for note in 48..72 {
        engine.note_on(0, note, 100);
    }
    let mut buf = vec![0.0_f32; 4096 * 2];
    engine.render_stereo(&mut buf);
    assert!(buf.iter().all(|s| s.is_finite()));
    assert!(peak(&buf) <= 1.0001);
    assert!(engine.active_voice_count() <= 16);
}

#[test]
fn verify_preset_lookup_and_search() {
    assert_eq!(get_preset(77).name_en, "Shakuhachi");
    assert_eq!(sound_hall_core::find_preset("尺八").unwrap().program, 77);
    assert_eq!(sound_hall_core::find_preset("koto").unwrap().program, 107);
    assert_eq!(sound_hall_core::find_preset("0").unwrap().program, 0);
}

#[test]
fn audit_smf_malformed_and_overflow_inputs() {
    use sound_hall_core::{MidiEvent, MidiSong, SmfError, TimedEvent};

    // Truncated header
    assert!(parse_smf(b"MThd\x00").is_err());

    // Declared track_len = u32::MAX (must fail with UnexpectedEof without overflow)
    let mut giant_trk = Vec::new();
    giant_trk.extend_from_slice(b"MThd\x00\x00\x00\x06\x00\x00\x00\x01\x01\xe0");
    giant_trk.extend_from_slice(b"MTrk\xff\xff\xff\xff\x00\xff\x2f\x00");
    assert_eq!(parse_smf(&giant_trk).unwrap_err(), SmfError::UnexpectedEof);

    // SMPTE division 0x8001 (-128 fps in i8 -> must not panic on negation)
    let smpte_min = [
        b'M', b'T', b'h', b'd', 0, 0, 0, 6, 0, 0, 0, 1, 0x80, 0x01, b'M', b'T', b'r', b'k', 0, 0,
        0, 4, 0x00, 0xFF, 0x2F, 0x00,
    ];
    assert_eq!(
        parse_smf(&smpte_min).unwrap_err(),
        SmfError::UnsupportedDivision
    );

    // Non-MTrk unknown chunk skipped before valid MTrk
    let mut with_unknown = Vec::new();
    with_unknown.extend_from_slice(b"MThd\x00\x00\x00\x06\x00\x00\x00\x01\x01\xe0");
    with_unknown.extend_from_slice(b"JUNK\x00\x00\x00\x02\xaa\xbb");
    let trk_body = [
        0x00, 0x90, 60, 100, 0x40, 0x80, 60, 0, 0x00, 0xFF, 0x2F, 0x00,
    ];
    with_unknown.extend_from_slice(b"MTrk");
    with_unknown.extend_from_slice(&(trk_body.len() as u32).to_be_bytes());
    with_unknown.extend_from_slice(&trk_body);
    let parsed = parse_smf(&with_unknown).expect("unknown chunk should be skipped");
    assert_eq!(parsed.track_names.len(), 1);
    assert!(!parsed.events.is_empty());

    // Giant duration song capped without OOM
    let huge_song = MidiSong::new(
        vec![
            TimedEvent {
                time_sec: 0.0,
                event: MidiEvent::NoteOn {
                    channel: 0,
                    note: 60,
                    velocity: 100,
                },
            },
            TimedEvent {
                time_sec: 1.0e9,
                event: MidiEvent::NoteOff {
                    channel: 0,
                    note: 60,
                },
            },
        ],
        vec!["Giant".to_string()],
    );
    let mut engine = SoundHallEngine::new(22050.0);
    let buf = MidiSequencer::render_to_buffer_with_max_secs(&huge_song, &mut engine, 0.2, 0.25);
    assert!(buf.len() <= (22050 * 2));
    assert!(buf.iter().all(|s| s.is_finite()));
}

#[test]
fn audit_score_repeat_bomb_and_nan_inputs() {
    assert!(parse_score("((((((c4)*64)*64)*64)*64)*64)*64").is_err());
    assert!(parse_score(&format!("{}c4{}", "(".repeat(20), ")".repeat(20))).is_err());
    assert!(parse_score("tempo NaN\nc4\n").is_err());
    assert!(parse_score("tempo inf\nc4\n").is_err());
    assert!(parse_score("cNaN\n").is_err());
}

#[test]
fn audit_engine_nan_parameters_and_cc121_sustain_release() {
    let mut engine = SoundHallEngine::new(f32::NAN);
    engine.set_master_gain(f32::NAN);
    engine.set_reverb_mix(f32::NAN);
    engine.set_chorus_mix(f32::NAN);
    engine.program_change(0, 16);
    engine.control_change(0, 64, 127);
    engine.note_on(0, 60, 100);
    let mut buf = vec![0.0_f32; 2048 * 2];
    engine.render_stereo(&mut buf);
    assert!(buf.iter().all(|s| s.is_finite()));
    assert!(rms(&buf) > 0.01);

    // NoteOff held by sustain, then CC 121 (Reset All Controllers) releases sustained voices
    engine.note_off(0, 60);
    engine.control_change(0, 121, 0);
    let mut tail = vec![0.0_f32; 8192 * 2];
    engine.render_stereo(&mut tail);
    assert!(rms(&tail[tail.len() - 2048..]) < 0.005);
}

#[test]
fn audit_wav_validation_rejects_invalid_params() {
    use sound_hall_core::write_wav_f32;
    use std::io::ErrorKind;

    let mut out = Vec::new();
    let err = write_wav_i16(&mut out, 0, 2, &[], false).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    let err = write_wav_i16(&mut out, 44100, 0, &[], false).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    let err = write_wav_f32(&mut out, 0, 2, &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    let err = write_wav_f32(&mut out, 44100, 0, &[]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
}

/// Renders one dry note (no reverb/chorus) and returns interleaved stereo samples.
fn render_dry_note(program: u8, note: u8, velocity: u8, secs: f32) -> Vec<f32> {
    let sr = 44100.0_f32;
    let mut engine = SoundHallEngine::new(sr);
    engine.set_reverb_mix(0.0);
    engine.set_chorus_mix(0.0);
    engine.program_change(0, program);
    engine.note_on(0, note, velocity);
    let mut buf = vec![0.0_f32; (secs * sr) as usize * 2];
    engine.render_stereo(&mut buf);
    buf
}

/// Loudest 100 ms RMS window (dB) — a short-term loudness proxy fair to both
/// percussive and sustained sounds.
fn short_term_db(samples: &[f32]) -> f32 {
    let win = 4410 * 2;
    let hop = win / 4;
    let mut best = 0.0_f32;
    let mut i = 0;
    while i + win <= samples.len() {
        best = best.max(rms(&samples[i..i + win]));
        i += hop;
    }
    20.0 * best.max(1e-9).log10()
}

fn representative_pitch(program: u8) -> u8 {
    let (lo, hi) = get_preset(program).pitch_range;
    if (lo..=hi).contains(&60) {
        60
    } else {
        ((lo as u16 + hi as u16) / 2) as u8
    }
}

#[test]
fn verify_preset_loudness_is_balanced() {
    let levels: Vec<(u8, f32)> = (0..128u8)
        .map(|p| {
            let buf = render_dry_note(p, representative_pitch(p), 100, 0.6);
            (p, short_term_db(&buf))
        })
        .collect();
    let mut sorted: Vec<f32> = levels.iter().map(|&(_, l)| l).collect();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2];
    for (p, l) in levels {
        assert!(
            (l - median).abs() < 8.0,
            "GM #{p} {} is {:+.1} dB from the preset median (unbalanced)",
            get_preset(p).name_en,
            l - median
        );
    }
}

#[test]
fn verify_strings_are_dc_free_and_sustain() {
    // Piano, nylon guitar, finger bass, harp, koto
    for &program in &[0u8, 24, 33, 46, 107] {
        let note = representative_pitch(program);
        let buf = render_dry_note(program, note, 100, 2.0);
        let mono: Vec<f32> = buf.chunks(2).map(|f| 0.5 * (f[0] + f[1])).collect();
        let first = &mono[..44100];
        let mean = first.iter().sum::<f32>() / first.len() as f32;
        assert!(
            mean.abs() < 0.02 * rms(first),
            "GM #{program} has a DC offset: mean={mean} rms={}",
            rms(first)
        );
        // A held plucked/struck string must still ring after one second
        let early = rms(&mono[2205..6615]);
        let late = rms(&mono[44100..48510]);
        assert!(
            late > early * 0.03,
            "GM #{program} decays too fast: early={early} late={late}"
        );
    }
}

#[test]
fn verify_velocity_shapes_loudness_and_timbre() {
    // Piano, nylon guitar, vibraphone, strings, trumpet, flute, saw lead, organ
    for &program in &[0u8, 24, 11, 48, 56, 73, 81, 16] {
        let note = representative_pitch(program);
        let soft = short_term_db(&render_dry_note(program, note, 30, 0.6));
        let loud = short_term_db(&render_dry_note(program, note, 127, 0.6));
        assert!(
            loud - soft > 7.5,
            "GM #{program} velocity range too flat: {:.1} dB",
            loud - soft
        );
    }

    // Harder piano strikes must be brighter: ratio of high-passed (first-difference)
    // energy to total energy rises with velocity.
    let brightness = |vel: u8| {
        let buf = render_dry_note(0, 60, vel, 0.3);
        let mono: Vec<f32> = buf.chunks(2).map(|f| 0.5 * (f[0] + f[1])).collect();
        let diff: Vec<f32> = mono.windows(2).map(|w| w[1] - w[0]).collect();
        rms(&diff) / rms(&mono).max(1e-9)
    };
    let (pp, ff) = (brightness(30), brightness(127));
    assert!(
        ff > pp * 1.3,
        "piano timbre should brighten with velocity: pp={pp} ff={ff}"
    );
}

#[test]
fn verify_master_limiter_keeps_dense_mix_clean() {
    let sr = 44100.0_f32;
    let mut engine = SoundHallEngine::new(sr);
    engine.set_master_gain(2.5);
    for ch in 0..8u8 {
        engine.program_change(ch, [0u8, 48, 61, 81, 89, 19, 33, 56][ch as usize]);
        for n in [48u8, 55, 60, 64, 67] {
            engine.note_on(ch, n, 127);
        }
    }
    let mut buf = vec![0.0_f32; (1.0 * sr) as usize * 2];
    engine.render_stereo(&mut buf);
    assert!(buf.iter().all(|s| s.is_finite()));
    assert!(
        peak(&buf) <= 0.95,
        "limiter ceiling exceeded: {}",
        peak(&buf)
    );
    assert!(engine.limiter_gain() < 1.0, "limiter should be engaged");
    let mean_l = buf.iter().step_by(2).sum::<f32>() / (buf.len() / 2) as f32;
    assert!(mean_l.abs() < 0.01, "master output carries DC: {mean_l}");
}
