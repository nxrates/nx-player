import { StretchNode } from './stretch-node';

type Deck = {
  el: HTMLAudioElement;
  src: MediaElementAudioSourceNode | null;
  gain: GainNode | null;
  st: StretchNode | null; // Signalsmith Stretch WASM time-stretcher
};

export class AudioPlayer {
  private ctx: AudioContext | null = null;
  private deckA: Deck;
  private deckB: Deck;
  private activeDeck: 'A' | 'B' = 'A';
  private crossfading = false;
  private crossfadeTimeoutId: ReturnType<typeof setTimeout> | null = null;
  private analyser: AnalyserNode | null = null;
  private masterGain: GainNode | null = null;
  private stretchRegistered = false;

  private targetRate = 1;
  private currentRate = 1;
  private incomingTargetRate = 1;
  private incomingCurrentRate = 1;

  // Callbacks
  onTimeUpdate: ((time: number) => void) | null = null;
  onEnded: (() => void) | null = null;
  onLoadedMetadata: ((duration: number) => void) | null = null;
  onError: ((error: string) => void) | null = null;
  onCrossfadeComplete: (() => void) | null = null;

  constructor() {
    this.deckA = { el: new Audio(), src: null, gain: null, st: null };
    this.deckB = { el: new Audio(), src: null, gain: null, st: null };

    // CRITICAL: preservesPitch = false because SoundTouch handles pitch preservation.
    // If true, the browser applies its own WSOLA on top of SoundTouch → double processing → artifacts.
    for (const deck of [this.deckA, this.deckB]) {
      deck.el.preservesPitch = false;
      (deck.el as any).webkitPreservesPitch = false;
    }

    // Wire events to active deck
    for (const deck of [this.deckA, this.deckB]) {
      deck.el.addEventListener('timeupdate', () => {
        if (this.getActiveDeckObj() === deck) {
          this.onTimeUpdate?.(deck.el.currentTime);
        }
      });
      deck.el.addEventListener('ended', () => {
        if (this.getActiveDeckObj() === deck && !this.crossfading) {
          this.onEnded?.();
        }
      });
      deck.el.addEventListener('loadedmetadata', () => {
        if (this.getActiveDeckObj() === deck) {
          this.onLoadedMetadata?.(deck.el.duration);
        }
      });
      deck.el.addEventListener('error', () => {
        if (this.getActiveDeckObj() === deck) {
          this.onError?.(deck.el.error?.message ?? 'Audio error');
        }
      });
    }
  }

  private async ensureContext() {
    if (!this.ctx) {
      this.ctx = new AudioContext();

      // Register Signalsmith Stretch WASM AudioWorklet (once)
      if (!this.stretchRegistered) {
        try {
          await StretchNode.register(this.ctx);
          this.stretchRegistered = true;
        } catch (e) {
          console.warn('Signalsmith Stretch WASM registration failed, falling back to native:', e);
          for (const deck of [this.deckA, this.deckB]) {
            deck.el.preservesPitch = true;
            (deck.el as any).webkitPreservesPitch = true;
          }
        }
      }

      // Master gain for volume control (0-2.0 for VLC-style amplification)
      this.masterGain = this.ctx.createGain();
      this.masterGain.connect(this.ctx.destination);

      // Wire chain: src → StretchNode (WASM) → gain → masterGain
      for (const deck of [this.deckA, this.deckB]) {
        deck.src = this.ctx.createMediaElementSource(deck.el);
        deck.gain = this.ctx.createGain();

        if (this.stretchRegistered) {
          deck.st = new StretchNode(this.ctx);
          await deck.st.ready; // Wait for WASM to initialize
          deck.src.connect(deck.st);
          deck.st.connect(deck.gain);
        } else {
          deck.src.connect(deck.gain);
        }

        deck.gain.connect(this.masterGain);
      }
      this.deckA.gain!.gain.setValueAtTime(1, this.ctx.currentTime);
      this.deckB.gain!.gain.setValueAtTime(0, this.ctx.currentTime);

      // AnalyserNode — created but NOT connected until visualizer requests it
      this.analyser = this.ctx.createAnalyser();
      this.analyser.fftSize = 1024;
      this.analyser.smoothingTimeConstant = 0.0;
    }
    if (this.ctx.state === 'suspended') await this.ctx.resume();
  }

  /** Release all media resources from a deck (frees decoded PCM + WASM heap). */
  private releaseDeck(deck: Deck) {
    deck.el.pause();
    deck.el.removeAttribute('src');
    deck.el.load(); // triggers "media element load algorithm" → releases decoded buffers
    // Destroy StretchNode to free its 16MB WASM heap
    if (deck.st) {
      deck.st.destroy();
      deck.st = null;
    }
  }

