/**
 * Zero-dependency ESM wrapper for loading `sound_hall_wasm.wasm`
 * in browsers (AudioContext / AudioWorklet) and Node.js.
 */
export class SoundHallWasm {
  constructor(instance, sampleRate = 44100, maxVoices = 64) {
    this.exports = instance.exports;
    this.sampleRate = sampleRate;
    this.ptr = this.exports.sh_engine_new(sampleRate, maxVoices);
    this._planarFrames = 256;
    this._leftPtr = this.exports.sh_alloc(this._planarFrames * 4);
    this._rightPtr = this.exports.sh_alloc(this._planarFrames * 4);
    this._encoder = new TextEncoder();
    this._decoder = new TextDecoder();
    if (!this.ptr || !this._leftPtr || !this._rightPtr) {
      if (this._leftPtr) this.exports.sh_free(this._leftPtr, this._planarFrames * 4);
      if (this._rightPtr) this.exports.sh_free(this._rightPtr, this._planarFrames * 4);
      if (this.ptr) this.exports.sh_engine_free(this.ptr);
      this.ptr = 0;
      this._leftPtr = 0;
      this._rightPtr = 0;
      this._planarFrames = 0;
      throw new Error("SoundHallWasm: failed to allocate WASM engine buffers");
    }
  }

  static async instantiate(wasmBytes, sampleRate = 44100, maxVoices = 64) {
    const { instance } = await WebAssembly.instantiate(wasmBytes, {});
    return new SoundHallWasm(instance, sampleRate, maxVoices);
  }

  static async loadUrl(url, sampleRate = 44100, maxVoices = 64) {
    const res = await fetch(url);
    if (!res.ok) {
      throw new Error(`Failed to load WASM from ${url}: HTTP ${res.status} ${res.statusText}`);
    }
    const bytes = await res.arrayBuffer();
    return SoundHallWasm.instantiate(bytes, sampleRate, maxVoices);
  }

  noteOn(channel, note, velocity = 100) {
    if (!this.ptr) return;
    this.exports.sh_note_on(this.ptr, channel & 0x0f, note & 0x7f, velocity & 0x7f);
  }

  noteOff(channel, note) {
    if (!this.ptr) return;
    this.exports.sh_note_off(this.ptr, channel & 0x0f, note & 0x7f);
  }

  programChange(channel, program) {
    if (!this.ptr) return;
    this.exports.sh_program_change(this.ptr, channel & 0x0f, program & 0x7f);
  }

  controlChange(channel, cc, value) {
    if (!this.ptr) return;
    this.exports.sh_control_change(this.ptr, channel & 0x0f, cc & 0x7f, value & 0x7f);
  }

  pitchBend(channel, value14Bit = 8192) {
    if (!this.ptr) return;
    this.exports.sh_pitch_bend(this.ptr, channel & 0x0f, value14Bit & 0x3fff);
  }

  allNotesOff() {
    if (!this.ptr) return;
    this.exports.sh_all_notes_off(this.ptr);
  }

  reset() {
    if (!this.ptr) return;
    this.exports.sh_reset(this.ptr);
  }

  setMasterGain(gain) {
    if (!this.ptr) return;
    this.exports.sh_set_master_gain(this.ptr, gain);
  }

  setReverbMix(mix) {
    if (!this.ptr) return;
    this.exports.sh_set_reverb_mix(this.ptr, mix);
  }

  setChorusMix(mix) {
    if (!this.ptr) return;
    this.exports.sh_set_chorus_mix(this.ptr, mix);
  }

  activeVoices() {
    if (!this.ptr) return 0;
    return this.exports.sh_active_voices(this.ptr);
  }

  loadDemo(demoIndex) {
    if (!this.ptr) return -1;
    return this.exports.sh_load_demo(this.ptr, demoIndex >>> 0);
  }

  loadScore(scoreText) {
    if (!this.ptr) return -1;
    const bytes = this._encoder.encode(scoreText);
    if (bytes.length === 0) return -1;
    const bufPtr = this.exports.sh_alloc(bytes.length);
    if (!bufPtr) throw new Error("SoundHallWasm: score buffer allocation failed");
    try {
      new Uint8Array(this.exports.memory.buffer, bufPtr, bytes.length).set(bytes);
      return this.exports.sh_load_score(this.ptr, bufPtr, bytes.length);
    } finally {
      this.exports.sh_free(bufPtr, bytes.length);
    }
  }

  loadSmf(midiUint8Array) {
    if (!this.ptr || !midiUint8Array || midiUint8Array.length === 0) return -1;
    const len = midiUint8Array.length;
    const bufPtr = this.exports.sh_alloc(len);
    if (!bufPtr) throw new Error("SoundHallWasm: MIDI buffer allocation failed");
    try {
      new Uint8Array(this.exports.memory.buffer, bufPtr, len).set(midiUint8Array);
      return this.exports.sh_load_smf(this.ptr, bufPtr, len);
    } finally {
      this.exports.sh_free(bufPtr, len);
    }
  }

