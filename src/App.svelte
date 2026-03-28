<script lang="ts">
  import TopBar from './components/TopBar.svelte';
  import NowPlaying from './components/NowPlaying.svelte';
  import ListView from './components/ListView.svelte';
  import MiniPlayer from './components/MiniPlayer.svelte';
  import Settings from './components/Settings.svelte';
  import { loadTracks, loadFolders, initScanListener, getTracks } from './stores/library.svelte';
  import {
    playPause, next, prev, seek, playTrack,
    getPosition, getDuration, getQueue, removeFromQueue, clearQueue,
    getCurrentTrack, selectRandom, getCoverUrl,
  } from './stores/player.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { Track } from './lib/types';

  let activeView = $state<'nowplaying' | 'library' | 'queue' | 'settings'>('nowplaying');
  let globalCoverUrl = $derived(getCoverUrl());

  // Run once on mount — not reactive (no reactive deps read inside)
  let initialized = false;
  $effect(() => {
    if (initialized) return;
    initialized = true;

    loadTracks();
    loadFolders();
    initScanListener();

    // Restore window size based on saved view mode
    const mode = localStorage.getItem('nx-view-mode');
    if (mode === 'mini') {
      import('@tauri-apps/api/dpi').then(({ LogicalSize }) => {
        const win = getCurrentWindow();
        win.setMinSize(new LogicalSize(360, 140)).then(() => {
          win.setSize(new LogicalSize(360, 160));
        });
      });
    }
  });

  // After tracks load, auto-select a random track if nothing is playing
  $effect(() => {
    const tracks = getTracks();
    const current = getCurrentTrack();
    if (tracks.length > 0 && !current) {
      selectRandom(tracks);
    }
  });

  function handleTrackPlay(track: Track, _index: number) {
    playTrack(track, getTracks());
    activeView = 'nowplaying';
  }

  function handleQueuePlay(track: Track, _index: number) {
    playTrack(track);
    activeView = 'nowplaying';
  }

  function handleKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const isInput = target.tagName === 'INPUT' || target.tagName === 'TEXTAREA';

    if (e.key === 'Escape') {
      if (activeView !== 'nowplaying') {
        activeView = 'nowplaying';
        return;
      }
      return;
    }

    if (e.key === ' ' && !isInput) {
      e.preventDefault();
      playPause();
    } else if (e.key === 'ArrowLeft' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      prev();
    } else if (e.key === 'ArrowRight' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      next();
    } else if (e.key === 'ArrowLeft' && !isInput) {
      e.preventDefault();
      const pos = getPosition();
      seek(Math.max(0, pos - 10));
    } else if (e.key === 'ArrowRight' && !isInput) {
      e.preventDefault();
      const pos = getPosition();
      const dur = getDuration();
      seek(Math.min(dur, pos + 10));
    } else if ((e.metaKey || e.ctrlKey) && e.key === 'l') {
      e.preventDefault();
      activeView = activeView === 'library' ? 'nowplaying' : 'library';
      if (activeView === 'library') loadTracks();
    } else if (e.key === 'm' && !isInput && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      document.dispatchEvent(new CustomEvent('toggle-mini-mode'));
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="app-root">
  <!-- Global album art tint: covers the entire app from header to bottom -->
  {#if globalCoverUrl}
    <div class="global-blur" style="background-image: url({globalCoverUrl})"></div>
  {/if}

  <TopBar
    onToggleQueue={() => { activeView = activeView === 'queue' ? 'nowplaying' : 'queue'; }}
    onToggleLibrary={() => { activeView = activeView === 'library' ? 'nowplaying' : 'library'; if (activeView === 'library') loadTracks(); }}
    onToggleSettings={() => { activeView = activeView === 'settings' ? 'nowplaying' : 'settings'; }}
    onToggleFullscreen={() => document.dispatchEvent(new CustomEvent('toggle-fullscreen'))}
  />

  {#if activeView === 'nowplaying'}
    <NowPlaying />
  {:else if activeView === 'library'}
    <ListView
      tracks={getTracks()}
      title="Library"
      showSearch
      onTrackPlay={handleTrackPlay}
    />
    <MiniPlayer onTap={() => { activeView = 'nowplaying'; }} />
  {:else if activeView === 'queue'}
    <ListView
      tracks={getQueue()}
      title="Queue"
      isQueue
      showClear
      emptyMessage="Queue is empty"
      onTrackPlay={handleQueuePlay}
      onTrackRemove={(i) => removeFromQueue(i)}
      onClear={() => clearQueue()}
    />
    <MiniPlayer onTap={() => { activeView = 'nowplaying'; }} />
  {:else if activeView === 'settings'}
    <Settings onBack={() => { activeView = 'nowplaying'; }} />
  {/if}
</div>

<style>
  .app-root {
    width: 100%;
    height: 100vh;
    max-width: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    color: var(--text-primary);
    border-radius: 12px;
    overflow: hidden;
    position: relative;
  }

  /* Global album art tint — covers the ENTIRE app window */
  .global-blur {
    position: absolute;
    inset: -40px;
    background-size: cover;
    background-position: center;
    filter: blur(80px) saturate(1.5);
    opacity: 0.35;
    pointer-events: none;
    z-index: 0;
  }

  :global(:fullscreen) .app-root,
  :global(:-webkit-full-screen) .app-root {
    max-width: 100%;
    border-radius: 0;
  }
</style>
