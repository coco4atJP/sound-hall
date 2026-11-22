//! SoundHall 和楽器 (Japanese traditional instrument) bank — MIDI Bank Select MSB 1.
//!
//! Complements the General MIDI Japanese voices (Shamisen #106, Koto #107, Shakuhachi #77,
//! Taiko #116) with gagaku, folk, theatre and temple instruments. Every voice is built from
//! the instrument's acoustics: sawari bridges buzz, membranes use their measured mode ratios,
//! gagaku winds glide into notes (embai), the shō swells on free reeds, the rin rings for
//! seconds with beating doublet modes.

use crate::dsp::envelope::AdsrParams;
use crate::dsp::filter::FormantPreset;
use crate::dsp::modal::ModeSpec;
use crate::dsp::waveguide::{WindType, WindVoicing};
use crate::synth::patch::{
    BowedParams, GmFamily, ModalParams, OrganParams, PluckedParams, PresetMetadata,
    SynthEngineKind, WindParams,
};

/// Bank Select MSB (CC 0) value of the 和楽器 bank.
pub const WAGAKKI_BANK: u8 = 1;

/// 長胴太鼓: thick cowhide on a long resonant barrel — a deep "don" with a warm body ring.
const NAGADO_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 1.0, 1.1),
    ModeSpec::new(1.52, 0.45, 0.55),
    ModeSpec::new(1.98, 0.30, 0.35),
    ModeSpec::new(2.44, 0.18, 0.22),
    ModeSpec::new(2.92, 0.12, 0.15),
    ModeSpec::new(3.60, 0.06, 0.08),
];

/// 締太鼓: rope-tensioned high-tension head — ideal-membrane mode ratios, dry "ten".
const SHIME_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 0.62, 0.16),
    ModeSpec::new(1.59, 0.37, 0.11),
    ModeSpec::new(2.14, 0.26, 0.08),
    ModeSpec::new(2.30, 0.19, 0.07),
    ModeSpec::new(2.65, 0.15, 0.06),
    ModeSpec::new(2.92, 0.12, 0.05),
    ModeSpec::new(3.16, 0.09, 0.04),
    ModeSpec::new(3.50, 0.06, 0.035),
];

/// 大太鼓: huge drum — very low fundamental with a long, rolling decay.
const ODAIKO_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 0.80, 2.4),
    ModeSpec::new(1.47, 0.44, 1.3),
    ModeSpec::new(1.90, 0.28, 0.8),
    ModeSpec::new(2.35, 0.18, 0.5),
    ModeSpec::new(2.80, 0.11, 0.32),
    ModeSpec::new(3.30, 0.06, 0.2),
];

/// 小鼓: hourglass drum whose cords and cavity give a pitched, singing "pon".
const KOTSUZUMI_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 1.0, 0.32),
    ModeSpec::new(1.50, 0.12, 0.08),
    ModeSpec::new(2.00, 0.35, 0.18),
    ModeSpec::new(2.98, 0.18, 0.10),
    ModeSpec::new(4.05, 0.08, 0.06),
];

/// 大鼓: oven-dried, tightly laced head struck with hard finger guards — a dry "kan!".
const OTSUZUMI_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 0.78, 0.045),
    ModeSpec::new(1.71, 0.55, 0.032),
    ModeSpec::new(2.43, 0.39, 0.025),
    ModeSpec::new(3.30, 0.27, 0.018),
    ModeSpec::new(4.40, 0.20, 0.012),
];

/// 当り鉦: small brass gong struck inside with a deer-horn beater — bright inharmonic "chan".
const ATARIGANE_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 0.68, 0.55),
    ModeSpec::new(1.62, 0.45, 0.42),
    ModeSpec::new(2.37, 0.52, 0.35),
    ModeSpec::new(3.21, 0.34, 0.26),
    ModeSpec::new(4.10, 0.22, 0.20),
    ModeSpec::new(5.30, 0.15, 0.14),
    ModeSpec::new(6.60, 0.09, 0.10),
];

