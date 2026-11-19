//! Zero-dependency WebAssembly C-ABI bindings for `sound-hall-core`.

use std::alloc::{alloc, dealloc, Layout};
use std::slice;

use sound_hall_core::{all_drums, all_presets, SoundHallEngine};

/// Allocates `len` bytes in WASM linear memory for host-side audio buffers.
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

/// Instantiates a new `SoundHallEngine` on the heap and returns its opaque pointer.
#[no_mangle]
pub extern "C" fn sh_engine_new(sample_rate: f32, max_voices: u32) -> *mut SoundHallEngine {
    let voices = if max_voices == 0 {
        64
    } else {
        max_voices as usize
    };
    let engine = SoundHallEngine::with_polyphony(sample_rate, voices);
    Box::into_raw(Box::new(engine))
}

/// Destroys a `SoundHallEngine` instance created with `sh_engine_new`.
///
/// # Safety
/// `ptr` must be a valid non-null pointer from `sh_engine_new` or null.
#[no_mangle]
pub unsafe extern "C" fn sh_engine_free(ptr: *mut SoundHallEngine) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

/// Triggers MIDI Note On (`velocity == 0` acts as Note Off).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_note_on(ptr: *mut SoundHallEngine, ch: u8, note: u8, vel: u8) {
    if let Some(engine) = ptr.as_mut() {
        engine.note_on(ch, note, vel);
    }
}

/// Triggers MIDI Note Off.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_note_off(ptr: *mut SoundHallEngine, ch: u8, note: u8) {
    if let Some(engine) = ptr.as_mut() {
        engine.note_off(ch, note);
    }
}

/// Changes General MIDI program (`0..=127`) on channel `0..=15`.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_program_change(ptr: *mut SoundHallEngine, ch: u8, program: u8) {
    if let Some(engine) = ptr.as_mut() {
        engine.program_change(ch, program);
    }
}

/// Sends a MIDI Control Change message.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_control_change(ptr: *mut SoundHallEngine, ch: u8, cc: u8, value: u8) {
    if let Some(engine) = ptr.as_mut() {
        engine.control_change(ch, cc, value);
    }
}

/// Sets 14-bit Pitch Bend (`8192` = center).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_pitch_bend(ptr: *mut SoundHallEngine, ch: u8, value_14bit: u16) {
    if let Some(engine) = ptr.as_mut() {
        engine.pitch_bend(ch, value_14bit);
    }
}

/// Releases all active notes gracefully.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_all_notes_off(ptr: *mut SoundHallEngine) {
    if let Some(engine) = ptr.as_mut() {
        engine.all_notes_off();
    }
}

/// Resets channels and kills all sounding voices immediately.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_reset(ptr: *mut SoundHallEngine) {
    if let Some(engine) = ptr.as_mut() {
        engine.reset();
    }
}

/// Sets master output gain (default `0.85`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_set_master_gain(ptr: *mut SoundHallEngine, gain: f32) {
    if let Some(engine) = ptr.as_mut() {
        engine.set_master_gain(gain);
    }
}

/// Sets global hall reverb wet mix (`0.0..=1.0`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_set_reverb_mix(ptr: *mut SoundHallEngine, mix: f32) {
    if let Some(engine) = ptr.as_mut() {
        engine.set_reverb_mix(mix);
    }
}

/// Sets global stereo chorus wet mix (`0.0..=1.0`).
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_set_chorus_mix(ptr: *mut SoundHallEngine, mix: f32) {
    if let Some(engine) = ptr.as_mut() {
        engine.set_chorus_mix(mix);
    }
}

/// Renders `frames` interleaved stereo f32 frames (`frames * 2` floats) to `out_ptr`.
///
/// # Safety
/// `out_ptr` must be valid, 4-byte aligned, and writable for `frames * 2` contiguous `f32` elements.
#[no_mangle]
pub unsafe extern "C" fn sh_render_stereo(
    ptr: *mut SoundHallEngine,
    out_ptr: *mut f32,
    frames: usize,
) {
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
    if let Some(engine) = ptr.as_mut() {
        let out = slice::from_raw_parts_mut(out_ptr, sample_count);
        engine.render_stereo(out);
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
    ptr: *mut SoundHallEngine,
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
    if let Some(engine) = ptr.as_mut() {
        let left = slice::from_raw_parts_mut(left_ptr, frames);
        let right = slice::from_raw_parts_mut(right_ptr, frames);
        engine.render_planar(left, right);
    }
}

/// Returns the number of currently active sounding voices.
///
/// # Safety
/// `ptr` must be a valid pointer from `sh_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn sh_active_voices(ptr: *const SoundHallEngine) -> u32 {
    ptr.as_ref()
        .map(|e| e.active_voice_count() as u32)
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
            sh_note_on(engine, 0, 60, 100);
            assert_eq!(sh_active_voices(engine), 1);

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
            sh_all_notes_off(engine);
            sh_reset(engine);
            assert_eq!(sh_active_voices(engine), 0);
            sh_engine_free(engine);
            sh_engine_free(core::ptr::null_mut());
        }
    }
}
