use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::TcpListener;
use std::path::{Component, Path, PathBuf};
use std::process::{self, Command};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sound_hall_core::{
    all_drums, all_presets, find_preset, get_demo, list_demos, parse_demo, parse_score, parse_smf,
    search_presets, write_wav_i16, MidiEvent, MidiSequencer, MidiSong, SoundHallEngine, TimedEvent,
};

fn print_help() {
    println!(
        r#"SoundHall (sound-hall) — 超軽量・ゼロサンプル物理モデリング＋ハイブリッド音源エンジン

USAGE:
  sound-hall <COMMAND> [OPTIONS]

COMMANDS:
  list [--family <name>] [--search <query>] [--drums]
      GM全128楽器または47ドラムキットを一覧表示
  info <program_or_name>
      指定した楽器の合成エンジン・音響パラメータ・音域詳細を表示
  note <program_or_name> [pitch=60] [--duration 1.5] [--velocity 100] [--out path.wav] [--play]
      単音を合成してWAV書き出し / 試聴再生
  chord <program_or_name> [pitches=60,64,67] [--duration 2.0] [--arpeggio 0.05] [--out path.wav] [--play]
      和音（アルペジオ対応）を合成してWAV書き出し / 試聴再生
  demo [name|list] [--out path.wav] [--play] [--reverb 0.3] [--normalize]
      内蔵デモ曲の一覧表示またはレンダリング・再生 (orchestra, jazz, wagakki, electronic, gm-tour)
  render <input.mid|input.shs> -o <output.wav> [--sample-rate 44100] [--reverb 0.3] [--normalize]
      MIDI (.mid) またはスコア (.shs) ファイルをWAVにレンダリング
  play <input.mid|input.shs> [--sample-rate 44100] [--reverb 0.3]
      MIDI (.mid) またはスコア (.shs) ファイルを即座に再生
  benchmark [--voices 32] [--seconds 10] [--sample-rate 44100]
      マルチボイス同時発音の処理速度・リアルタイム倍率を測定
  serve [--port 8080] [--dir web]
      ビジュアルWebスタジオ (SoundHall Studio) のローカルHTTPサーバーを起動"#
    );
}

fn die(msg: &str) -> ! {
    eprintln!("Error: {msg}");
    process::exit(1);
}

fn require_arg<'a>(args: &'a [String], idx: usize, flag: &str) -> &'a str {
    args.get(idx)
        .map(|s| s.as_str())
        .unwrap_or_else(|| die(&format!("missing value for {flag}")))
}

fn parse_f64_arg(args: &[String], idx: usize, flag: &str, min: f64, max: f64) -> f64 {
    let raw = require_arg(args, idx, flag);
    let val: f64 = raw
        .parse()
        .unwrap_or_else(|_| die(&format!("invalid number '{raw}' for {flag}")));
    if !val.is_finite() {
        die(&format!("non-finite number '{raw}' for {flag}"));
    }
    val.clamp(min, max)
}

fn parse_f32_arg(args: &[String], idx: usize, flag: &str, min: f32, max: f32) -> f32 {
    let raw = require_arg(args, idx, flag);
    let val: f32 = raw
        .parse()
        .unwrap_or_else(|_| die(&format!("invalid number '{raw}' for {flag}")));
    if !val.is_finite() {
        die(&format!("non-finite number '{raw}' for {flag}"));
    }
    val.clamp(min, max)
}

fn parse_u32_arg(args: &[String], idx: usize, flag: &str, min: u32, max: u32) -> u32 {
    let raw = require_arg(args, idx, flag);
    raw.parse::<u32>()
        .unwrap_or_else(|_| die(&format!("invalid integer '{raw}' for {flag}")))
        .clamp(min, max)
}

fn parse_u8_arg(args: &[String], idx: usize, flag: &str, min: u8, max: u8) -> u8 {
    let raw = require_arg(args, idx, flag);
    raw.parse::<u8>()
        .unwrap_or_else(|_| die(&format!("invalid byte '{raw}' for {flag}")))
        .clamp(min, max)
}