  /** Ensure a deck has a live StretchNode (recreate if destroyed after crossfade). */
  private async ensureStretch(deck: Deck) {
    if (deck.st || !this.stretchRegistered || !this.ctx) return;
    deck.st = new StretchNode(this.ctx);
    await deck.st.ready;
    // Rewire: disconnect src from gain, insert stretch in between
    if (deck.src && deck.gain) {
      try { deck.src.disconnect(deck.gain); } catch {}
      deck.src.connect(deck.st);
      deck.st.connect(deck.gain);
    }
  }

  private getActiveDeckObj() {
    return this.activeDeck === 'A' ? this.deckA : this.deckB;
  }
  private getInactiveDeckObj() {
    return this.activeDeck === 'A' ? this.deckB : this.deckA;
  }

  async play(src: string, metadata?: { title?: string; artist?: string; album?: string; artwork?: string }) {
    await this.ensureContext();
    const deck = this.getActiveDeckObj();
    await this.ensureStretch(deck);
    deck.el.src = src;
    deck.gain!.gain.cancelScheduledValues(0);
    deck.gain!.gain.setValueAtTime(1, this.ctx!.currentTime);
    await deck.el.play();

    // MediaSession
    if ('mediaSession' in navigator && metadata) {
      navigator.mediaSession.metadata = new MediaMetadata({
        title: metadata.title ?? '',
        artist: metadata.artist ?? '',
        album: metadata.album ?? '',
        artwork: metadata.artwork ? [{ src: metadata.artwork }] : [],
      });
    }
  }

  async startCrossfade(nextSrc: string, durationMs: number, metadata?: any) {
    await this.ensureContext();
    this.crossfading = true;

    const outgoing = this.getActiveDeckObj();
    const incoming = this.getInactiveDeckObj();

    // Ensure incoming deck has a StretchNode (may have been destroyed after last crossfade)
    await this.ensureStretch(incoming);

    // 1. Set incoming gain to ZERO before anything — no audible bleed
    incoming.gain!.gain.cancelScheduledValues(0);
    incoming.gain!.gain.setValueAtTime(0, this.ctx!.currentTime);

    // 2. Set incoming playback rate IMMEDIATELY since it's inaudible at gain=0
    incoming.el.playbackRate = this.incomingTargetRate;
    if (incoming.st) {
      incoming.st.setSpeed(this.incomingTargetRate);
    }
    this.incomingCurrentRate = this.incomingTargetRate;

    // 3. Load and preload the incoming track before starting the fade
    incoming.el.src = nextSrc;
    incoming.el.preload = 'auto';

    // Wait for enough data to play smoothly (canplaythrough = browser estimates no buffering needed)
    await new Promise<void>((resolve) => {
      const onReady = () => { incoming.el.removeEventListener('canplaythrough', onReady); resolve(); };
      // If already ready (cached), resolve immediately
      if (incoming.el.readyState >= 4) { resolve(); return; }
      incoming.el.addEventListener('canplaythrough', onReady, { once: true });
      // Timeout fallback: don't wait forever if stream is slow
      setTimeout(resolve, 2000);
    });

    await incoming.el.play();

    // 4. Schedule crossfade curves AFTER play started
    // Use a 50ms offset to let the decoder settle (avoids initial click/pop)
    const dur = durationMs / 1000;
    const steps = 128; // More steps = smoother curve (was 64)
    const curveOut = new Float32Array(steps);
    const curveIn = new Float32Array(steps);
    for (let i = 0; i < steps; i++) {
      const p = i / (steps - 1);
      curveOut[i] = Math.cos(p * Math.PI / 2);
      curveIn[i] = Math.sin(p * Math.PI / 2);
    }

    const fadeStart = this.ctx!.currentTime + 0.05; // 50ms settle time
    outgoing.gain!.gain.cancelScheduledValues(0);
    incoming.gain!.gain.cancelScheduledValues(0);
    outgoing.gain!.gain.setValueAtTime(1, fadeStart);
    incoming.gain!.gain.setValueAtTime(0, fadeStart);
    outgoing.gain!.gain.setValueCurveAtTime(curveOut, fadeStart, dur);
    incoming.gain!.gain.setValueCurveAtTime(curveIn, fadeStart, dur);

    // 5. After crossfade completes — swap decks, release outgoing
    if (this.crossfadeTimeoutId) clearTimeout(this.crossfadeTimeoutId);
    this.crossfadeTimeoutId = setTimeout(() => {
      this.crossfadeTimeoutId = null;
      this.activeDeck = this.activeDeck === 'A' ? 'B' : 'A';
      this.releaseDeck(outgoing);
      outgoing.gain!.gain.cancelScheduledValues(0);
      outgoing.gain!.gain.setValueAtTime(0, this.ctx!.currentTime);
      incoming.gain!.gain.cancelScheduledValues(0);
      incoming.gain!.gain.setValueAtTime(1, this.ctx!.currentTime);
      this.crossfading = false;
      this.onCrossfadeComplete?.();

      if (metadata && 'mediaSession' in navigator) {
        navigator.mediaSession.metadata = new MediaMetadata({
          title: metadata.title ?? '', artist: metadata.artist ?? '',
          album: metadata.album ?? '',
          artwork: metadata.artwork ? [{ src: metadata.artwork }] : [],
        });
      }
    }, durationMs + 50); // +50ms to account for the settle offset
  }

