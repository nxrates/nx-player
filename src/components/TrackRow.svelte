<script lang="ts">
  import type { Track } from '../lib/types';
  import { formatDuration } from '../lib/format';

  let { track, isPlaying = false, showRemove = false, onclick, onRemove }: {
    track: Track;
    isPlaying?: boolean;
    showRemove?: boolean;
    onclick?: () => void;
    onRemove?: () => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="track-row" class:active={isPlaying} onclick={onclick} role="button" tabindex="0" onkeydown={(e) => { if (e.key === 'Enter') onclick?.(); }}>
  <div class="art-placeholder">
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <path d="M9 18V5l12-2v13"/>
      <circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>
    </svg>
  </div>
  <div class="info">
    <span class="title">{track.title || 'Unknown'}</span>
    <span class="artist">{track.artist || 'Unknown Artist'}</span>
  </div>
  {#if showRemove}
    <button class="remove-btn" onclick={(e) => { e.stopPropagation(); onRemove?.(); }} title="Remove">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M18 6 6 18"/><path d="m6 6 12 12"/>
      </svg>
    </button>
  {:else}
    <span class="duration">{formatDuration(track.duration)}</span>
  {/if}
</div>

<style>
  .track-row {
    display: flex;
    align-items: center;
    height: 56px;
    padding: 0 20px;
    gap: 12px;
    width: 100%;
    text-align: left;
    cursor: pointer;
    border-bottom: 1px solid var(--border);
    transition: background 100ms ease;
    position: relative;
  }

  .track-row:hover {
    background: var(--bg-surface);
  }

  .track-row.active {
    border-left: 3px solid var(--accent);
    padding-left: 17px;
  }

  .art-placeholder {
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
    gap: 2px;
  }

  .title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .artist {
    font-size: 12px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .duration {
    font-size: 12px;
    color: var(--text-tertiary);
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  .remove-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    border-radius: 6px;
    flex-shrink: 0;
    transition: color 100ms ease;
  }

  .remove-btn:hover {
    color: var(--accent);
  }
</style>