fn parse_usize_arg(args: &[String], idx: usize, flag: &str, min: usize, max: usize) -> usize {
    let raw = require_arg(args, idx, flag);
    raw.parse::<usize>()
        .unwrap_or_else(|_| die(&format!("invalid integer '{raw}' for {flag}")))
        .clamp(min, max)
}

fn parse_note_arg(s: &str) -> Result<u8, String> {
    let trimmed = s.trim();
    if let Ok(n) = trimmed.parse::<u8>() {
        return Ok(n.min(127));
    }
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() {
        return Err("empty pitch".to_string());
    }
    let base: i32 = match bytes[0].to_ascii_lowercase() {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        b'b' => 11,
        _ => {
            return Err(format!(
                "invalid pitch '{s}' (use MIDI 0..127 or e.g. C4, F#3)"
            ))
        }
    };
    let mut idx = 1usize;
    let mut acc = 0i32;
    while idx < bytes.len() {
        match bytes[idx] {
            b'#' | b'+' => {
                acc += 1;
                idx += 1;
            }
            b'b' | b'-' => {
                acc -= 1;
                idx += 1;
            }
            _ => break,
        }
    }
    let oct: i32 = trimmed[idx..]
        .parse()
        .map_err(|_| format!("invalid octave in pitch '{s}'"))?;
    let midi = ((oct + 1) * 12 + base + acc).clamp(0, 127);
    Ok(midi as u8)
}

fn write_wav_file(
    path: &Path,
    sample_rate: u32,
    samples: &[f32],
    normalize: bool,
) -> Result<(), String> {
    let file =
        File::create(path).map_err(|e| format!("failed to create {}: {e}", path.display()))?;
    let mut writer = BufWriter::new(file);
    write_wav_i16(&mut writer, sample_rate, 2, samples, normalize)
        .map_err(|e| format!("failed to write WAV {}: {e}", path.display()))
}

fn create_exclusive_temp_wav() -> Result<(PathBuf, File), String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for attempt in 0..16u32 {
        let path = env::temp_dir().join(format!(
            "sound_hall_play_{}_{}_{}.wav",
            process::id(),
            nanos,
            attempt
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("failed to create temp WAV {}: {e}", path.display())),
        }
    }
    Err("could not allocate unique temporary WAV path".to_string())
}

fn play_samples(sample_rate: u32, samples: &[f32], normalize: bool) -> Result<(), String> {
    let (tmp, file) = create_exclusive_temp_wav()?;
    {
        let mut writer = BufWriter::new(file);
        if let Err(e) = write_wav_i16(&mut writer, sample_rate, 2, samples, normalize) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("failed to write temp WAV {}: {e}", tmp.display()));
        }
    }

    let status = if cfg!(target_os = "macos") {
        Command::new("afplay").arg(&tmp).status()
    } else {
        Command::new("paplay")
            .arg(&tmp)
            .status()
            .or_else(|_| Command::new("aplay").arg(&tmp).status())
            .or_else(|_| {
                Command::new("ffplay")
                    .args(["-nodisp", "-autoexit", "-loglevel", "quiet"])
                    .arg(&tmp)
                    .status()
            })
    };

    let _ = fs::remove_file(&tmp);
    match status {
        Ok(st) if st.success() => Ok(()),
        Ok(st) => Err(format!("audio player exited with status {st}")),
        Err(e) => Err(format!("could not launch audio player: {e}")),
    }
}

fn load_song(path: &Path) -> Result<MidiSong, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "mid" || ext == "midi" {
        let bytes =
            fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        parse_smf(&bytes).map_err(|e| format!("MIDI parse error: {e}"))
    } else {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        parse_score(&text).map_err(|e| format!("{e}"))
    }
}

