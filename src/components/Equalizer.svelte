<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  const BANDS = [
    { label: 'Bass', freq: '80 Hz', band: 0 },
    { label: 'Mid', freq: '1 kHz', band: 1 },
    { label: 'Treble', freq: '12 kHz', band: 2 },
  ] as const;

  let gains = $state([0, 0, 0]);
  let enabled = $state(localStorage.getItem('nx-eq-enabled') !== 'false');

  // Restore saved gains
  try {
    const saved = localStorage.getItem('nx-eq-gains');
    if (saved) gains = JSON.parse(saved);
  } catch {}

  // Apply saved state on mount
  $effect(() => {
    if (enabled) {
      for (let i = 0; i < 3; i++) setBand(i, gains[i]);
    } else {
      invoke('audio_reset_eq').catch(() => {});
    }
  });

  function setBand(band: number, db: number) {
    gains[band] = db;
    localStorage.setItem('nx-eq-gains', JSON.stringify(gains));
    if (enabled) invoke('audio_set_eq_band', { band, gainDb: db }).catch(() => {});
  }

  function toggleEnabled() {
    enabled = !enabled;
    localStorage.setItem('nx-eq-enabled', String(enabled));
    if (!enabled) {
      invoke('audio_reset_eq').catch(() => {});
    } else {
      for (let i = 0; i < 3; i++) setBand(i, gains[i]);
    }
  }

  function reset() {
    gains = [0, 0, 0];
    localStorage.setItem('nx-eq-gains', JSON.stringify(gains));
    invoke('audio_reset_eq').catch(() => {});
  }
</script>

<div class="eq">
  <div class="eq-header">
    <button class="toggle" class:on={enabled} onclick={toggleEnabled}>
      <div class="toggle-thumb"></div>
    </button>
    <button class="reset-btn" onclick={reset} disabled={gains.every(g => g === 0)}>Reset</button>
  </div>
  <div class="eq-bands" class:disabled={!enabled}>
    {#each BANDS as { label, freq, band }}
      <div class="eq-band">
        <span class="eq-db">{gains[band] > 0 ? '+' : ''}{gains[band].toFixed(1)}</span>
        <div class="eq-slider-wrap">
          <input
            type="range"
            class="eq-slider"
            min="-12"
            max="12"
            step="0.5"
            value={gains[band]}
            oninput={(e) => setBand(band, parseFloat((e.target as HTMLInputElement).value))}
            disabled={!enabled}
          />
        </div>
        <span class="eq-label">{label}</span>
        <span class="eq-freq">{freq}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .eq-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 16px;
    min-height: 48px;
    border-bottom: 1px solid var(--border);
  }

  .reset-btn {
    font-size: 13px;
    color: var(--accent);
    font-weight: 500;
    padding: 4px 10px;
    border-radius: 6px;
  }
  .reset-btn:hover { background: var(--accent-dim); }
  .reset-btn:disabled { opacity: 0.3; pointer-events: none; }

  .eq-bands {
    display: flex;
    justify-content: center;
    gap: 24px;
    padding: 16px 16px 12px;
    transition: opacity 200ms;
  }
  .eq-bands.disabled { opacity: 0.35; pointer-events: none; }

  .eq-band {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    flex: 1;
    max-width: 80px;
  }

  .eq-db {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
    min-width: 36px;
    text-align: center;
  }

  .eq-slider-wrap {
    height: 100px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .eq-slider {
    writing-mode: vertical-lr;
    direction: rtl;
    -webkit-appearance: slider-vertical;
    appearance: slider-vertical;
    width: 4px;
    height: 100px;
    background: var(--bg-surface);
    border-radius: 2px;
    outline: none;
  }

  .eq-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text-primary);
    cursor: pointer;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
  }

  .eq-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .eq-freq {
    font-size: 10px;
    color: var(--text-tertiary);
  }

  /* Reuse toggle styles from parent */
  .toggle {
    width: 44px;
    height: 26px;
    border-radius: 13px;
    background: var(--bg-surface);
    position: relative;
    transition: background 200ms ease;
    flex-shrink: 0;
  }
  .toggle.on { background: var(--accent); }
  .toggle-thumb {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: white;
    position: absolute;
    top: 2px;
    left: 2px;
    transition: transform 200ms ease;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  }
  .toggle.on .toggle-thumb { transform: translateX(18px); }
</style>
