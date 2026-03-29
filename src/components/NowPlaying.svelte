<script lang="ts">
  import Waveform from './Waveform.svelte';
  import MilkDrop from './MilkDrop.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { LogicalSize } from '@tauri-apps/api/dpi';
  import {
    playPause, next, prev, getStatus, getCurrentTrack,
    getShuffle, toggleShuffle, selectRandom, getRepeat, cycleRepeat,
    getCoverUrl, getCrossfadeProgress, getOutgoingTrack,
    getOutgoingPosition, getOutgoingDuration,
  } from '../stores/player.svelte';
  import { getWaveform } from '../lib/ipc';

  type ViewMode = 'mini' | 'normal' | 'milkdrop';
  let viewMode = $state<ViewMode>((localStorage.getItem('nx-view-mode') as ViewMode) || 'normal');

  // MilkDrop expanded/fullscreen state
  let milkdropExpanded = $state(false);
  let controlsHidden = $state(false);
  let controlsTimeout: ReturnType<typeof setTimeout> | null = null;
  let isFullscreen = $state(false);

  async function setViewMode(mode: ViewMode) {
    viewMode = mode;
    localStorage.setItem('nx-view-mode', mode);
    const win = getCurrentWindow();
    if (mode === 'mini') {
      await win.setMinSize(new LogicalSize(360, 140));
      await win.setSize(new LogicalSize(360, 160));
      // Reset milkdrop expanded state when switching modes
      milkdropExpanded = false;
      controlsHidden = false;
    } else {
      await win.setSize(new LogicalSize(360, 560));
      await win.setMinSize(new LogicalSize(360, 560));
    }
  }

  function toggleMiniMode() {
    setViewMode(viewMode === 'mini' ? 'normal' : 'mini');
  }

  // Listen for mini-mode toggle from keyboard shortcut
  $effect(() => {
    const handler = () => toggleMiniMode();
    document.addEventListener('toggle-mini-mode', handler);
    return () => document.removeEventListener('toggle-mini-mode', handler);
  });

  // Listen for fullscreen toggle from TopBar green button
  $effect(() => {
    const handler = () => toggleFullscreen();
    document.addEventListener('toggle-fullscreen', handler);
    return () => document.removeEventListener('toggle-fullscreen', handler);
  });

  // MilkDrop expanded controls auto-hide
  function resetControlsTimer() {
    controlsHidden = false;
    if (controlsTimeout) clearTimeout(controlsTimeout);
    controlsTimeout = setTimeout(() => {
      if (milkdropExpanded || isFullscreen) {
        controlsHidden = true;
      }
    }, 3000);
  }

  function handleExpandedMouseMove() {
    if (milkdropExpanded || isFullscreen) {
      resetControlsTimer();
    }
  }

  function toggleMilkdropExpanded() {
    milkdropExpanded = !milkdropExpanded;
    if (milkdropExpanded) {
      resetControlsTimer();
    } else {
      controlsHidden = false;
      if (controlsTimeout) clearTimeout(controlsTimeout);
    }
  }

  async function toggleFullscreen() {
    const win = getCurrentWindow();
    if (isFullscreen) {
      await win.setFullscreen(false);
      isFullscreen = false;
      milkdropExpanded = false;
      controlsHidden = false;
      if (controlsTimeout) clearTimeout(controlsTimeout);
    } else {
      // Await fullscreen BEFORE setting expanded state to avoid resize race condition
      await win.setFullscreen(true);
      isFullscreen = true;
      milkdropExpanded = true;
      resetControlsTimer();
    }
  }

  // Escape key exits fullscreen + cleanup timeout on unmount
  $effect(() => {
    const handler = async (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isFullscreen) {
        e.preventDefault();
        e.stopPropagation();
        await getCurrentWindow().setFullscreen(false);
        isFullscreen = false;
        milkdropExpanded = false;
        controlsHidden = false;
        if (controlsTimeout) clearTimeout(controlsTimeout);
      }
    };
    document.addEventListener('keydown', handler, true);
    return () => {
      document.removeEventListener('keydown', handler, true);
      if (controlsTimeout) { clearTimeout(controlsTimeout); controlsTimeout = null; }
    };
  });

  // Outgoing track waveform data
  let outgoingWaveform = $state<number[] | null>(null);
  let lastOutgoingId = $state<string | null>(null);

  $effect(() => {
    const og = getOutgoingTrack();
    if (og && og.id !== lastOutgoingId) {
      lastOutgoingId = og.id;
      getWaveform(og.id).then(d => { outgoingWaveform = d; }).catch(() => {});
    }
    if (!og) {
      lastOutgoingId = null;
      outgoingWaveform = null;
    }
  });

  let status = $derived(getStatus());
  let track = $derived(getCurrentTrack());
  let coverUrl = $derived(getCoverUrl());
  let shuffleOn = $derived(getShuffle());
  let repeatMode = $derived(getRepeat());
  let outgoingTrack = $derived(getOutgoingTrack());
  let cfProgress = $derived(getCrossfadeProgress());
  let ogPos = $derived(getOutgoingPosition());
  let ogDur = $derived(getOutgoingDuration());

  let liked = $state(false);

  function handleOutgoingClick() {
    // Cancel crossfade and return to outgoing track
    prev();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="now-playing"
  class:mini-mode={viewMode === 'mini'}
  class:milkdrop-expanded={milkdropExpanded}
  class:controls-hidden-mode={controlsHidden}
  class:fullscreen={isFullscreen}
  onmousemove={handleExpandedMouseMove}
>
  {#if viewMode === 'mini'}
    <!-- Mini mode expand button -->
    <button class="mini-expand-btn" onclick={() => setViewMode('normal')} title="Expand (M)">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/>
      </svg>
    </button>
  {/if}

  {#if viewMode !== 'mini'}
    <!-- Full-screen content (art or milkdrop) -->
    <div class="content-fill">
      {#if viewMode === 'milkdrop'}
        <MilkDrop expanded={milkdropExpanded} />
      {:else}
        {#if coverUrl}
          <img src={coverUrl} alt="Album art" class="full-art" />
        {:else}
          <div class="art-placeholder">
            <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round">
              <path d="M9 18V5l12-2v13"/>
              <circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>
            </svg>
          </div>
        {/if}
      {/if}
    </div>

    <!-- BPM tag: floating top-left, aligned with art-pill -->
    {#if track?.bpm}
      <div class="bpm-tag">{Math.round(track.bpm)} BPM</div>
    {/if}

    <!-- Floating vertical pill: heart + view toggle (top-right) -->
    <div class="art-pill">
      <button class="pill-btn" class:liked onclick={() => liked = !liked} title="Like">
        <svg width="14" height="14" viewBox="0 0 24 24" fill={liked ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>
        </svg>
      </button>
      {#if viewMode === 'normal'}
        <button class="pill-btn" onclick={() => setViewMode('milkdrop')} title="Visualizer">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/>
          </svg>
        </button>
      {:else}
        <button class="pill-btn" onclick={() => setViewMode('normal')} title="Album Art">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="3" y="3" width="18" height="18" rx="2"/>
            <circle cx="8.5" cy="8.5" r="1.5"/>
            <path d="m21 15-5-5L5 21"/>
          </svg>
        </button>
      {/if}
    </div>


  {/if}

  <!-- Controls overlay (glass effect in normal/milkdrop, solid in mini) -->
  <div class="controls-overlay" class:glass={viewMode !== 'mini'}>
    <!-- Waveform: top element of the overlay, forms its ragged upper boundary -->
    {#if viewMode !== 'mini'}
      <div class="waveform-edge">
        {#if outgoingTrack}
          <div class="cf-waveform" style="opacity: {Math.max(0.1, 1 - cfProgress)}">
            <Waveform overrideTrackId={outgoingTrack.id} overridePosition={ogPos} overrideDuration={ogDur} tint="red" beatGrid={outgoingTrack.beat_grid} downbeats={outgoingTrack.downbeats} />
          </div>
        {/if}
        <div class="cf-waveform" style="opacity: {outgoingTrack ? Math.max(0.3, cfProgress) : 1}">
          <Waveform tint={undefined} beatGrid={track?.beat_grid} downbeats={track?.downbeats} title={track?.title} artist={track?.artist} bpm={track?.bpm} showOverlay={!outgoingTrack} />
        </div>
      </div>
    {/if}

    <!-- Mini mode: inline waveform + track info (only shown in mini) -->
    {#if viewMode === 'mini'}
      <div class="cf-track cf-in">
        <Waveform tint={undefined} beatGrid={track?.beat_grid} downbeats={track?.downbeats} />
        <div class="track-info">
          <div class="info-left">
            <span class="track-title">{track?.title || 'No Track'}{track?.artist ? ' \u2014 ' + track.artist : ''}</span>
          </div>
        </div>
      </div>
    {/if}

    <!-- Transport -->
    <div class="transport">
      <button
        class="transport-btn side"
        class:active={shuffleOn}
        onclick={() => { toggleShuffle(); if (!shuffleOn) selectRandom(); }}
        title="Shuffle"
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M2 18h1.4c1.3 0 2.5-.6 3.3-1.7l6.1-8.6c.7-1.1 2-1.7 3.3-1.7H20"/>
          <path d="m18 2 4 4-4 4"/>
          <path d="M2 6h1.9c1.5 0 2.9.9 3.6 2.2"/>
          <path d="M20 18h-3.9c-1.3 0-2.5-.6-3.3-1.7l-.6-.8"/>
          <path d="m18 14 4 4-4 4"/>
        </svg>
      </button>

      <button class="transport-btn skip" onclick={() => prev()} title="Previous">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <polygon points="19 20 9 12 19 4 19 20"/>
          <line x1="5" y1="19" x2="5" y2="5"/>
        </svg>
      </button>

      <button class="transport-btn play-btn" onclick={() => playPause()} title={status === 'playing' ? 'Pause' : 'Play'}>
        {#if status === 'playing'}
          <svg width="28" height="28" viewBox="0 0 24 24" fill="var(--bg)" stroke="none">
            <rect x="14" y="4" width="4" height="16" rx="1"/>
            <rect x="6" y="4" width="4" height="16" rx="1"/>
          </svg>
        {:else}
          <svg width="28" height="28" viewBox="0 0 24 24" fill="var(--bg)" stroke="none">
            <polygon points="8 4 20 12 8 20"/>
          </svg>
        {/if}
      </button>

      <button class="transport-btn skip" onclick={() => next()} title="Next">
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <polygon points="5 4 15 12 5 20 5 4"/>
          <line x1="19" y1="5" x2="19" y2="19"/>
        </svg>
      </button>

      <button
        class="transport-btn side"
        class:active={repeatMode !== 'off'}
        onclick={() => cycleRepeat()}
        title={repeatMode === 'one' ? 'Repeat One' : repeatMode === 'all' ? 'Repeat All' : 'Repeat Off'}
      >
        {#if repeatMode === 'one'}
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="m17 2 4 4-4 4"/>
            <path d="M3 11v-1a4 4 0 0 1 4-4h14"/>
            <path d="m7 22-4-4 4-4"/>
            <path d="M21 13v1a4 4 0 0 1-4 4H3"/>
            <path d="M11 10h1v4"/>
          </svg>
        {:else}
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="m17 2 4 4-4 4"/>
            <path d="M3 11v-1a4 4 0 0 1 4-4h14"/>
            <path d="m7 22-4-4 4-4"/>
            <path d="M21 13v1a4 4 0 0 1-4 4H3"/>
          </svg>
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .now-playing {
    flex: 1;
    position: relative;
    overflow: hidden;
    z-index: 1;
  }

  .now-playing.fullscreen {
    position: fixed;
    inset: 0;
    width: 100vw;
    height: 100vh;
    z-index: 100;
  }

  .now-playing.mini-mode {
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
  }

  /* --- Full-screen content fill (art or milkdrop) --- */
  .content-fill {
    position: absolute;
    inset: 0;
    z-index: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    /* Reserve space so album art isn't hidden behind controls */
    padding-bottom: calc(48px + clamp(48px, 14vh, 110px));
  }

  /* Fullscreen/expanded: MilkDrop goes edge-to-edge, controls overlay on top */
  .fullscreen .content-fill,
  .milkdrop-expanded .content-fill {
    padding-bottom: 0;
  }

  .full-art {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
  }

  .art-blur {
    position: fixed;
    inset: 0;
    background-size: cover;
    background-position: center;
    filter: blur(80px) saturate(1.5);
    opacity: 0.35;
    transform: scale(1.5);
    pointer-events: none;
    z-index: -1;
  }

  .art-placeholder {
    width: 100%;
    height: 100%;
    background: var(--bg-surface);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
  }

  /* --- Controls overlay --- */
  .controls-overlay {
    position: relative;
    z-index: 5;
    padding: 0 0 4px;
  }

  .controls-overlay.glass {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    /* Transparent — the global art-blur tint shows through from app-root */
    background: transparent;
  }

  /* Waveform edge: first child inside the overlay, fills available space above transport */
  .waveform-edge {
    width: 100%;
    height: clamp(48px, 14vh, 110px);
    position: relative;
    flex-shrink: 0;
  }

  .cf-waveform {
    position: absolute;
    inset: 0;
    width: 100%;
    display: flex;
    transition: opacity 300ms ease;
  }

  .mini-mode .controls-overlay {
    position: relative;
    background: var(--bg);
  }

  /* MilkDrop expanded / fullscreen: overlay controls and auto-hide with slide */
  .milkdrop-expanded .controls-overlay,
  .fullscreen .controls-overlay {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    z-index: 110;
    transition: transform 300ms ease, opacity 300ms ease;
  }
  .milkdrop-expanded.controls-hidden-mode .controls-overlay,
  .fullscreen.controls-hidden-mode .controls-overlay {
    transform: translateY(100%);
    opacity: 0;
    pointer-events: none;
  }

  /* --- BPM tag: floating top-left --- */
  .bpm-tag {
    position: absolute;
    top: 12px;
    left: 12px;
    z-index: 10;
    font-size: clamp(10px, 1.4vw, 13px);
    font-weight: 600;
    color: rgba(255, 255, 255, 0.75);
    background: rgba(0, 0, 0, 0.35);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    border-radius: 14px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    padding: 4px 12px;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }
  .fullscreen .bpm-tag,
  .milkdrop-expanded .bpm-tag {
    transition: opacity 300ms ease;
  }
  .fullscreen.controls-hidden-mode .bpm-tag,
  .milkdrop-expanded.controls-hidden-mode .bpm-tag {
    opacity: 0;
  }

  /* --- Floating vertical pill (top-right): heart + view toggle --- */
  .art-pill {
    position: absolute;
    top: 12px;
    right: 12px;
    z-index: 10;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: rgba(0, 0, 0, 0.35);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    border-radius: 14px;
    padding: 4px;
    border: 1px solid rgba(255, 255, 255, 0.08);
  }
  .pill-btn {
    width: 28px;
    height: 28px;
    border-radius: 7px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.75);
    transition: all 150ms;
    cursor: pointer;
  }
  .pill-btn:hover { color: #fff; }
  .pill-btn.liked { color: var(--accent); }

  /* Auto-hide pill in fullscreen/expanded */
  .fullscreen .art-pill,
  .milkdrop-expanded .art-pill {
    transition: opacity 300ms ease;
  }
  .fullscreen.controls-hidden-mode .art-pill,
  .milkdrop-expanded.controls-hidden-mode .art-pill {
    opacity: 0;
    pointer-events: none;
  }


  /* --- Mini mode expand button --- */
  .mini-expand-btn {
    position: absolute;
    top: 4px;
    right: 8px;
    z-index: 10;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--glass-bg);
    backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    transition: transform 100ms ease, color 100ms ease;
    cursor: pointer;
  }
  .mini-expand-btn:hover {
    transform: scale(1.1);
    color: var(--text-primary);
  }

  /* --- Crossfade track wrappers --- */
  .cf-track {
    flex-shrink: 0;
    transition: opacity 300ms ease;
  }
  .cf-out {
    cursor: pointer;
  }
  .cf-out:hover {
    background: var(--bg-elevated);
  }

  /* --- Track Info --- */
  .track-info {
    height: 46px;
    padding: 0 16px;
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .info-left {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .track-title {
    font-size: 18px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .track-artist {
    font-size: 14px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .info-badges {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .badge-btn {
    width: 28px;
    height: 22px;
    border-radius: 10px;
    background: var(--bg-surface);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    cursor: pointer;
    transition: color 150ms ease;
    flex-shrink: 0;
  }
  .badge-btn:hover {
    color: var(--text-secondary);
  }
  .badge-btn.liked {
    color: var(--accent);
  }

  .bpm-badge {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-tertiary);
    background: var(--bg-surface);
    padding: 3px 8px;
    border-radius: 10px;
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  /* --- Transport --- */
  .transport {
    height: 48px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 16px 4px;
    flex-shrink: 0;
  }

  .transport-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    transition: transform 100ms ease, color 100ms ease;
  }

  .transport-btn:hover {
    transform: scale(1.1);
  }

  .transport-btn:active {
    transform: scale(0.95);
  }

  .transport-btn.side {
    width: 18px;
    height: 18px;
    color: var(--text-tertiary);
  }

  .transport-btn.side:hover {
    color: var(--text-primary);
  }

  .transport-btn.side.active {
    color: var(--accent);
  }

  .transport-btn.skip {
    width: 24px;
    height: 24px;
    color: var(--text-secondary);
  }

  .transport-btn.skip:hover {
    color: var(--text-primary);
  }

  .play-btn {
    width: 72px;
    height: 44px;
    border-radius: 12px;
    background: var(--text-primary);
    color: var(--bg);
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.3);
  }

  .play-btn:hover {
    transform: scale(1.05);
  }

  .play-btn:active {
    transform: scale(0.97);
  }

  /* --- Mini mode overrides --- */
  .mini-mode .track-info {
    height: 28px;
    padding: 0 16px;
  }
  .mini-mode .track-title {
    font-size: 14px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mini-mode .info-left {
    flex-direction: row;
    gap: 0;
  }

  .mini-mode .transport {
    height: 44px;
    padding-bottom: 0;
  }
  .mini-mode .play-btn {
    width: 36px;
    height: 36px;
  }
  .mini-mode .play-btn svg {
    width: 20px;
    height: 20px;
  }
  .mini-mode .transport-btn.skip svg {
    width: 18px;
    height: 18px;
  }
  .mini-mode .transport-btn.side svg {
    width: 14px;
    height: 14px;
  }
</style>
