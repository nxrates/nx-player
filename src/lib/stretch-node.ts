/**
 * Main-thread AudioWorkletNode wrapper for the Signalsmith Stretch WASM processor.
 *
 * Usage:
 *   await StretchNode.register(audioCtx);
 *   const node = new StretchNode(audioCtx);
 *   await node.ready;
 *   source.connect(node);
 *   node.connect(destination);
 *   node.setSpeed(1.2);           // compensate HTMLAudioElement.playbackRate
 *   node.setPitchSemitones(-3);   // independent pitch shift
 *   node.reset();                 // flush internal buffers on seek
 */

const PROCESSOR_NAME = 'stretch-worklet-processor';
const PROCESSOR_URL = '/stretch-worklet-processor.js';
const WASM_URL = '/stretch.wasm';

export class StretchNode extends AudioWorkletNode {
  /**
   * Promise that resolves when the WASM module is loaded and the processor
   * is ready to process audio.
   */
  readonly ready: Promise<void>;

  private _resolveReady!: () => void;
  private _wasmBytes: ArrayBuffer | null = null;

  /**
   * Registers the AudioWorklet processor module and fetches the WASM binary.
   * Must be called once before constructing any StretchNode instances.
   *
   * The fetched WASM bytes are stored on the AudioContext so that multiple
   * StretchNode instances (e.g. two decks) can share the same binary.
   */
  static async register(ctx: AudioContext): Promise<void> {
    // Fetch WASM binary and register worklet processor in parallel
    const [wasmBytes] = await Promise.all([
      fetch(WASM_URL).then((r) => {
        if (!r.ok) throw new Error(`Failed to fetch ${WASM_URL}: ${r.status}`);
        return r.arrayBuffer();
      }),
      ctx.audioWorklet.addModule(PROCESSOR_URL),
    ]);

    // Stash WASM bytes on the context for StretchNode constructors to grab
    (ctx as any).__stretchWasmBytes = wasmBytes;
  }

  /**
   * Creates a StretchNode.
   *
   * After construction, send the WASM binary to the processor and await
   * `node.ready` before connecting audio.
   */
  constructor(ctx: AudioContext) {
    super(ctx, PROCESSOR_NAME, {
      numberOfInputs: 1,
      numberOfOutputs: 1,
      outputChannelCount: [2],
    });

    this._wasmBytes = (ctx as any).__stretchWasmBytes ?? null;

    this.ready = new Promise<void>((resolve) => {
      this._resolveReady = resolve;
    });

    // Listen for 'ready' from the processor
    this.port.onmessage = (e: MessageEvent) => {
      if (e.data.type === 'ready') {
        this._resolveReady();
      }
    };

    // Send the WASM binary to the processor for synchronous instantiation
    if (this._wasmBytes) {
      // Make a copy for this processor (original stays on ctx for other decks)
      const copy = this._wasmBytes.slice(0);
      this.port.postMessage(
        { type: 'load-wasm', wasmBytes: copy },
        [copy] // transfer ownership of the copy — no extra memory held by main thread
      );
    } else {
      console.error(
        'StretchNode: No WASM bytes found. Did you call StretchNode.register(ctx) first?'
      );
    }
  }

  /**
   * Notify the processor of the current HTMLAudioElement.playbackRate.
   *
   * The processor uses this to compute the transpose factor that undoes the
   * pitch shift caused by the browser's playback rate change:
   *   transposeFactor = (1 / speed) * 2^(pitchSemitones / 12)
   */
  setSpeed(speed: number): void {
    this.port.postMessage({ type: 'set-speed', value: speed });
  }

  /**
   * Apply an independent pitch shift in semitones (positive = up, negative = down).
   * This is additive on top of the playback-rate pitch compensation.
   */
  setPitchSemitones(semitones: number): void {
    this.port.postMessage({ type: 'set-pitch-semitones', value: semitones });
  }

  /**
   * Reset the stretcher's internal state. Call this on seek or track change
   * to flush internal overlap buffers and avoid audio artifacts.
   */
  reset(): void {
    this.port.postMessage({ type: 'reset' });
  }

  /**
   * Destroy this node — disconnect from audio graph and release WASM memory.
   * Call this when the deck is no longer needed (e.g., after crossfade completes).
   */
  destroy(): void {
    try { this.disconnect(); } catch {}
    this.port.postMessage({ type: 'destroy' });
    this.port.close();
  }
}
