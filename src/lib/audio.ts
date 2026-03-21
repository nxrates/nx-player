export class AudioPlayer {
  private ctx: AudioContext | null = null;
  private deckA: { el: HTMLAudioElement; src: MediaElementAudioSourceNode | null; gain: GainNode | null };
  private deckB: { el: HTMLAudioElement; src: MediaElementAudioSourceNode | null; gain: GainNode | null };
  private activeDeck: 'A' | 'B' = 'A';
  private crossfading = false;
  private crossfadeTimeoutId: ReturnType<typeof setTimeout> | null = null;
  private analyser: AnalyserNode | null = null;
  private masterGain: GainNode | null = null;

  // Smooth playback rate ramping (avoids audio glitches)
  private targetRate = 1;
  private currentRate = 1;
  private rateAnimationId: number | null = null;
  private incomingTargetRate = 1;
  private incomingCurrentRate = 1;
  private incomingRateAnimationId: number | null = null;

  // Callbacks
  onTimeUpdate: ((time: number) => void) | null = null;
  onEnded: (() => void) | null = null;
  onLoadedMetadata: ((duration: number) => void) | null = null;
  onError: ((error: string) => void) | null = null;
  onCrossfadeComplete: (() => void) | null = null;

  constructor() {
    this.deckA = { el: new Audio(), src: null, gain: null };
    this.deckB = { el: new Audio(), src: null, gain: null };

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
      // Create source nodes and gain nodes
      // Master gain for volume control (0-2.0 for VLC-style amplification)
      this.masterGain = this.ctx.createGain();
      this.masterGain.connect(this.ctx.destination);

      for (const deck of [this.deckA, this.deckB]) {
        deck.src = this.ctx.createMediaElementSource(deck.el);
        deck.gain = this.ctx.createGain();
        deck.src.connect(deck.gain);
        deck.gain.connect(this.masterGain);
      }
      this.deckA.gain!.gain.setValueAtTime(1, this.ctx.currentTime);
      this.deckB.gain!.gain.setValueAtTime(0, this.ctx.currentTime);

      // AnalyserNode — passive tap on master gain for visualizers
      this.analyser = this.ctx.createAnalyser();
      this.analyser.fftSize = 2048;
      this.analyser.smoothingTimeConstant = 0.0;
      this.masterGain.connect(this.analyser);
    }
    if (this.ctx.state === 'suspended') await this.ctx.resume();
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

    // Cancel any prior automation on both decks before setting up new values
    const preNow = this.ctx!.currentTime;
    outgoing.gain!.gain.cancelScheduledValues(0);
    incoming.gain!.gain.cancelScheduledValues(0);

    // Set incoming gain to 0 via automation API (NOT .value — direct assignment
    // conflicts with setValueCurveAtTime in some browsers)
    incoming.gain!.gain.setValueAtTime(0, preNow);
    // Ensure outgoing is at full volume
    outgoing.gain!.gain.setValueAtTime(1, preNow);

    incoming.el.src = nextSrc;
    await incoming.el.play();

    // Equal power crossfade curves
    const dur = durationMs / 1000;
    const steps = 64;
    const curveOut = new Float32Array(steps);
    const curveIn = new Float32Array(steps);
    for (let i = 0; i < steps; i++) {
      const p = i / (steps - 1);
      curveOut[i] = Math.cos(p * Math.PI / 2);
      curveIn[i] = Math.sin(p * Math.PI / 2);
    }

    // Schedule the crossfade curves — use a tiny offset to avoid start-time races
    const fadeStart = this.ctx!.currentTime + 0.005;
    outgoing.gain!.gain.cancelScheduledValues(0);
    incoming.gain!.gain.cancelScheduledValues(0);
    outgoing.gain!.gain.setValueAtTime(1, fadeStart);
    incoming.gain!.gain.setValueAtTime(0, fadeStart);
    outgoing.gain!.gain.setValueCurveAtTime(curveOut, fadeStart, dur);
    incoming.gain!.gain.setValueCurveAtTime(curveIn, fadeStart, dur);

    // After crossfade completes
    if (this.crossfadeTimeoutId) clearTimeout(this.crossfadeTimeoutId);
    this.crossfadeTimeoutId = setTimeout(() => {
      this.crossfadeTimeoutId = null;
      this.activeDeck = this.activeDeck === 'A' ? 'B' : 'A';
      outgoing.el.pause();
      outgoing.el.src = '';
      // Use automation API to set gain after crossfade
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
    }, durationMs);
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
    incoming.el.pause();
    incoming.el.src = '';

    // Restore outgoing deck to full volume
    outgoing.gain!.gain.cancelScheduledValues(0);
    outgoing.gain!.gain.setValueAtTime(1, now);

    this.crossfading = false;
  }

  getAnalyser(): AnalyserNode | null { return this.analyser; }
  getContext(): AudioContext | null { return this.ctx; }

  pause() { this.getActiveDeckObj().el.pause(); }
  resume() { this.getActiveDeckObj().el.play(); }
  pauseAll() {
    this.deckA.el.pause();
    this.deckB.el.pause();
  }
  resumeAll() {
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
    if (!this.rateAnimationId) {
      this.animateRate();
    }
  }

  private animateRate() {
    const diff = this.targetRate - this.currentRate;
    if (Math.abs(diff) < 0.001) {
      this.currentRate = this.targetRate;
      this.getActiveDeckObj().el.playbackRate = this.currentRate;
      this.rateAnimationId = null;
      return;
    }
    // Ease toward target (~200ms ramp via lerp factor 0.15 per frame at 60fps)
    this.currentRate += diff * 0.15;
    this.getActiveDeckObj().el.playbackRate = this.currentRate;
    this.rateAnimationId = requestAnimationFrame(() => this.animateRate());
  }

  setIncomingPlaybackRate(r: number) {
    this.incomingTargetRate = r;
    if (!this.incomingRateAnimationId) {
      this.animateIncomingRate();
    }
  }

  private animateIncomingRate() {
    const incoming = this.getInactiveDeckObj();
    const diff = this.incomingTargetRate - this.incomingCurrentRate;
    if (Math.abs(diff) < 0.001) {
      this.incomingCurrentRate = this.incomingTargetRate;
      incoming.el.playbackRate = this.incomingCurrentRate;
      this.incomingRateAnimationId = null;
      return;
    }
    this.incomingCurrentRate += diff * 0.15;
    incoming.el.playbackRate = this.incomingCurrentRate;
    this.incomingRateAnimationId = requestAnimationFrame(() => this.animateIncomingRate());
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