  cancelCrossfade() {
    if (!this.ctx || !this.crossfading) return;

    // Cancel the pending deck-swap timeout
    if (this.crossfadeTimeoutId) {
      clearTimeout(this.crossfadeTimeoutId);
      this.crossfadeTimeoutId = null;
    }

    const now = this.ctx.currentTime;

    // During crossfade, active deck = outgoing (old track), inactive = incoming (new track)
    const outgoing = this.getActiveDeckObj();
    const incoming = this.getInactiveDeckObj();

    // Stop the incoming deck
    incoming.gain!.gain.cancelScheduledValues(0);
    incoming.gain!.gain.setValueAtTime(0, now);
    this.releaseDeck(incoming);

    // Restore outgoing deck to full volume
    outgoing.gain!.gain.cancelScheduledValues(0);
    outgoing.gain!.gain.setValueAtTime(1, now);

    this.crossfading = false;
  }

  getAnalyser(): AnalyserNode | null { return this.analyser; }
  getContext(): AudioContext | null { return this.ctx; }

  /** Get the total audio output latency in seconds (for audio-visual sync compensation). */
  getOutputLatency(): number {
    if (!this.ctx) return 0;
    // outputLatency: time from audio graph to speakers (hardware dependent)
    // baseLatency: processing latency within the graph
    return (this.ctx.outputLatency ?? 0) + (this.ctx.baseLatency ?? 0);
  }

  /** Connect analyser to audio graph (for visualizer). */
  connectAnalyser() {
    if (this.analyser && this.masterGain) {
      try { this.masterGain.connect(this.analyser); } catch {}
    }
  }

  /** Disconnect analyser from audio graph (saves CPU when visualizer hidden). */
  disconnectAnalyser() {
    if (this.analyser) {
      try { this.masterGain?.disconnect(this.analyser); } catch {}
      try { this.analyser.disconnect(); } catch {}
    }
  }

  pause() {
    this.getActiveDeckObj().el.pause();
    // Suspend AudioContext to stop processing the audio graph (saves CPU when idle)
    if (this.ctx && this.ctx.state === 'running') this.ctx.suspend();
  }
  async resume() {
    if (this.ctx && this.ctx.state === 'suspended') await this.ctx.resume();
    this.getActiveDeckObj().el.play();
  }
  pauseAll() {
    this.deckA.el.pause();
    this.deckB.el.pause();
    if (this.ctx && this.ctx.state === 'running') this.ctx.suspend();
  }
  resumeAll() {
    if (this.ctx && this.ctx.state === 'suspended') this.ctx.resume();
    this.deckA.el.play();
    this.deckB.el.play();
  }
  seek(time: number) { this.getActiveDeckObj().el.currentTime = time; }

  setVolume(v: number) {
    // Use masterGain for volume (supports 0-2.0 amplification via Web Audio API)
    if (this.masterGain && this.ctx) {
      this.masterGain.gain.setValueAtTime(v, this.ctx.currentTime);
    }
    // Also set on HTMLAudioElements for before AudioContext exists (capped at 1.0)
    this.deckA.el.volume = Math.min(1, v);
    this.deckB.el.volume = Math.min(1, v);
  }

  setPlaybackRate(r: number) {
    this.targetRate = r;
    this.currentRate = r;
    const deck = this.getActiveDeckObj();
    deck.el.playbackRate = r;
    // Tell Signalsmith Stretch the current speed so it compensates pitch
    if (deck.st) {
      deck.st.setSpeed(r);
    }
  }

  /** Set the incoming deck's playback rate (for BPM matching during crossfade). */
  setIncomingPlaybackRate(r: number) {
    this.incomingTargetRate = r;
    this.incomingCurrentRate = r;
    const deck = this.getInactiveDeckObj();
    deck.el.playbackRate = r;
    if (deck.st) {
      deck.st.setSpeed(r);
    }
  }

  get paused() { return this.getActiveDeckObj().el.paused; }
  get currentTime() { return this.getActiveDeckObj().el.currentTime; }
  get currentDuration() { return this.getActiveDeckObj().el.duration; }
  get incomingTime() { return this.getInactiveDeckObj().el.currentTime; }
  get incomingDuration() { return this.getInactiveDeckObj().el.duration; }
  get isCrossfading() { return this.crossfading; }

  setMediaSessionHandlers(handlers: { onPrev?: () => void; onNext?: () => void }) {
    if ('mediaSession' in navigator) {
      navigator.mediaSession.setActionHandler('previoustrack', handlers.onPrev ?? null);
      navigator.mediaSession.setActionHandler('nexttrack', handlers.onNext ?? null);
      navigator.mediaSession.setActionHandler('play', () => this.resume());
      navigator.mediaSession.setActionHandler('pause', () => this.pause());
    }
  }
}

export const audioPlayer = new AudioPlayer();