fn cmd_list(args: &[String]) {
    let mut family_filter: Option<String> = None;
    let mut search_filter: Option<String> = None;
    let mut show_drums = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--family" | "-f" => {
                i += 1;
                family_filter = Some(require_arg(args, i, "--family").to_string());
            }
            "--search" | "-s" => {
                i += 1;
                search_filter = Some(require_arg(args, i, "--search").to_string());
            }
            "--drums" | "-d" => show_drums = true,
            other => die(&format!("unknown option for list: {other}")),
        }
        i += 1;
    }

    if show_drums {
        println!("GM Standard Percussion Kit (47 voices, Channel 10 / ch=9):");
        println!(
            "{:<5} {:<26} {:<30} {:<6}",
            "Note", "Name (EN)", "名前 (JA)", "Choke"
        );
        println!("{}", "-".repeat(72));
        for d in all_drums() {
            println!(
                "{:<5} {:<26} {:<30} {}",
                d.note,
                d.name_en,
                d.name_ja,
                if d.choke_group == 0 {
                    "-".to_string()
                } else {
                    format!("G{}", d.choke_group)
                }
            );
        }
        return;
    }

    let list = if let Some(ref q) = search_filter {
        search_presets(q)
    } else {
        all_presets().iter().collect()
    };

    println!(
        "{:<4} {:<30} {:<32} {:<22}",
        "Prog", "Name (EN)", "名前 (JA)", "Family"
    );
    println!("{}", "-".repeat(94));
    let mut count = 0;
    for p in list {
        if let Some(ref fam) = family_filter {
            let fl = fam.to_ascii_lowercase();
            if !p.family.name_en().to_ascii_lowercase().contains(&fl)
                && !p.family.name_ja().contains(fam)
            {
                continue;
            }
        }
        println!(
            "{:<4} {:<30} {:<32} {}",
            p.program,
            p.name_en,
            p.name_ja,
            p.family.name_en()
        );
        count += 1;
    }
    println!("\nTotal: {count} instruments (use `--drums` for 47 GM percussion voices)");
}

fn cmd_info(args: &[String]) {
    let query = args
        .first()
        .unwrap_or_else(|| die("usage: sound-hall info <program_or_name>"));
    let p =
        find_preset(query).unwrap_or_else(|| die(&format!("instrument not found for '{query}'")));

    println!("Program     : {} (GM #{})", p.name_en, p.program);
    println!("日本語名    : {}", p.name_ja);
    println!(
        "Family      : {} / {}",
        p.family.name_en(),
        p.family.name_ja()
    );
    println!("Synth Engine: {}", p.patch.engine_name());
    println!("Gain        : {:.2}", p.gain);
    println!(
        "Effects     : Reverb {:.0}%, Chorus {:.0}%",
        p.default_reverb * 100.0,
        p.default_chorus * 100.0
    );
    println!(
        "Pitch Range : MIDI {}..={}",
        p.pitch_range.0, p.pitch_range.1
    );
}

fn cmd_note(args: &[String]) {
    if args.is_empty() {
        die("usage: sound-hall note <program_or_name> [pitch=60] [--duration 1.5] [--velocity 100] [--out path.wav] [--play]");
    }
    let query = &args[0];
    let preset =
        find_preset(query).unwrap_or_else(|| die(&format!("instrument not found for '{query}'")));

    let mut pitch = 60u8;
    let mut duration = 1.5_f64;
    let mut velocity = 100u8;
    let mut out_path: Option<PathBuf> = None;
    let mut do_play = false;
    let mut sample_rate = 44100u32;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--duration" | "-d" => {
                i += 1;
                duration = parse_f64_arg(args, i, "--duration", 0.1, 30.0);
            }
            "--velocity" | "-v" => {
                i += 1;
                velocity = parse_u8_arg(args, i, "--velocity", 1, 127);
            }
            "--out" | "-o" => {
                i += 1;
                out_path = Some(PathBuf::from(require_arg(args, i, "--out")));
            }
            "--sample-rate" | "-r" => {
                i += 1;
                sample_rate = parse_u32_arg(args, i, "--sample-rate", 8000, 192_000);
            }
            "--play" | "-p" => do_play = true,
            arg if !arg.starts_with('-') && i == 1 => {
                pitch = parse_note_arg(arg).unwrap_or_else(|e| die(&e));
            }
            other => die(&format!("unknown option for note: {other}")),
        }
        i += 1;
    }

    if out_path.is_none() && !do_play {
        do_play = true;
    }

    let song = MidiSong::new(
        vec![
            TimedEvent {
                time_sec: 0.0,
                event: MidiEvent::ProgramChange {
                    channel: 0,
                    program: preset.program,
                },
            },
            TimedEvent {
                time_sec: 0.0,
                event: MidiEvent::NoteOn {
                    channel: 0,
                    note: pitch,
                    velocity,
                },
            },
            TimedEvent {
                time_sec: duration,
                event: MidiEvent::NoteOff {
                    channel: 0,
                    note: pitch,
                },
            },
        ],
        vec![preset.name_en.to_string()],
    );

    let mut engine = SoundHallEngine::new(sample_rate as f32);
    let samples = MidiSequencer::render_to_buffer(&song, &mut engine, 1.0);

    if let Some(ref p) = out_path {
        write_wav_file(p, sample_rate, &samples, true).unwrap_or_else(|e| die(&e));
        println!(
            "Rendered GM #{} {} (MIDI {}, {:.2}s) -> {}",
            preset.program,
            preset.name_en,
            pitch,
            duration,
            p.display()
        );
    }
    if do_play {
        println!(
            "Playing GM #{} {} (MIDI {}, vel {})...",
            preset.program, preset.name_en, pitch, velocity
        );
        play_samples(sample_rate, &samples, true).unwrap_or_else(|e| die(&e));
    }
}