  lastError() {
    if (!this.ptr) return "";
    const len = this.exports.sh_last_error_len(this.ptr);
    if (!len) return "";
    const ptr = this.exports.sh_last_error_ptr(this.ptr);
    return this._decoder.decode(new Uint8Array(this.exports.memory.buffer, ptr, len));
  }

  seqPlay() {
    if (!this.ptr) return;
    this.exports.sh_seq_play(this.ptr);
  }

  seqPause() {
    if (!this.ptr) return;
    this.exports.sh_seq_pause(this.ptr);
  }

  seqStop() {
    if (!this.ptr) return;
    this.exports.sh_seq_stop(this.ptr);
  }

  seqSeek(timeSec) {
    if (!this.ptr) return;
    this.exports.sh_seq_seek(this.ptr, timeSec);
  }

  seqIsPlaying() {
    if (!this.ptr) return false;
    return this.exports.sh_seq_is_playing(this.ptr) !== 0;
  }

  seqPositionSec() {
    if (!this.ptr) return 0;
    return this.exports.sh_seq_position_sec(this.ptr);
  }

  seqDurationSec() {
    if (!this.ptr) return 0;
    return this.exports.sh_seq_duration_sec(this.ptr);
  }

  activeNotesLow() {
    if (!this.ptr) return 0n;
    return BigInt.asUintN(64, this.exports.sh_active_notes_low(this.ptr));
  }

  activeNotesHigh() {
    if (!this.ptr) return 0n;
    return BigInt.asUintN(64, this.exports.sh_active_notes_high(this.ptr));
  }

  isNoteActive(note) {
    if (!this.ptr || note < 0 || note > 127) return false;
    if (note < 64) {
      return (this.activeNotesLow() & (1n << BigInt(note))) !== 0n;
    }
    return (this.activeNotesHigh() & (1n << BigInt(note - 64))) !== 0n;
  }

  channelProgram(ch) {
    if (!this.ptr) return 0;
    return this.exports.sh_channel_program(this.ptr, ch & 0x0f);
  }

  channelActivity(ch) {
    if (!this.ptr) return 0;
    return this.exports.sh_channel_activity(this.ptr, ch & 0x0f);
  }

  channelLastNote(ch) {
    if (!this.ptr) return 60;
    return this.exports.sh_channel_last_note(this.ptr, ch & 0x0f);
  }

  exportSongWav(normalize = true) {
    if (!this.ptr) return null;
    const len = this.exports.sh_render_song_wav(this.ptr, normalize ? 1 : 0);
    if (!len) return null;
    const ptr = this.exports.sh_wav_ptr(this.ptr);
    return new Uint8Array(this.exports.memory.buffer, ptr, len).slice();
  }

  renderPlanar(leftOut, rightOut) {
    if (!this.ptr) return;
    const frames = Math.min(leftOut.length, rightOut.length);
    if (frames <= 0) return;

    if (frames > this._planarFrames) {
      const nextLeft = this.exports.sh_alloc(frames * 4);
      const nextRight = this.exports.sh_alloc(frames * 4);
      if (!nextLeft || !nextRight) {
        if (nextLeft) this.exports.sh_free(nextLeft, frames * 4);
        if (nextRight) this.exports.sh_free(nextRight, frames * 4);
        throw new Error("SoundHallWasm: failed to grow planar audio buffers");
      }
      this.exports.sh_free(this._leftPtr, this._planarFrames * 4);
      this.exports.sh_free(this._rightPtr, this._planarFrames * 4);
      this._planarFrames = frames;
      this._leftPtr = nextLeft;
      this._rightPtr = nextRight;
    }

    this.exports.sh_render_planar(this.ptr, this._leftPtr, this._rightPtr, frames);
    const mem = new Float32Array(this.exports.memory.buffer);
    const lOff = this._leftPtr >>> 2;
    const rOff = this._rightPtr >>> 2;
    leftOut.set(mem.subarray(lOff, lOff + frames));
    rightOut.set(mem.subarray(rOff, rOff + frames));
  }

  dispose() {
    if (this.ptr) {
      if (this._leftPtr) this.exports.sh_free(this._leftPtr, this._planarFrames * 4);
      if (this._rightPtr) this.exports.sh_free(this._rightPtr, this._planarFrames * 4);
      this.exports.sh_engine_free(this.ptr);
      this.ptr = 0;
      this._leftPtr = 0;
      this._rightPtr = 0;
      this._planarFrames = 0;
    }
  }
}
