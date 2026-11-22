pub mod bass_strings;
pub mod brass_reed_pipe;
pub mod drums;
pub mod organ_guitar;
pub mod piano_chromatic;
pub mod synth_ethnic_sfx;
pub mod wagakki;

use crate::synth::drum::DrumPatch;
use crate::synth::patch::PresetMetadata;

use bass_strings::PRESETS_32_55;
use brass_reed_pipe::PRESETS_56_79;
use drums::GM_DRUMS;
use organ_guitar::PRESETS_16_31;
use piano_chromatic::PRESETS_0_15;
use synth_ethnic_sfx::PRESETS_80_127;
use wagakki::WAGAKKI_PRESETS;
pub use wagakki::{WAGAKKI_BANK, WAGAKKI_COUNT};

const fn build_all_presets() -> [PresetMetadata; 128] {
    // Placeholder element replaced cleanly via loop in const context
    let mut table: [PresetMetadata; 128] = [PRESETS_0_15[0]; 128];
    let mut i = 0;
    while i < 16 {
        table[i] = PRESETS_0_15[i];
        i += 1;
    }
    let mut i = 0;
    while i < 16 {
        table[16 + i] = PRESETS_16_31[i];
        i += 1;
    }
    let mut i = 0;
    while i < 24 {
        table[32 + i] = PRESETS_32_55[i];
        i += 1;
    }
    let mut i = 0;
    while i < 24 {
        table[56 + i] = PRESETS_56_79[i];
        i += 1;
    }
    let mut i = 0;
    while i < 48 {
        table[80 + i] = PRESETS_80_127[i];
        i += 1;
    }
    table
}

/// Complete array of all 128 General MIDI melodic presets (programs 0..=127).
pub static ALL_PRESETS: [PresetMetadata; 128] = build_all_presets();

/// Returns all 128 General MIDI melodic presets.
pub fn all_presets() -> &'static [PresetMetadata; 128] {
    &ALL_PRESETS
}

/// Looks up a preset by General MIDI program number (`0..=127`).
pub fn get_preset(program: u8) -> &'static PresetMetadata {
    &ALL_PRESETS[(program & 0x7F) as usize]
}

/// Returns the 和楽器 (Japanese traditional instrument) bank — Bank Select MSB 1.
pub fn wagakki_presets() -> &'static [PresetMetadata; WAGAKKI_COUNT] {
    &WAGAKKI_PRESETS
}

/// Returns the presets of bank `bank` (0 = General MIDI, 1 = 和楽器; empty otherwise).
pub fn bank_presets(bank: u8) -> &'static [PresetMetadata] {
    match bank {
        0 => &ALL_PRESETS,
        WAGAKKI_BANK => &WAGAKKI_PRESETS,
        _ => &[],
    }
}

/// Every preset of every bank: the 128 General MIDI programs followed by the 和楽器 bank.
pub fn all_bank_presets() -> impl Iterator<Item = &'static PresetMetadata> {
    ALL_PRESETS.iter().chain(WAGAKKI_PRESETS.iter())
}

/// Bank-aware lookup (Bank Select MSB + Program Change). Programs that do not exist in the
/// selected bank fall back to the General MIDI program, like GS/XG sound modules do.
pub fn get_bank_preset(bank: u8, program: u8) -> &'static PresetMetadata {
    bank_presets(bank)
        .get((program & 0x7F) as usize)
        .unwrap_or_else(|| get_preset(program))
}

/// Searches for a single preset matching either a GM program number (0..127), English name,
/// or Japanese name across all banks (exact match preferred, General MIDI first, otherwise
/// first substring match).
pub fn find_preset(query: &str) -> Option<&'static PresetMetadata> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    if let Ok(num) = q.parse::<u8>() {
        if num <= 127 {
            return Some(get_preset(num));
        }
    }
    let q_lower = q.to_lowercase();
    if let Some(exact) =
        all_bank_presets().find(|p| p.name_en.to_lowercase() == q_lower || p.name_ja == q)
    {
        return Some(exact);
    }
    all_bank_presets()
        .find(|p| p.name_en.to_lowercase().contains(&q_lower) || p.name_ja.contains(q))
}

/// Returns all presets (all banks) whose English/Japanese name or family matches `query`.
/// `"wagakki"` / `"和楽器"` lists the whole 和楽器 bank.
pub fn search_presets(query: &str) -> Vec<&'static PresetMetadata> {
    let q = query.trim();
    let q_lower = q.to_lowercase();
    let bank_match = |p: &PresetMetadata| {
        p.bank == WAGAKKI_BANK && (q_lower.contains("wagakki") || q.contains("和楽器"))
    };
    all_bank_presets()
        .filter(|p| {
            bank_match(p)
                || p.name_en.to_lowercase().contains(&q_lower)
                || p.name_ja.contains(q)
                || p.family.name_en().to_lowercase().contains(&q_lower)
                || p.family.name_ja().contains(q)
        })
        .collect()
}

/// Returns all 47 General MIDI Standard Drum Kit definitions (MIDI notes 35..=81).
pub fn all_drums() -> &'static [DrumPatch; 47] {
    &GM_DRUMS
}

/// Looks up a GM drum instrument by MIDI note (`35..=81`).
/// Notes outside `35..=81` wrap gracefully to a valid drum patch so percussion never drops silently.
pub fn get_drum_patch(note: u8) -> &'static DrumPatch {
    if (35..=81).contains(&note) {
        &GM_DRUMS[(note - 35) as usize]
    } else {
        // Fallback mapping for non-standard drum notes
        let idx = (note as usize) % GM_DRUMS.len();
        &GM_DRUMS[idx]
    }
}

/// Looks up exact drum metadata if `note` is within `35..=81`.
pub fn get_drum_info(note: u8) -> Option<&'static DrumPatch> {
    if (35..=81).contains(&note) {
        Some(&GM_DRUMS[(note - 35) as usize])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bank_lookup_and_names() {
        assert_eq!(get_bank_preset(0, 107).name_en, "Koto");
        assert_eq!(get_bank_preset(WAGAKKI_BANK, 0).name_ja, "琵琶");
        // Unknown program in a bank and unknown banks fall back to General MIDI
        assert_eq!(get_bank_preset(WAGAKKI_BANK, 120).program, 120);
        assert_eq!(get_bank_preset(9, 3).bank, 0);
        // GM names win exact matches; 和楽器 names are found by Japanese and English name
        assert_eq!(find_preset("koto").unwrap().bank, 0);
        assert_eq!(find_preset("篳篥").unwrap().bank, WAGAKKI_BANK);
        assert_eq!(find_preset("shinobue").unwrap().name_ja, "篠笛");
        assert_eq!(search_presets("和楽器").len(), WAGAKKI_COUNT);
    }

    #[test]
    fn preset_table_indices_are_contiguous() {
        for (i, p) in ALL_PRESETS.iter().enumerate() {
            assert_eq!(p.program as usize, i);
            assert!(!p.name_en.is_empty());
            assert!(!p.name_ja.is_empty());
        }
        for (i, p) in WAGAKKI_PRESETS.iter().enumerate() {
            assert_eq!(p.program as usize, i);
            assert_eq!(p.bank, WAGAKKI_BANK);
            assert!(!p.name_en.is_empty() && !p.name_ja.is_empty());
        }
        for p in ALL_PRESETS.iter() {
            assert_eq!(p.bank, 0);
        }
        for (i, d) in GM_DRUMS.iter().enumerate() {
            assert_eq!(d.note as usize, 35 + i);
            assert!(!d.name_en.is_empty());
            assert!(!d.name_ja.is_empty());
        }
    }
}