fn cmd_chord(args: &[String]) {
    if args.is_empty() {
        die("usage: sound-hall chord <program_or_name> [pitches=60,64,67] [--duration 2.0] [--arpeggio 0.05] [--out path.wav] [--play]");
    }
    let preset = find_preset(&args[0])
        .unwrap_or_else(|| die(&format!("instrument not found for '{}'", args[0])));

    let mut pitches = vec![60u8, 64, 67];
    let mut duration = 2.0_f64;
    let mut arpeggio = 0.05_f64;
    let mut out_path: Option<PathBuf> = None;
    let mut do_play = false;
    let mut sample_rate = 44100u32;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--duration" | "-d" => {
                i += 1;
                duration = parse_f64_arg(args, i, "--duration", 0.1, 30.0);
            }
            "--arpeggio" | "-a" => {
                i += 1;
                arpeggio = parse_f64_arg(args, i, "--arpeggio", 0.0, 2.0);
            }
            "--out" | "-o" => {
                i += 1;
                out_path = Some(PathBuf::from(require_arg(args, i, "--out")));
            }
            "--sample-rate" | "-r" => {
                i += 1;
                sample_rate = parse_u32_arg(args, i, "--sample-rate", 8000, 192_000);
            }
            "--play" | "-p" => do_play = true,
            arg if !arg.starts_with('-') && i == 1 => {
                pitches = arg
                    .split(',')
                    .map(|p| parse_note_arg(p).unwrap_or_else(|e| die(&e)))
                    .collect();
            }
            other => die(&format!("unknown option for chord: {other}")),
        }
        i += 1;
    }

    if out_path.is_none() && !do_play {
        do_play = true;
    }

    let mut events = vec![TimedEvent {
        time_sec: 0.0,
        event: MidiEvent::ProgramChange {
            channel: 0,
            program: preset.program,
        },
    }];
    for (idx, &note) in pitches.iter().enumerate() {
        let onset = idx as f64 * arpeggio;
        events.push(TimedEvent {
            time_sec: onset,
            event: MidiEvent::NoteOn {
                channel: 0,
                note,
                velocity: 96,
            },
        });
        events.push(TimedEvent {
            time_sec: onset + duration,
            event: MidiEvent::NoteOff { channel: 0, note },
        });
    }

    let song = MidiSong::new(events, vec![preset.name_en.to_string()]);
    let mut engine = SoundHallEngine::new(sample_rate as f32);
    let samples = MidiSequencer::render_to_buffer(&song, &mut engine, 1.2);

    if let Some(ref p) = out_path {
        write_wav_file(p, sample_rate, &samples, true).unwrap_or_else(|e| die(&e));
        println!("Rendered chord ({:?}) -> {}", pitches, p.display());
    }
    if do_play {
        println!(
            "Playing GM #{} {} chord {:?}...",
            preset.program, preset.name_en, pitches
        );
        play_samples(sample_rate, &samples, true).unwrap_or_else(|e| die(&e));
    }
}

