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
      await win.setSize(new LogicalSize(360, 640));
      await win.setMinSize(new LogicalSize(360, 640));
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
      milkdropExpanded = true;
      await win.setFullscreen(true);
      isFullscreen = true;
      resetControlsTimer();
    }
  }

  // Escape key exits fullscreen
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
    return () => document.removeEventListener('keydown', handler, true);
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
          <div class="art-blur" style="background-image: url({coverUrl})"></div>
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

    <!-- Mode bar (floating top-right) -->
    <div class="mode-bar">
      <button class="mode-btn" class:active={viewMode === 'normal'} onclick={() => setViewMode('normal')} title="Album Art">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="3" width="18" height="18" rx="2"/>
          <circle cx="8.5" cy="8.5" r="1.5"/>
          <path d="m21 15-5-5L5 21"/>
        </svg>
      </button>
      <button class="mode-btn" class:active={viewMode === 'milkdrop'} onclick={() => setViewMode('milkdrop')} title="MilkDrop Visualizer">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2"/>
        </svg>
      </button>
      <button class="mode-btn" onclick={() => setViewMode('mini')} title="Mini Mode (M)">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M4 14h6v6H4z"/>
          <path d="M14 4h6v6h-6z"/>
        </svg>
      </button>
      {#if viewMode === 'milkdrop'}
        <button class="mode-btn" onclick={toggleFullscreen} title={isFullscreen ? 'Exit Fullscreen' : 'Fullscreen'}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            {#if isFullscreen}
              <path d="M8 3v3a2 2 0 0 1-2 2H3m18 0h-3a2 2 0 0 1-2-2V3m0 18v-3a2 2 0 0 1 2-2h3M3 16h3a2 2 0 0 1 2 2v3"/>
            {:else}
              <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/>
            {/if}
          </svg>
        </button>
      {/if}
    </div>

  {/if}

  <!-- Controls overlay (glass effect in normal/milkdrop, solid in mini) -->
  <div class="controls-overlay" class:glass={viewMode !== 'mini'}>
    <!-- Crossfade: outgoing track (fading out — RED tint, same layout as current) -->
    {#if outgoingTrack}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="cf-track cf-out" style="opacity: {Math.max(0.1, 1 - cfProgress)}" onclick={handleOutgoingClick}>
        <Waveform overrideTrackId={outgoingTrack.id} overridePosition={ogPos} overrideDuration={ogDur} tint="red" />
        <div class="track-info">
          <div class="info-left">
            <span class="track-title">{outgoingTrack.title}</span>
            <span class="track-artist">{outgoingTrack.artist || 'Unknown'}</span>
          </div>
          {#if outgoingTrack.bpm}
            <span class="bpm-badge">{Math.round(outgoingTrack.bpm)} BPM</span>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Current track (fading in — GREEN tint during crossfade, normal otherwise) -->
    <div class="cf-track cf-in" style="opacity: {outgoingTrack ? Math.max(0.3, cfProgress) : 1}">
      <Waveform tint={outgoingTrack ? 'green' : undefined} />
      <div class="track-info">
        <div class="info-left">
          {#if viewMode === 'mini'}
            <span class="track-title">{track?.title || 'No Track'}{track?.artist ? ' \u2014 ' + track.artist : ''}</span>
          {:else}
            <span class="track-title">{track?.title || 'No Track'}</span>
            <span class="track-artist">{track?.artist || 'Unknown Artist'}</span>
          {/if}
        </div>
        <div class="info-badges">
          <button class="badge-btn" class:liked onclick={() => liked = !liked}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill={liked ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>
            </svg>
          </button>
          {#if track?.bpm}
            <span class="bpm-badge">{Math.round(track.bpm)} BPM</span>
          {/if}
        </div>
      </div>
    </div>

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
    /* Reserve space at bottom so art doesn't hide behind glass controls */
    padding-bottom: 200px;
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
    filter: blur(60px) saturate(1.8);
    opacity: 0.3;
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
    padding: 8px 0 12px;
  }

  .controls-overlay.glass {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    background: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .mini-mode .controls-overlay {
    position: relative;
    background: var(--bg);
  }

  /* MilkDrop expanded / fullscreen: auto-hide controls */
  .milkdrop-expanded .controls-overlay,
  .fullscreen .controls-overlay {
    transition: opacity 300ms ease;
  }
  .milkdrop-expanded.controls-hidden-mode .controls-overlay,
  .fullscreen.controls-hidden-mode .controls-overlay {
    opacity: 0;
    pointer-events: none;
  }

  /* --- Mode bar (floating top-right) --- */
  .mode-bar {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 10;
    display: flex;
    gap: 2px;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(8px);
    border-radius: 8px;
    padding: 3px;
  }
  .mode-btn {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.5);
    transition: all 150ms;
    cursor: pointer;
  }
  .mode-btn:hover { color: rgba(255, 255, 255, 0.8); }

  /* Auto-hide mode bar in fullscreen */
  .fullscreen .mode-bar,
  .milkdrop-expanded .mode-bar {
    transition: opacity 300ms ease;
  }
  .fullscreen.controls-hidden-mode .mode-bar,
  .milkdrop-expanded.controls-hidden-mode .mode-bar {
    opacity: 0;
    pointer-events: none;
  }
  .mode-btn.active {
    background: rgba(255, 255, 255, 0.15);
    color: white;
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
    background: rgba(0, 0, 0, 0.3);
    backdrop-filter: blur(8px);
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.6);
    transition: transform 100ms ease, color 100ms ease;
    cursor: pointer;
  }
  .mini-expand-btn:hover {
    transform: scale(1.1);
    color: rgba(255, 255, 255, 0.9);
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
    background: rgba(255, 255, 255, 0.03);
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
    padding: 0 16px;
    flex-shrink: 0;
    padding-bottom: 12px;
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