/// 拍子木: two hardwood bars clapped together — free-free bar mode ratios, sharp crack.
const HYOSHIGI_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 1.0, 0.07),
    ModeSpec::new(2.76, 0.55, 0.035),
    ModeSpec::new(5.40, 0.30, 0.02),
    ModeSpec::new(8.93, 0.15, 0.012),
];

/// 木魚: carved hollow slit drum — hollow cavity "pok".
const MOKUGYO_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 1.0, 0.11),
    ModeSpec::new(1.55, 0.45, 0.07),
    ModeSpec::new(2.42, 0.25, 0.045),
    ModeSpec::new(3.60, 0.12, 0.03),
];

/// りん: Buddhist singing bowl. Each bowl mode is a slightly split doublet (the bowl is never
/// perfectly round), which produces the characteristic slow beating "wah-wah" of the ring.
const RIN_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 0.80, 9.0),
    ModeSpec::new(1.0012, 0.36, 8.0),
    ModeSpec::new(2.71, 0.45, 5.0),
    ModeSpec::new(2.7135, 0.20, 4.5),
    ModeSpec::new(5.12, 0.25, 2.6),
    ModeSpec::new(5.127, 0.11, 2.3),
    ModeSpec::new(8.25, 0.12, 1.4),
    ModeSpec::new(8.262, 0.05, 1.2),
    ModeSpec::new(12.1, 0.05, 0.7),
];

/// 鈴: a cluster of small pellet bells (kagura suzu) — detuned bells that jingle.
const SUZU_MODES: &[ModeSpec] = &[
    ModeSpec::new(1.0, 0.48, 0.35),
    ModeSpec::new(1.09, 0.42, 0.30),
    ModeSpec::new(1.17, 0.36, 0.28),
    ModeSpec::new(2.60, 0.24, 0.20),
    ModeSpec::new(2.83, 0.21, 0.18),
    ModeSpec::new(4.30, 0.12, 0.12),
];

const NO_FORMANTS: [(f32, f32, f32); 3] = [(0.0, 1.0, 0.0); 3];

#[allow(clippy::too_many_arguments)]
const fn plucked(
    pick_pos: f32,
    brightness: f32,
    rt60: f32,
    release_damping: f32,
    stiffness: f32,
    buzz: f32,
    body_specs: [(f32, f32, f32); 3],
    octave_double: bool,
) -> SynthEngineKind {
    SynthEngineKind::PluckedString(PluckedParams {
        pick_pos,
        brightness,
        rt60,
        release_damping,
        stiffness,
        buzz,
        body_specs,
        body_dry: 0.68,
        body_wet: 0.40,
        distortion: 0.0,
        palm_mute: false,
        octave_double,
        harmonic_bell: false,
    })
}

#[allow(clippy::too_many_arguments)]
const fn wind(
    wind_type: WindType,
    breath_noise: f32,
    brightness: f32,
    scoop_cents: f32,
    scoop_time: f32,
    formant: Option<FormantPreset>,
    formant_mix: f32,
    adsr: AdsrParams,
    vibrato_rate: f32,
    vibrato_depth_cents: f32,
    voicing: WindVoicing,
) -> SynthEngineKind {
    SynthEngineKind::WindBore(WindParams {
        wind_type,
        cylindrical_odd: false,
        breath_noise,
        brightness,
        scoop_cents,
        scoop_time,
        formant,
        formant_mix,
        adsr,
        vibrato_rate,
        vibrato_depth_cents,
        drone_fifth: false,
        voicing,
    })
}

const fn modal(
    modes: &'static [ModeSpec],
    mallet_hardness: f32,
    noise_amt: f32,
    release_time: f32,
    re_strike_delay: f32,
) -> SynthEngineKind {
    SynthEngineKind::ModalPercussion(ModalParams {
        modes,
        mallet_hardness,
        noise_amt,
        tremolo_hz: 0.0,
        tremolo_depth: 0.0,
        release_time,
        re_strike_delay,
    })
}