fn cmd_demo(args: &[String]) {
    let mut demo_name = "orchestra".to_string();
    let mut explicit_name = false;
    let mut out_path: Option<PathBuf> = None;
    let mut do_play = false;
    let mut sample_rate = 44100u32;
    let mut reverb = 0.32_f32;
    let mut normalize = true;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "list" => {
                println!("Built-in Demos:");
                for d in list_demos() {
                    println!("  {:<12} {} ({})", d.id, d.title_en, d.title_ja);
                    println!("               {}", d.description);
                }
                return;
            }
            "--out" | "-o" => {
                i += 1;
                out_path = Some(PathBuf::from(require_arg(args, i, "--out")));
            }
            "--play" | "-p" => do_play = true,
            "--sample-rate" | "-r" => {
                i += 1;
                sample_rate = parse_u32_arg(args, i, "--sample-rate", 8000, 192_000);
            }
            "--reverb" => {
                i += 1;
                reverb = parse_f32_arg(args, i, "--reverb", 0.0, 1.0);
            }
            "--normalize" => normalize = true,
            "--no-normalize" => normalize = false,
            arg if !arg.starts_with('-') && !explicit_name => {
                demo_name = arg.to_string();
                explicit_name = true;
            }
            other => die(&format!("unknown option for demo: {other}")),
        }
        i += 1;
    }

    if args.is_empty() {
        println!("Available demos (run `sound-hall demo <name> --play` or `--out file.wav`):");
        for d in list_demos() {
            println!("  {:<12} {} — {}", d.id, d.title_en, d.description);
        }
        return;
    }

    if out_path.is_none() && !do_play {
        do_play = true;
    }

    let demo = get_demo(&demo_name).unwrap_or_else(|| {
        die(&format!(
            "unknown demo '{demo_name}' (try: orchestra, jazz, wagakki, electronic, gm-tour)"
        ))
    });
    let song = parse_demo(&demo_name).unwrap_or_else(|| die("failed to parse built-in demo"));

    let mut engine = SoundHallEngine::new(sample_rate as f32);
    engine.set_reverb_mix(reverb);
    let samples = MidiSequencer::render_to_buffer(&song, &mut engine, 1.8);

    if let Some(ref p) = out_path {
        write_wav_file(p, sample_rate, &samples, normalize).unwrap_or_else(|e| die(&e));
        println!(
            "Rendered demo '{}' ({:.2}s) -> {}",
            demo.id,
            song.duration_secs,
            p.display()
        );
    }
    if do_play {
        println!(
            "Playing demo '{}' — {} ({})...",
            demo.id, demo.title_en, demo.title_ja
        );
        play_samples(sample_rate, &samples, normalize).unwrap_or_else(|e| die(&e));
    }
}

fn cmd_render(args: &[String]) {
    if args.is_empty() {
        die("usage: sound-hall render <input.mid|input.shs> -o <output.wav> [--sample-rate 44100] [--reverb 0.3] [--normalize]");
    }
    let input = PathBuf::from(&args[0]);
    let mut out_path: Option<PathBuf> = None;
    let mut sample_rate = 44100u32;
    let mut reverb = 0.30_f32;
    let mut normalize = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                i += 1;
                out_path = Some(PathBuf::from(require_arg(args, i, "--out")));
            }
            "--sample-rate" | "-r" => {
                i += 1;
                sample_rate = parse_u32_arg(args, i, "--sample-rate", 8000, 192_000);
            }
            "--reverb" => {
                i += 1;
                reverb = parse_f32_arg(args, i, "--reverb", 0.0, 1.0);
            }
            "--normalize" => normalize = true,
            other => die(&format!("unknown option for render: {other}")),
        }
        i += 1;
    }

    let out = out_path.unwrap_or_else(|| input.with_extension("wav"));
    let song = load_song(&input).unwrap_or_else(|e| die(&e));
    let mut engine = SoundHallEngine::new(sample_rate as f32);
    engine.set_reverb_mix(reverb);

    let t0 = Instant::now();
    let samples = MidiSequencer::render_to_buffer(&song, &mut engine, 1.8);
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    write_wav_file(&out, sample_rate, &samples, normalize).unwrap_or_else(|e| die(&e));

    println!(
        "Rendered {} ({} tracks, {:.2}s audio) -> {} in {:.1} ms",
        input.display(),
        song.track_names.len(),
        samples.len() as f64 / (2.0 * sample_rate as f64),
        out.display(),
        elapsed_ms
    );
}

