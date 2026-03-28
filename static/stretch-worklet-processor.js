/**
 * Signalsmith Stretch AudioWorkletProcessor
 *
 * Wraps the Signalsmith Stretch WASM module for real-time pitch-corrected
 * time-stretching inside a Web Audio AudioWorklet.
 *
 * Architecture:
 *   HTMLAudioElement (playbackRate = speed, preservesPitch = false)
 *     -> MediaElementSourceNode
 *       -> this AudioWorkletProcessor (pitch correction via WASM)
 *         -> GainNode -> destination
 *
 * When playbackRate = 1.2, the browser feeds audio 20% faster with pitch
 * shifted UP by factor 1.2. We call setTransposeFactor(1.0 / speed) on the
 * WASM stretcher to shift pitch DOWN by the same ratio, restoring original
 * pitch. Input and output are both 128 frames (one render quantum).
 *
 * WASM C API (Emscripten exports):
 *   _stretch_init(channels, sampleRate, blockSize) -> void
 *   _stretch_set_transpose_factor(factor)          -> void
 *   _stretch_process(inputPtr, inputFrames, outputPtr, outputFrames) -> void
 *   _stretch_reset()                               -> void
 *   _stretch_flush(outputPtr, maxFrames)            -> int (frames written)
 *   _malloc(bytes) -> ptr
 *   _free(ptr)     -> void
 *
 * Memory layout (ALLOW_MEMORY_GROWTH=0):
 *   We allocate two interleaved buffers on the WASM heap:
 *     inputPtr:  channels * MAX_BLOCK_SIZE * 4 bytes
 *     outputPtr: channels * MAX_BLOCK_SIZE * 4 bytes
 *   Float32Array views are created once and reused (zero allocations in process()).
 */

const PROCESSOR_NAME = 'stretch-worklet-processor';
const CHANNELS = 2;
const RENDER_QUANTUM = 128;
// Maximum block size we'll ever need. At 4x speed the browser could
// theoretically feed up to 4 * 128 = 512 frames, but the AudioWorklet
// spec guarantees exactly 128 frames per call. We use 128.
const MAX_BLOCK_SIZE = 128;

class StretchWorkletProcessor extends AudioWorkletProcessor {
  static get parameterDescriptors() {
    return [];
  }

  constructor() {
    super();

    // State
    this._ready = false;
    this._speed = 1.0;
    this._pitchSemitones = 0.0;
    this._transposeFactor = 1.0; // combined factor sent to WASM

    // WASM
    this._instance = null;
    this._inputPtr = 0;
    this._outputPtr = 0;
    this._inputView = null;  // Float32Array over WASM heap (input)
    this._outputView = null; // Float32Array over WASM heap (output)

    this.port.onmessage = this._onMessage.bind(this);
  }

  /**
   * Recalculate the transpose factor from speed and pitch semitones.
   *
   * The browser's playbackRate shifts pitch UP by `speed`.
   * We want to:
   *   1. Undo the browser's pitch shift: multiply by 1/speed
   *   2. Apply user-requested pitch shift: multiply by 2^(semitones/12)
   *
   * Combined: transposeFactor = (1 / speed) * 2^(semitones / 12)
   */
  _updateTransposeFactor() {
    var factor = (1.0 / this._speed) * Math.pow(2.0, this._pitchSemitones / 12.0);
    this._transposeFactor = factor;
    if (this._ready) {
      this._instance.exports._stretch_set_transpose_factor(factor);
    }
  }

  _onMessage(e) {
    var data = e.data;
    switch (data.type) {
      case 'load-wasm':
        this._loadWasm(data.wasmBytes);
        break;
      case 'set-speed':
        this._speed = data.value;
        this._updateTransposeFactor();
        break;
      case 'set-pitch-semitones':
        this._pitchSemitones = data.value;
        this._updateTransposeFactor();
        break;
      case 'reset':
        if (this._ready) {
          this._instance.exports._stretch_reset();
        }
        break;
      case 'destroy':
        // Free WASM heap allocations and mark processor as inactive
        if (this._ready && this._instance) {
          var exports = this._instance.exports;
          if (exports._free) {
            if (this._inputPtr) exports._free(this._inputPtr);
            if (this._outputPtr) exports._free(this._outputPtr);
          }
          this._inputPtr = 0;
          this._outputPtr = 0;
          this._inputView = null;
          this._outputView = null;
          this._instance = null;
          this._ready = false;
        }
        break;
    }
  }

