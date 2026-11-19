import { SoundHallWasm } from "./sound-hall.js";

/**
 * Web Audio API AudioWorkletProcessor driven by `sound_hall_wasm.wasm`.
 *
 * Usage on main thread:
 *   const wasmBytes = await (await fetch("sound_hall_wasm.wasm")).arrayBuffer();
 *   await audioCtx.audioWorklet.addModule("worklet-processor.js");
 *   const node = new AudioWorkletNode(audioCtx, "sound-hall-processor", {
 *     processorOptions: { wasmBytes },
 *     outputChannelCount: [2],
 *   });
 */
class SoundHallWorkletProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.synth = null;
    this.pendingEvents = [];
    this._monoScratch = new Float32Array(128);

    const wasmBytes = options?.processorOptions?.wasmBytes;
    if (wasmBytes) {
      SoundHallWasm.instantiate(wasmBytes, sampleRate, 64)
        .then((engine) => {
          this.synth = engine;
          for (const msg of this.pendingEvents) {
            this.handleMidiMessage(msg);
          }
          this.pendingEvents.length = 0;
        })
        .catch((err) => {
          this.port.postMessage({ type: "error", message: String(err) });
        });
    }

    this.port.onmessage = (ev) => {
      if (!this.synth) {
        this.pendingEvents.push(ev.data);
      } else {
        this.handleMidiMessage(ev.data);
      }
    };
  }

  handleMidiMessage(msg) {
    if (!msg || !this.synth) return;
    switch (msg.type) {
      case "noteOn":
        this.synth.noteOn(msg.channel ?? 0, msg.note, msg.velocity ?? 100);
        break;
      case "noteOff":
        this.synth.noteOff(msg.channel ?? 0, msg.note);
        break;
      case "programChange":
        this.synth.programChange(msg.channel ?? 0, msg.program);
        break;
      case "controlChange":
        this.synth.controlChange(msg.channel ?? 0, msg.cc, msg.value);
        break;
      case "pitchBend":
        this.synth.pitchBend(msg.channel ?? 0, msg.value ?? 8192);
        break;
      case "allNotesOff":
        this.synth.allNotesOff();
        break;
      case "reset":
        this.synth.reset();
        break;
      case "reverbMix":
        this.synth.setReverbMix(msg.value);
        break;
      case "chorusMix":
        this.synth.setChorusMix(msg.value);
        break;
    }
  }

  process(_inputs, outputs) {
    const output = outputs[0];
    if (!output || output.length === 0) return true;

    const left = output[0];
    const right = output[1] ?? output[0];

    if (!this.synth) {
      left.fill(0);
      if (right !== left) right.fill(0);
      return true;
    }

    if (right === left) {
      if (this._monoScratch.length < left.length) {
        this._monoScratch = new Float32Array(left.length);
      }
      const scratch = this._monoScratch.subarray(0, left.length);
      this.synth.renderPlanar(left, scratch);
      for (let i = 0; i < left.length; i++) {
        left[i] = 0.5 * (left[i] + scratch[i]);
      }
    } else {
      this.synth.renderPlanar(left, right);
    }
    return true;
  }
}

registerProcessor("sound-hall-processor", SoundHallWorkletProcessor);