fn cmd_play(args: &[String]) {
    if args.is_empty() {
        die("usage: sound-hall play <input.mid|input.shs> [--sample-rate 44100] [--reverb 0.3]");
    }
    let input = PathBuf::from(&args[0]);
    let mut sample_rate = 44100u32;
    let mut reverb = 0.30_f32;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--sample-rate" | "-r" => {
                i += 1;
                sample_rate = parse_u32_arg(args, i, "--sample-rate", 8000, 192_000);
            }
            "--reverb" => {
                i += 1;
                reverb = parse_f32_arg(args, i, "--reverb", 0.0, 1.0);
            }
            other => die(&format!("unknown option for play: {other}")),
        }
        i += 1;
    }

    let song = load_song(&input).unwrap_or_else(|e| die(&e));
    let mut engine = SoundHallEngine::new(sample_rate as f32);
    engine.set_reverb_mix(reverb);
    let samples = MidiSequencer::render_to_buffer(&song, &mut engine, 1.8);
    println!(
        "Playing {} ({:.2}s)...",
        input.display(),
        song.duration_secs
    );
    play_samples(sample_rate, &samples, true).unwrap_or_else(|e| die(&e));
}

fn cmd_benchmark(args: &[String]) {
    let mut voices = 32usize;
    let mut seconds = 10.0_f64;
    let mut sample_rate = 44100u32;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--voices" | "-v" => {
                i += 1;
                voices = parse_usize_arg(args, i, "--voices", 1, 256);
            }
            "--seconds" | "-s" => {
                i += 1;
                seconds = parse_f64_arg(args, i, "--seconds", 0.5, 120.0);
            }
            "--sample-rate" | "-r" => {
                i += 1;
                sample_rate = parse_u32_arg(args, i, "--sample-rate", 8000, 192_000);
            }
            other => die(&format!("unknown option for benchmark: {other}")),
        }
        i += 1;
    }

    let mut engine = SoundHallEngine::with_polyphony(sample_rate as f32, voices.max(64));
    // Trigger unique (channel, note) pairs across all 8 synthesis engines without note collisions.
    let benchmark_programs: [u8; 8] = [0, 12, 16, 24, 40, 56, 73, 81];
    let mut voice_specs = Vec::with_capacity(voices);
    for v in 0..voices {
        let slot = (v % 15) as u8;
        let ch = if slot >= 9 { slot + 1 } else { slot };
        let prog = benchmark_programs[v % benchmark_programs.len()];
        let note = (36 + ((v / 15) * 3) % 60) as u8;
        let decays = matches!(prog, 0 | 12 | 24);
        voice_specs.push((ch, prog, note, decays));
        engine.program_change(ch, prog);
        engine.note_on(ch, note, 96);
    }

    let total_frames = (seconds * sample_rate as f64).round() as usize;
    let retrigger_interval = (0.8 * sample_rate as f64).round() as usize;
    let mut next_retrigger = retrigger_interval;
    let block_frames = 256usize;
    let mut buf = vec![0.0_f32; block_frames * 2];

    let start = Instant::now();
    let mut rendered = 0usize;
    let mut min_active = engine.active_voice_count();
    let mut max_active = min_active;
    while rendered < total_frames {
        let n = (total_frames - rendered).min(block_frames);
        engine.render_stereo(&mut buf[..n * 2]);
        rendered += n;
        if rendered >= next_retrigger {
            for &(ch, prog, note, decays) in &voice_specs {
                if decays {
                    engine.program_change(ch, prog);
                    engine.note_on(ch, note, 96);
                }
            }
            next_retrigger += retrigger_interval;
        }
        let active = engine.active_voice_count();
        min_active = min_active.min(active);
        max_active = max_active.max(active);
    }
    let elapsed = start.elapsed().as_secs_f64();
    let speedup = seconds / elapsed.max(1e-9);
    let cpu_pct = (elapsed / seconds) * 100.0;
    let per_voice_cpu_pct = cpu_pct / voices as f64;

    println!("SoundHall DSP Benchmark");
    println!("  Sample rate       : {sample_rate} Hz (stereo)");
    println!(
        "  Simultaneous voices: {voices} (verified active {min_active}..={max_active}; mixed 8 engines + chorus + reverb)"
    );
    println!("  Rendered audio    : {seconds:.2} s ({total_frames} frames)");
    println!("  Elapsed time      : {:.2} ms", elapsed * 1000.0);
    println!("  Realtime speedup  : {speedup:.2}x faster than realtime");
    println!("  Total CPU load    : {cpu_pct:.2}% (~{per_voice_cpu_pct:.3}% per voice)");
}