  _loadWasm(wasmBytes) {
    // Synchronous instantiation on the audio thread — permitted and fast
    // because we are NOT on the main thread.
    var wasmModule = new WebAssembly.Module(wasmBytes);
    var wasmInstance = new WebAssembly.Instance(wasmModule, {
      env: {
        // Emscripten may require these stubs; provide no-ops for a minimal build.
        emscripten_notify_memory_growth: function() {},
        // Signalsmith Stretch minimal build should not need anything else,
        // but add a catch-all for safety in debug builds.
      },
      wasi_snapshot_preview1: {
        proc_exit: function() {},
        fd_close: function() { return 0; },
        fd_write: function() { return 0; },
        fd_seek: function() { return 0; },
      },
    });

    this._instance = wasmInstance;
    var exports = wasmInstance.exports;

    // Initialize the stretcher: 2 channels, current sample rate, block size 4096
    // (internal FFT/overlap size for quality — NOT the render quantum)
    exports._stretch_init(CHANNELS, sampleRate, 4096);

    // Allocate interleaved buffers on WASM heap
    // Each buffer: channels * MAX_BLOCK_SIZE * 4 bytes (float32)
    var bufferBytes = CHANNELS * MAX_BLOCK_SIZE * 4;
    this._inputPtr = exports._malloc(bufferBytes);
    this._outputPtr = exports._malloc(bufferBytes);

    // Create Float32Array views over the WASM heap memory.
    // ALLOW_MEMORY_GROWTH=0, so these views remain valid forever.
    var heap = exports.memory.buffer;
    this._inputView = new Float32Array(heap, this._inputPtr, CHANNELS * MAX_BLOCK_SIZE);
    this._outputView = new Float32Array(heap, this._outputPtr, CHANNELS * MAX_BLOCK_SIZE);

    // Apply initial transpose factor
    this._updateTransposeFactor();

    this._ready = true;
    this.port.postMessage({ type: 'ready' });
  }

  process(inputs, outputs, parameters) {
    // Always return true to keep the processor alive.
    if (!this._ready) return true;

    var input = inputs[0];
    var output = outputs[0];

    // Guard: no input or output channels available
    if (!input || !input.length || !output || !output.length) return true;

    var leftIn = input[0];
    var rightIn = input.length > 1 ? input[1] : input[0];
    var leftOut = output[0];
    var rightOut = output.length > 1 ? output[1] : output[0];
    var frames = leftIn.length; // Always 128 (render quantum)

    // --- Copy planar input into WASM interleaved buffer ---
    // No allocations: we write directly into the pre-cached Float32Array view.
    var inView = this._inputView;
    for (var i = 0; i < frames; i++) {
      inView[i * 2] = leftIn[i];
      inView[i * 2 + 1] = rightIn[i];
    }

    // --- Process ---
    // Both inputFrames and outputFrames are 128 (the render quantum).
    // The browser already handles the speed change via playbackRate on the
    // HTMLAudioElement. Signalsmith Stretch only needs to apply the pitch
    // transpose factor to correct (or shift) pitch.
    this._instance.exports._stretch_process(
      this._inputPtr,
      frames,
      this._outputPtr,
      frames
    );

    // --- Copy WASM interleaved output back to planar AudioWorklet buffers ---
    var outView = this._outputView;
    for (var i = 0; i < frames; i++) {
      leftOut[i] = outView[i * 2];
      rightOut[i] = outView[i * 2 + 1];
    }

    return true;
  }
}

registerProcessor(PROCESSOR_NAME, StretchWorkletProcessor);
