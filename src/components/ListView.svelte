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

  <div class="track-list">
    {#if filteredTracks.length === 0}
      <div class="empty">{emptyMessage}</div>
    {:else}
      {#each filteredTracks as track, i}
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
</style>
