<script lang="ts">
  import type { Track } from '../lib/types';
  import TrackRow from './TrackRow.svelte';
  import { getCurrentTrack, getStatus } from '../stores/player.svelte';

  let {
    tracks,
    title,
    showSearch = false,
    emptyMessage = 'No tracks',
    showClear = false,
    isQueue = false,
    onTrackPlay,
    onTrackRemove,
    onClear,
  }: {
    tracks: Track[];
    title: string;
    showSearch?: boolean;
    emptyMessage?: string;
    showClear?: boolean;
    isQueue?: boolean;
    onTrackPlay?: (track: Track, index: number) => void;
    onTrackRemove?: (index: number) => void;
    onClear?: () => void;
  } = $props();

  let searchQuery = $state('');
  let sourceFilter = $state<string>('all');
  let formatFilter = $state<string>('all');

  let currentTrack = $derived(getCurrentTrack());
  let status = $derived(getStatus());

  let filteredTracks = $derived(
    searchQuery
      ? tracks.filter(t =>
          t.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
          t.artist.toLowerCase().includes(searchQuery.toLowerCase()) ||
          t.album.toLowerCase().includes(searchQuery.toLowerCase())
        )
      : tracks
  );

  let sources = $derived([...new Set(tracks.map(t => t.source || 'local'))]);

  // Extract format once per track (avoids repeated split('.').pop() in filter + format list)
  function getFormat(path: string): string {
    return path.split('.').pop()?.toLowerCase() || '';
  }
  let formats = $derived([...new Set(tracks.map(t => getFormat(t.path)))].filter(Boolean));

  let displayTracks = $derived(
    filteredTracks.filter(t => {
      if (sourceFilter !== 'all' && (t.source || 'local') !== sourceFilter) return false;
      if (formatFilter !== 'all' && getFormat(t.path) !== formatFilter) return false;
      return true;
    })
  );
</script>

<div class="list-view">
  <div class="header">
    <span class="title">{title}</span>
    {#if showClear && tracks.length > 0}
      <button class="clear-btn" onclick={() => onClear?.()}>Clear</button>
    {/if}
  </div>

  {#if showSearch}
    <div class="search-wrap">
      <input
        type="text"
        class="search-input"
        placeholder="Search..."
        bind:value={searchQuery}
      />
    </div>
  {/if}

  {#if sources.length > 1 || formats.length > 1}
    <div class="filter-bar">
      {#if sources.length > 1}
        <div class="filter-group">
          <button class="filter-pill" class:active={sourceFilter === 'all'} onclick={() => sourceFilter = 'all'}>All</button>
          {#each sources as s}
            <button class="filter-pill" class:active={sourceFilter === s} onclick={() => sourceFilter = s}>{s}</button>
          {/each}
        </div>
      {/if}
      {#if formats.length > 1}
        <div class="filter-group">
          <button class="filter-pill" class:active={formatFilter === 'all'} onclick={() => formatFilter = 'all'}>All</button>
          {#each formats as f}
            <button class="filter-pill" class:active={formatFilter === f} onclick={() => formatFilter = f}>.{f}</button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <div class="track-list">
    {#if displayTracks.length === 0}
      <div class="empty">{emptyMessage}</div>
    {:else}
      {#each displayTracks as track, i}
        <TrackRow
          {track}
          isPlaying={currentTrack?.id === track.id && status !== 'stopped'}
          showRemove={isQueue}
          onclick={() => onTrackPlay?.(track, i)}
          onRemove={() => onTrackRemove?.(i)}
        />
      {/each}
    {/if}
  </div>
</div>

<style>
  .list-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .header {
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 20px;
    flex-shrink: 0;
    position: relative;
  }

  .header .title {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .clear-btn {
    position: absolute;
    right: 20px;
    font-size: 13px;
    color: var(--accent);
    font-weight: 500;
  }

  .clear-btn:hover {
    opacity: 0.8;
  }

  .search-wrap {
    padding: 0 20px 8px;
    flex-shrink: 0;
  }

  .search-input {
    width: 100%;
    height: 32px;
    padding: 0 12px;
    font-size: 13px;
    color: var(--text-primary);
    background: var(--bg-surface);
    border-radius: 8px;
  }

  .search-input::placeholder {
    color: var(--text-tertiary);
  }

  .track-list {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
  }

  .empty {
    padding: 48px 20px;
    text-align: center;
    font-size: 14px;
    color: var(--text-tertiary);
  }

  .filter-bar {
    display: flex;
    gap: 6px;
    padding: 4px 16px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .filter-group {
    display: flex;
    gap: 2px;
  }
  .filter-pill {
    font-size: 10px;
    padding: 2px 8px;
    border-radius: 10px;
    color: var(--text-tertiary);
    background: var(--bg-surface);
    transition: all 150ms;
  }
  .filter-pill.active {
    color: var(--text-primary);
    background: var(--accent-dim);
  }
  .filter-pill:hover {
    color: var(--text-secondary);
  }
</style>
