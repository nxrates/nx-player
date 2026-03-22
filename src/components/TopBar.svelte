<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import monoLogo from '../assets/nxp-mono.png';
  import { getVolume, setVolume, getMuted, toggleMute } from '../stores/player.svelte';

  let {
    onToggleQueue,
    onToggleLibrary,
    onToggleSettings,
  }: {
    onToggleQueue?: () => void;
    onToggleLibrary?: () => void;
    onToggleSettings?: () => void;
  } = $props();

  let volumeOpen = $state(false);
  let vol = $derived(getMuted() ? 0 : getVolume());

  function onVolumeInput(e: Event) {
    const v = parseFloat((e.target as HTMLInputElement).value);
    setVolume(v);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="topbar" data-tauri-drag-region>
  <!-- macOS traffic lights -->
  <div class="traffic-lights">
    <button class="tl tl-close" onclick={() => getCurrentWindow().close()} title="Close"></button>
    <button class="tl tl-minimize" onclick={() => getCurrentWindow().minimize()} title="Minimize"></button>
    <button class="tl tl-maximize" disabled title="Maximize"></button>
  </div>

  <!-- Logo + title -->
  <img class="app-logo" src={monoLogo} alt="NX Player" data-tauri-drag-region />
  <span class="app-title" data-tauri-drag-region>Player</span>

  <!-- Spacer -->
  <div class="spacer" data-tauri-drag-region></div>

  <!-- Right buttons: Queue, Search/Library, Volume, Settings -->
  <div class="right">
    <!-- Queue with icon + label -->
    <button class="icon-btn queue-btn" onclick={onToggleQueue} title="Queue">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 15V6"/><path d="M18.5 18a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z"/><path d="M12 12H3"/><path d="M16 6H3"/><path d="M12 18H3"/>
      </svg>
      <span class="btn-label">Q</span>
    </button>

    <!-- Search / Library -->
    <button class="icon-btn" onclick={onToggleLibrary} title="Library">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>
      </svg>
    </button>

    <!-- Volume -->
    <div class="volume-wrap"
      onmouseenter={() => volumeOpen = true}
      onmouseleave={() => volumeOpen = false}>
      <button class="icon-btn" onclick={() => toggleMute()} title="Volume">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          {#if vol === 0}
            <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
            <line x1="22" y1="9" x2="16" y2="15"/><line x1="16" y1="9" x2="22" y2="15"/>
          {:else if vol < 0.5}
            <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
            <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
          {:else}
            <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
            <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
            <path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>
          {/if}
        </svg>
      </button>
      {#if volumeOpen}
        <div class="volume-popover">
          <span class="vol-label">{Math.round(vol * 100)}%</span>
          <input type="range" min="0" max="2" step="0.02" value={vol}
            oninput={onVolumeInput} class="vol-slider" orient="vertical" />
        </div>
      {/if}
    </div>

    <!-- Settings -->
    <button class="icon-btn" onclick={onToggleSettings} title="Settings">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/>
        <circle cx="12" cy="12" r="3"/>
      </svg>
    </button>
  </div>
</div>

<style>
  .topbar {
    height: 40px;
    display: flex;
    align-items: center;
    background: var(--bg);
    flex-shrink: 0;
    padding: 0 10px;
    z-index: 100;
    gap: 6px;
  }

  .traffic-lights {
    display: flex;
    align-items: center;
    gap: 7px;
    padding-right: 4px;
    -webkit-app-region: no-drag;
  }

  .tl {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: none;
    padding: 0;
    cursor: pointer;
    transition: opacity 100ms;
  }
  .tl:hover { opacity: 0.8; }
  .tl:disabled { opacity: 0.4; cursor: default; }
  .tl-close { background: #FF5F57; }
  .tl-minimize { background: #FEBC2E; }
  .tl-maximize { background: #28C840; }

  .app-logo {
    width: 26px;
    height: 26px;
    border-radius: 4px;
    filter: brightness(0) invert(0.8);
    opacity: 1;
  }
  :global([data-theme="light"]) .app-logo {
    filter: none;
    opacity: 1;
  }

  .app-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .spacer {
    flex: 1;
  }

  .right {
    display: flex;
    align-items: center;
    gap: 2px;
    -webkit-app-region: no-drag;
  }

  .icon-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    transition: color 150ms ease;
    border-radius: 6px;
    flex-shrink: 0;
  }
  .icon-btn:hover { color: var(--text-primary); }

  .queue-btn {
    width: auto;
    gap: 3px;
    padding: 0 7px;
    border: 1px solid var(--border);
  }
  .btn-label {
    font-size: 11px;
    font-weight: 700;
  }

  .volume-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }
  .volume-popover,
  .volume-popover * {
    -webkit-app-region: no-drag;
  }
  .volume-popover {
    position: absolute;
    top: 32px;
    right: 0;
    width: 36px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px 8px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    z-index: 300;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .vol-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .vol-slider {
    writing-mode: vertical-lr;
    direction: rtl;
    width: 4px;
    height: 100px;
    min-height: 100px;
    max-height: 100px;
    flex-shrink: 0;
    appearance: none;
    -webkit-appearance: none;
    background: var(--progress-bg);
    border-radius: 2px;
    outline: none;
  }
  .vol-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-primary);
    cursor: pointer;
  }
</style>