fn content_type_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "wav" => "audio/wav",
        "mid" | "midi" => "audio/midi",
        "shs" | "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn cmd_serve(args: &[String]) {
    let mut port: u16 = 8080;
    let mut web_dir = PathBuf::from("web");

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                i += 1;
                port = parse_u32_arg(args, i, "--port", 1, 65535) as u16;
            }
            "--dir" | "-d" => {
                i += 1;
                web_dir = PathBuf::from(require_arg(args, i, "--dir"));
            }
            other => die(&format!("unknown option for serve: {other}")),
        }
        i += 1;
    }

    if !web_dir.is_dir() {
        die(&format!(
            "web directory '{}' not found (run from repository root or pass --dir <path>)",
            web_dir.display()
        ));
    }

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr)
        .unwrap_or_else(|e| die(&format!("failed to bind http://{addr}: {e}")));

    println!("SoundHall Visual Web Studio running at http://{addr}");
    println!("Serving directory: {}", web_dir.display());
    println!("Press Ctrl+C to stop.");

    for stream in listener.incoming().flatten() {
        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
            continue;
        }
        // Drain remaining request headers
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                break;
            }
        }

        let parts: Vec<&str> = request_line.split_whitespace().collect();
        let raw_path = parts.get(1).copied().unwrap_or("/");
        let clean_path = raw_path.split('?').next().unwrap_or("/");
        let rel_str = clean_path.trim_start_matches('/');
        let rel_path = if rel_str.is_empty() {
            Path::new("index.html")
        } else {
            Path::new(rel_str)
        };

        // Reject path traversal components
        if rel_path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            let _ = (&stream)
                .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 9\r\n\r\nForbidden");
            continue;
        }

        let full_path = web_dir.join(rel_path);
        match fs::read(&full_path) {
            Ok(body) => {
                let mime = content_type_for(&full_path);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = (&stream).write_all(header.as_bytes());
                let _ = (&stream).write_all(&body);
            }
            Err(_) => {
                let _ = (&stream)
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nNot Found");
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        return;
    }

    match args[0].as_str() {
        "-h" | "--help" | "help" => print_help(),
        "list" => cmd_list(&args[1..]),
        "info" => cmd_info(&args[1..]),
        "note" => cmd_note(&args[1..]),
        "chord" => cmd_chord(&args[1..]),
        "demo" => cmd_demo(&args[1..]),
        "render" => cmd_render(&args[1..]),
        "play" => cmd_play(&args[1..]),
        "benchmark" => cmd_benchmark(&args[1..]),
        "serve" => cmd_serve(&args[1..]),
        other => {
            eprintln!("Unknown command: {other}\n");
            print_help();
            process::exit(1);
        }
    }
}