#[allow(clippy::too_many_arguments)]
const fn preset(
    program: u8,
    name_en: &'static str,
    name_ja: &'static str,
    family: GmFamily,
    gain: f32,
    default_reverb: f32,
    pitch_range: (u8, u8),
    patch: SynthEngineKind,
) -> PresetMetadata {
    PresetMetadata {
        program,
        bank: WAGAKKI_BANK,
        name_en,
        name_ja,
        family,
        gain,
        default_reverb,
        default_chorus: 0.0,
        pitch_range,
        patch,
    }
}

/// Number of instruments in the 和楽器 bank.
pub const WAGAKKI_COUNT: usize = 23;

/// The 和楽器 bank (Bank Select MSB 1, programs `0..WAGAKKI_COUNT`).
pub static WAGAKKI_PRESETS: [PresetMetadata; WAGAKKI_COUNT] = [
    // ---- 弦楽器 (strings) ----
    // 0: Satsuma-biwa — big bachi near the bridge, high frets: strong sawari buzz
    preset(
        0,
        "Biwa",
        "琵琶",
        GmFamily::Ethnic,
        5.29,
        0.38,
        (40, 76),
        plucked(
            0.09,
            0.86,
            2.0,
            0.6,
            0.22,
            0.78,
            [(180.0, 2.8, 0.30), (430.0, 3.2, 0.32), (1150.0, 2.6, 0.18)],
            false,
        ),
    ),
    // 1: Jushichigen — 17-string bass koto, large body, long ring
    preset(
        1,
        "Jushichigen (Bass Koto)",
        "十七絃",
        GmFamily::Ethnic,
        4.48,
        0.40,
        (31, 67),
        plucked(
            0.22,
            0.58,
            4.2,
            2.0,
            0.12,
            0.03,
            [(95.0, 2.4, 0.32), (230.0, 2.8, 0.32), (650.0, 2.2, 0.16)],
            false,
        ),
    ),
    // 2: Tsugaru-jamisen — thick neck, the bachi slaps the skin, aggressive sawari
    preset(
        2,
        "Tsugaru Shamisen",
        "津軽三味線",
        GmFamily::Ethnic,
        5.76,
        0.28,
        (45, 84),
        plucked(
            0.08,
            0.96,
            1.0,
            0.25,
            0.16,
            0.62,
            [(360.0, 3.0, 0.32), (980.0, 3.4, 0.36), (2700.0, 3.0, 0.24)],
            false,
        ),
    ),
    // 3: Taishogoto — keyed zither with steel melody strings in octaves
    preset(
        3,
        "Taishogoto",
        "大正琴",
        GmFamily::Ethnic,
        5.09,
        0.34,
        (55, 91),
        plucked(
            0.12,
            0.92,
            2.4,
            0.45,
            0.38,
            0.12,
            [(260.0, 3.0, 0.28), (820.0, 3.2, 0.30), (2400.0, 3.0, 0.22)],
            true,
        ),
    ),
    // 4: Gakuso — the gagaku koto: soft tsume, darker and more resonant than sokyoku koto
    preset(
        4,
        "Gakuso (Gagaku Koto)",
        "楽箏",
        GmFamily::Ethnic,
        4.59,
        0.42,
        (43, 81),
        plucked(
            0.24,
            0.60,
            3.0,
            1.6,
            0.15,
            0.02,
            [(150.0, 2.6, 0.30), (390.0, 3.0, 0.30), (1100.0, 2.4, 0.16)],
            false,
        ),
    ),
    // 5: Kokyu — bowed shamisen-family fiddle: small skin body, wide expressive vibrato
    preset(
        5,
        "Kokyu",
        "胡弓",
        GmFamily::Strings,
        5.47,
        0.40,
        (55, 88),
        SynthEngineKind::BowedString(BowedParams {
            bow_pressure: 0.60,
            brightness: 0.74,
            rosin_noise: 0.11,
            ensemble_detune: 0.0,
            body_scale: 1.15,
            adsr: AdsrParams::new(0.09, 0.16, 0.86, 0.28),
            vibrato_rate: 4.8,
            vibrato_depth_cents: 22.0,
            tremolo_hz: 0.0,
        }),
    ),
    // ---- 管楽器 (winds) ----
    // 6: Shinobue — festival bamboo flute, bright and breathy
    preset(
        6,
        "Shinobue",
        "篠笛",
        GmFamily::Pipe,
        3.28,
        0.40,
        (67, 98),
        wind(
            WindType::FluteJet,
            0.30,
            0.82,
            -30.0,
            0.06,
            None,
            0.0,
            AdsrParams::new(0.03, 0.12, 0.90, 0.12),
            5.6,
            10.0,
            WindVoicing::new(1500.0, 0.45, 0.0, NO_FORMANTS),
        ),
    ),
    // 7: Ryuteki — gagaku dragon flute: strong tone, notes glide up from below (embai)
    preset(
        7,
        "Ryuteki",
        "龍笛",
        GmFamily::Pipe,
        2.81,
        0.46,
        (64, 93),
        wind(
            WindType::FluteJet,
            0.34,
            0.88,
            -90.0,
            0.16,
            None,
            0.0,
            AdsrParams::new(0.05, 0.15, 0.92, 0.16),
            4.6,
            6.0,
            WindVoicing::new(2000.0, 0.5, 0.0, NO_FORMANTS),
        ),
    ),
    // 8: Nokan — Noh flute: shrill, airy, piercing upper register
    preset(
        8,
        "Nokan",
        "能管",
        GmFamily::Pipe,
        2.45,
        0.42,
        (67, 100),
        wind(
            WindType::FluteJet,
            0.45,
            0.96,
            -60.0,
            0.10,
            None,
            0.0,
            AdsrParams::new(0.025, 0.10, 0.90, 0.10),
            5.0,
            4.0,
            WindVoicing::new(
                2800.0,
                0.6,
                0.0,
                [(3100.0, 2.0, 5.0), (0.0, 1.0, 0.0), (0.0, 1.0, 0.0)],
            ),
        ),
    ),
    // 9: Hichiriki — gagaku double reed: loud, nasal, wide pitch glides (embai)
    preset(
        9,
        "Hichiriki",
        "篳篥",
        GmFamily::Reed,
        2.67,
        0.44,
        (62, 84),
        wind(
            WindType::DoubleReed,
            0.12,
            0.95,
            -140.0,
            0.22,
            Some(FormantPreset::Nasal),
            0.25,
            AdsrParams::new(0.06, 0.15, 0.92, 0.14),
            4.2,
            8.0,
            WindVoicing::new(
                2600.0,
                0.5,
                350.0,
                [(1050.0, 2.2, 10.0), (2600.0, 2.8, 7.0), (4000.0, 3.0, 3.0)],
            ),
        ),
    ),
    // 10: Sho — gagaku free-reed mouth organ: bright reedy partials, slow breath swell
    preset(
        10,
        "Sho",
        "笙",
        GmFamily::Organ,
        2.28,
        0.50,
        (64, 93),
        SynthEngineKind::AdditiveOrgan(OrganParams {
            drawbars: [0.0, 0.0, 1.0, 0.60, 0.45, 0.35, 0.28, 0.22, 0.15],
            key_click: 0.0,
            percussion_level: 0.0,
            percussion_harmonic: 2.0,
            rotary_speed: 0.0,
            rotary_depth: 0.0,
            overdrive: 0.05,
            breathiness: 0.05,
            adsr: AdsrParams::new(0.5, 0.3, 1.0, 0.7),
        }),
    ),
    // 11: Horagai — conch trumpet blown like brass: rough, swooping, few pitches
    preset(
        11,
        "Horagai (Conch)",
        "法螺貝",
        GmFamily::Brass,
        3.08,
        0.46,
        (43, 67),
        wind(
            WindType::LipReed,
            0.24,
            0.60,
            -220.0,
            0.35,
            None,
            0.0,
            AdsrParams::new(0.12, 0.30, 0.85, 0.35),
            0.0,
            0.0,
            WindVoicing::new(
                650.0,
                0.9,
                120.0,
                [(380.0, 2.0, 7.0), (950.0, 2.5, 5.0), (2200.0, 3.0, 2.0)],
            ),
        ),
    ),
    // 12: Shakuhachi with heavy muraiki (explosive breath) and deep meri vibrato
    preset(
        12,
        "Shakuhachi (Muraiki)",
        "尺八 (ムラ息)",
        GmFamily::Pipe,
        3.27,
        0.46,
        (50, 84),
        wind(
            WindType::FluteJet,
            0.78,
            0.62,
            -45.0,
            0.12,
            None,
            0.0,
            AdsrParams::new(0.08, 0.20, 0.86, 0.22),
            4.4,
            26.0,
            WindVoicing::new(850.0, 0.55, 0.0, NO_FORMANTS),
        ),
    ),
    // ---- 打楽器 (percussion) ----
    // 13: Nagado-daiko — the main festival taiko
    preset(
        13,
        "Nagado-daiko",
        "長胴太鼓",
        GmFamily::Percussive,
        2.50,
        0.34,
        (24, 55),
        modal(NAGADO_MODES, 0.50, 0.22, 1.2, 0.0),
    ),
    // 14: Shime-daiko — small high-tension drum
    preset(
        14,
        "Shime-daiko",
        "締太鼓",
        GmFamily::Percussive,
        4.09,
        0.28,
        (48, 76),
        modal(SHIME_MODES, 0.95, 0.35, 0.25, 0.0),
    ),
    // 15: O-daiko — giant drum
    preset(
        15,
        "O-daiko",
        "大太鼓",
        GmFamily::Percussive,
        3.12,
        0.40,
        (24, 48),
        modal(ODAIKO_MODES, 0.35, 0.18, 2.5, 0.0),
    ),
    // 16: Kotsuzumi — shoulder drum of Noh and kabuki
    preset(
        16,
        "Kotsuzumi",
        "小鼓",
        GmFamily::Percussive,
        2.53,
        0.36,
        (55, 79),
        modal(KOTSUZUMI_MODES, 0.55, 0.30, 0.40, 0.0),
    ),
    // 17: Otsuzumi — hip drum, dry and cutting
    preset(
        17,
        "Otsuzumi",
        "大鼓",
        GmFamily::Percussive,
        4.24,
        0.30,
        (60, 84),
        modal(OTSUZUMI_MODES, 1.0, 0.65, 0.10, 0.0),
    ),
    // 18: Atarigane (chanchiki) — festival hand gong
    preset(
        18,
        "Atarigane",
        "当り鉦",
        GmFamily::ChromaticPercussion,
        2.83,
        0.30,
        (72, 96),
        modal(ATARIGANE_MODES, 1.0, 0.18, 0.5, 0.0),
    ),
    // 19: Hyoshigi — kabuki / night-watch clappers
    preset(
        19,
        "Hyoshigi",
        "拍子木",
        GmFamily::Percussive,
        2.76,
        0.32,
        (72, 96),
        modal(HYOSHIGI_MODES, 1.0, 0.40, 0.12, 0.0),
    ),
    // 20: Mokugyo — temple wooden fish
    preset(
        20,
        "Mokugyo",
        "木魚",
        GmFamily::Percussive,
        3.13,
        0.34,
        (48, 79),
        modal(MOKUGYO_MODES, 0.70, 0.30, 0.15, 0.0),
    ),
    // 21: Rin — singing bowl, rings for many seconds after the note
    preset(
        21,
        "Rin (Singing Bowl)",
        "りん",
        GmFamily::ChromaticPercussion,
        1.92,
        0.50,
        (60, 96),
        modal(RIN_MODES, 0.60, 0.04, 6.0, 0.0),
    ),
    // 22: Suzu — kagura bell tree, jingling cluster
    preset(
        22,
        "Suzu (Shrine Bells)",
        "鈴",
        GmFamily::ChromaticPercussion,
        2.68,
        0.38,
        (72, 100),
        modal(SUZU_MODES, 1.0, 0.40, 0.40, 0.045),
    ),
];
