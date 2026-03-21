<script lang="ts">
  import {
    getCurrentTrack, getCoverUrl, getStatus,
    getPosition, getDuration, playPause,
  } from '../stores/player.svelte';

  let { onTap }: { onTap?: () => void } = $props();

  let track = $derived(getCurrentTrack());
  let coverUrl = $derived(getCoverUrl());
  let status = $derived(getStatus());
  let position = $derived(getPosition());
  let duration = $derived(getDuration());
  let progress = $derived(duration > 0 ? (position / duration) * 100 : 0);
</script>

{#if track}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="mini-player" onclick={onTap}>
    <div class="progress-bar">
      <div class="progress-fill" style="width: {progress}%"></div>
    </div>
    <div class="content">
      {#if coverUrl}
        <img src={coverUrl} alt="" class="mini-art" />
      {:else}
        <div class="mini-art-placeholder">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M9 18V5l12-2v13"/>
            <circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>
          </svg>
        </div>
      {/if}
      <div class="info">
        <span class="title">{track.title || 'Unknown'}</span>
        <span class="artist">{track.artist || 'Unknown Artist'}</span>
      </div>
      <button
        class="play-btn"
        onclick={(e) => { e.stopPropagation(); playPause(); }}
        title={status === 'playing' ? 'Pause' : 'Play'}
      >
        {#if status === 'playing'}
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" stroke="none">
            <rect x="14" y="4" width="4" height="16" rx="1"/>
            <rect x="6" y="4" width="4" height="16" rx="1"/>
          </svg>
        {:else}
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" stroke="none">
            <polygon points="8 4 20 12 8 20"/>
          </svg>
        {/if}
      </button>
    </div>
  </div>
{/if}

<style>
  .mini-player {
    height: 56px;
    background: var(--bg-elevated);
    flex-shrink: 0;
    position: relative;
    cursor: pointer;
  }

  .progress-bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--progress-bg);
  }

  .progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 250ms linear;
  }

  .content {
    display: flex;
    align-items: center;
    height: 100%;
    padding: 0 12px 0 12px;
    gap: 10px;
  }

  .mini-art {
    width: 40px;
    height: 40px;
    border-radius: 4px;
    object-fit: cover;
    flex-shrink: 0;
  }

  .mini-art-placeholder {
    width: 40px;
    height: 40px;
    border-radius: 4px;
    background: var(--bg-surface);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    flex-shrink: 0;
  }

  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .artist {
    font-size: 11px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .play-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-primary);
    flex-shrink: 0;
    transition: transform 100ms ease;
  }

  .play-btn:hover {
    transform: scale(1.1);
  }

  .play-btn:active {
    transform: scale(0.95);
  }
</style>
