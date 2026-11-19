/**
 * Zero-dependency ESM wrapper for loading `sound_hall_wasm.wasm`
 * in browsers (offline render or AudioWorklet) and Node.js.
 */
export class SoundHallWasm {
  constructor(instance, sampleRate = 44100, maxVoices = 64) {
    this.exports = instance.exports;
    this.sampleRate = sampleRate;
    this.ptr = this.exports.sh_engine_new(sampleRate, maxVoices);
    this._planarFrames = 128;
    this._leftPtr = this.exports.sh_alloc(this._planarFrames * 4);
    this._rightPtr = this.exports.sh_alloc(this._planarFrames * 4);
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
