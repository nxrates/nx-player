<script lang="ts">
  import { getTheme, setTheme } from '../stores/ui.svelte';
  import { getFolders, loadFolders, startScan, getScanStatus, getScanProgress } from '../stores/library.svelte';
  import { getPlaybackRate, setPlaybackRate, getCurrentTrack } from '../stores/player.svelte';
  import { addFolder, listExtensions, installExtension, uninstallExtension, startExtension, stopExtension } from '../lib/ipc';
  import Equalizer from './Equalizer.svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import type { Extension } from '../lib/types';

  let { onBack }: { onBack?: () => void } = $props();

  let theme = $derived(getTheme());
  let folders = $derived(getFolders());
  let scanStatus = $derived(getScanStatus());
  let scanProgress = $derived(getScanProgress());
  let playbackRate = $derived(getPlaybackRate());
  let track = $derived(getCurrentTrack());

  let vizFps = $state(parseInt(localStorage.getItem('nx-viz-fps') || '60'));
  let vizScale = $state(parseFloat(localStorage.getItem('nx-viz-scale') || '1'));
  let vizCycleInterval = $state(parseInt(localStorage.getItem('nx-viz-cycle') || '20'));
  let showPresetList = $state(false);
  let presetList = $state<string[]>([]);
  let selectedPreset = $state(localStorage.getItem('nx-viz-preset') || '');

  function setVizFps(f: number) {
    vizFps = f;
    localStorage.setItem('nx-viz-fps', String(f));
  }

  function setVizScale(s: number) {
    vizScale = s;
    localStorage.setItem('nx-viz-scale', String(s));
  }

  function setVizCycleInterval(val: number) {
    vizCycleInterval = val;
    localStorage.setItem('nx-viz-cycle', String(val));
  }

  function selectPreset(name: string) {
    selectedPreset = name;
    localStorage.setItem('nx-viz-preset', name);
    showPresetList = false;
  }

  async function loadPresetList() {
    // Dynamically import butterchurn-presets to get the list
    try {
      const presetsMod = await import('butterchurn-presets');
      const presets = presetsMod.default.getPresets();
      presetList = Object.keys(presets).sort();
    } catch {
      presetList = [];
    }
  }

  function togglePresetList() {
    showPresetList = !showPresetList;
    if (showPresetList && presetList.length === 0) {
      loadPresetList();
    }
  }

  let extensions = $state<Extension[]>([]);
  let extensionsLoaded = false;

  $effect(() => {
    if (extensionsLoaded) return;
    extensionsLoaded = true;
    loadExts();
  });

  async function loadExts() {
    try { extensions = await listExtensions(); } catch {}
  }
  async function startExt(id: string) { await startExtension(id); await loadExts(); }
  async function stopExt(id: string) { await stopExtension(id); await loadExts(); }
  async function removeExt(id: string) {
    if (confirm('Remove this extension?')) { await uninstallExtension(id); await loadExts(); }
  }
  async function installExt() {
    const selected = await open({ directory: true, multiple: false });
    if (selected && typeof selected === 'string') {
      await installExtension(selected); await loadExts();
    }
  }
  function getExtSetting(extId: string, key: string): string {
    return localStorage.getItem(`ext-${extId}-${key}`) || '';
  }
  function setExtSetting(extId: string, key: string, value: string) {
    localStorage.setItem(`ext-${extId}-${key}`, value);
  }
  function toggleExtSetting(extId: string, key: string) {
    const current = getExtSetting(extId, key);
    setExtSetting(extId, key, current === 'true' ? 'false' : 'true');
  }

  let automix = $state(localStorage.getItem('ls-automix') === 'true');
  let crossfade = $state(localStorage.getItem('ls-crossfade') === 'true');
  let crossfadeDur = $state(parseFloat(localStorage.getItem('ls-crossfade-dur') ?? '8'));
  let matchBpm = $state(localStorage.getItem('ls-match-bpm') === 'true');

  function toggleAutomix() {
    automix = !automix;
    localStorage.setItem('ls-automix', String(automix));
  }

  function toggleCrossfade() {
    crossfade = !crossfade;
    localStorage.setItem('ls-crossfade', String(crossfade));
  }

  function setCrossfadeDur(val: number) {
    crossfadeDur = val;
    localStorage.setItem('ls-crossfade-dur', String(val));
  }

  function toggleMatchBpm() {
    matchBpm = !matchBpm;
    localStorage.setItem('ls-match-bpm', String(matchBpm));
  }

  async function changeFolder() {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (selected) {
        await addFolder(selected as string);
        await loadFolders();
      }
    } catch { /* cancelled */ }
  }

  function rescan() {
    if (folders.length > 0) {
      startScan(folders);
    }
  }

  let resultingBpm = $derived(
    track?.bpm ? Math.round(track.bpm * playbackRate) : null
  );
</script>

<div class="settings">
  <div class="back-bar">
    <button class="back-btn" onclick={onBack}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="m15 18-6-6 6-6"/>
      </svg>
      Back
    </button>
  </div>

  <div class="scroll-area">
    <!-- Appearance -->
    <div class="section-header">Appearance</div>
    <div class="row">
      <span class="row-label">Theme</span>
      <div class="segmented">
        <button class:active={theme === 'light'} onclick={() => setTheme('light')}>Light</button>
        <button class:active={theme === 'dark'} onclick={() => setTheme('dark')}>Dark</button>
        <button class:active={theme === 'system'} onclick={() => setTheme('system')}>System</button>
      </div>
    </div>

    <!-- Library -->
    <div class="section-header">Library</div>
    <div class="row">
      <div class="folder-info">
        {#if folders.length > 0}
          <span class="folder-path">{folders[0]}</span>
        {:else}
          <span class="folder-path empty">No folder selected</span>
        {/if}
      </div>
      <button class="action-btn" onclick={changeFolder}>Change</button>
    </div>
    <div class="row">
      <span class="row-label">Rescan Library</span>
      <button
        class="action-btn"
        onclick={rescan}
        disabled={scanStatus === 'scanning' || folders.length === 0}
      >
        {#if scanStatus === 'scanning'}
          {scanProgress ? `${scanProgress.current}/${scanProgress.total}` : 'Scanning...'}
        {:else}
          Rescan
        {/if}
      </button>
    </div>

    <!-- Playback -->
    <div class="section-header">Playback</div>
    <div class="row column">
      <div class="slider-header">
        <span class="row-label">Speed: {playbackRate.toFixed(2)}x</span>
        {#if resultingBpm}
          <span class="bpm-result">{resultingBpm} BPM</span>
        {/if}
      </div>
      <input
        type="range"
        class="slider"
        min="0.5"
        max="2"
        step="0.05"
        value={playbackRate}
        oninput={(e) => setPlaybackRate(parseFloat((e.target as HTMLInputElement).value))}
      />
    </div>

    <!-- Equalizer -->
    <div class="section-header">Equalizer</div>
    <Equalizer />

    <!-- Visualizer -->
    <div class="section-header">Visualizer</div>
    <div class="row column">
      <div class="slider-header">
        <span class="row-label">Frame Rate: {vizFps} fps</span>
      </div>
      <input
        type="range"
        class="slider"
        min="15"
        max="120"
        step="5"
        value={vizFps}
        oninput={(e) => setVizFps(parseInt((e.target as HTMLInputElement).value))}
      />
    </div>
    <div class="row column">
      <div class="slider-header">
        <span class="row-label">Pixel Scale: {vizScale.toFixed(1)}x</span>
      </div>
      <input
        type="range"
        class="slider"
        min="0.5"
        max="2"
        step="0.25"
        value={vizScale}
        oninput={(e) => setVizScale(parseFloat((e.target as HTMLInputElement).value))}
      />
    </div>
    <div class="row column">
      <div class="slider-header">
        <span class="row-label">Auto-Cycle: {vizCycleInterval}s</span>
      </div>
      <input
        type="range"
        class="slider"
        min="10"
        max="60"
        step="5"
        value={vizCycleInterval}
        oninput={(e) => setVizCycleInterval(parseInt((e.target as HTMLInputElement).value))}
      />
    </div>
    <div class="row">
      <span class="row-label">Choose Preset</span>
      <button class="action-btn" onclick={togglePresetList}>
        {showPresetList ? 'Hide' : 'Browse'}
      </button>
    </div>
    {#if showPresetList}
      <div class="preset-list">
        {#each presetList as name}
          <button
            class="preset-item"
            class:selected={selectedPreset === name}
            onclick={() => selectPreset(name)}
          >
            {name}
          </button>
        {/each}
        {#if presetList.length === 0}
          <div class="preset-loading">Loading presets...</div>
        {/if}
      </div>
    {/if}

    <!-- Auto-Mix -->
    <div class="section-header">Auto-Mix</div>
    <div class="row">
      <span class="row-label">Enable Auto-Mix</span>
      <button class="toggle" class:on={automix} onclick={toggleAutomix}>
        <div class="toggle-thumb"></div>
      </button>
    </div>
    <div class="row">
      <span class="row-label">Crossfade</span>
      <button class="toggle" class:on={crossfade} onclick={toggleCrossfade}>
        <div class="toggle-thumb"></div>
      </button>
    </div>
    {#if crossfade}
      <div class="row column">
        <div class="slider-header">
          <span class="row-label">Duration: {crossfadeDur.toFixed(0)}s</span>
        </div>
        <input
          type="range"
          class="slider"
          min="0"
          max="30"
          step="1"
          value={crossfadeDur}
          oninput={(e) => setCrossfadeDur(parseFloat((e.target as HTMLInputElement).value))}
        />
      </div>
    {/if}
    <div class="row">
      <span class="row-label">Match BPM</span>
      <button class="toggle" class:on={matchBpm} onclick={toggleMatchBpm}>
        <div class="toggle-thumb"></div>
      </button>
    </div>

    <!-- Extensions -->
    <div class="section-header">Extensions</div>
    {#each extensions as ext}
      <div class="row">
        <span class="row-label">{ext.name}</span>
        <div style="display:flex;gap:4px;align-items:center">
          <button class="toggle" class:on={ext.running} onclick={() => ext.running ? stopExt(ext.id) : startExt(ext.id)}>
            <div class="toggle-thumb"></div>
          </button>
          <button class="action-btn" onclick={() => removeExt(ext.id)}>Remove</button>
        </div>
      </div>
      {#if ext.running && ext.settings.length > 0}
        {#each ext.settings as setting}
          <div class="row">
            <span class="row-label" style="padding-left:16px">{setting.label}</span>
            {#if setting.type === 'toggle'}
              <button class="toggle" class:on={getExtSetting(ext.id, setting.key) === 'true'} onclick={() => toggleExtSetting(ext.id, setting.key)}>
                <div class="toggle-thumb"></div>
              </button>
            {:else}
              <input type={setting.type === 'password' ? 'password' : 'text'}
                value={getExtSetting(ext.id, setting.key) || setting.default || ''}
                onchange={(e) => setExtSetting(ext.id, setting.key, (e.target as HTMLInputElement).value)}
                class="ext-input" />
            {/if}
          </div>
        {/each}
      {/if}
    {/each}
    {#if extensions.length === 0}
      <div class="row"><span class="row-label" style="color:var(--text-tertiary)">No extensions installed</span></div>
    {/if}
    <div class="row">
      <button class="action-btn" onclick={installExt}>Install Extension...</button>
    </div>
  </div>
</div>

<style>
  .settings {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .back-bar {
    height: 44px;
    display: flex;
    align-items: center;
    padding: 0 12px;
    flex-shrink: 0;
  }

  .back-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 14px;
    color: var(--accent);
    font-weight: 500;
  }

  .scroll-area {
    flex: 1;
    overflow-y: auto;
    padding: 0 0 20px;
  }

  .section-header {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.8px;
    color: var(--text-tertiary);
    padding: 16px 16px 6px;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 48px;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
  }

  .row.column {
    flex-direction: column;
    align-items: stretch;
    padding: 10px 16px;
    gap: 8px;
  }

  .row-label {
    font-size: 14px;
    color: var(--text-primary);
  }

  .folder-info {
    flex: 1;
    min-width: 0;
    margin-right: 8px;
  }

  .folder-path {
    font-size: 12px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: block;
  }

  .folder-path.empty {
    color: var(--text-tertiary);
  }

  .action-btn {
    font-size: 13px;
    color: var(--accent);
    font-weight: 500;
    padding: 4px 10px;
    border-radius: 6px;
    flex-shrink: 0;
  }

  .action-btn:hover {
    background: var(--accent-dim);
  }

  .action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Preset list */
  .preset-list {
    max-height: 240px;
    overflow-y: auto;
    border-bottom: 1px solid var(--border);
  }
  .preset-item {
    display: block;
    width: 100%;
    text-align: left;
    font-size: 12px;
    padding: 8px 16px;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    transition: background 100ms;
  }
  .preset-item:hover {
    background: var(--bg-surface);
    color: var(--text-primary);
  }
  .preset-item.selected {
    color: var(--accent);
    font-weight: 600;
  }
  .preset-loading {
    padding: 16px;
    text-align: center;
    font-size: 12px;
    color: var(--text-tertiary);
  }

  /* Segmented control */
  .segmented {
    display: flex;
    background: var(--bg-surface);
    border-radius: 8px;
    padding: 2px;
    gap: 1px;
  }

  .segmented button {
    font-size: 12px;
    font-weight: 500;
    padding: 5px 12px;
    border-radius: 6px;
    color: var(--text-secondary);
    transition: all 100ms ease;
  }

  .segmented button.active {
    background: var(--bg-elevated);
    color: var(--text-primary);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.15);
  }

  /* Toggle */
  .toggle {
    width: 44px;
    height: 26px;
    border-radius: 13px;
    background: var(--bg-surface);
    position: relative;
    transition: background 200ms ease;
    flex-shrink: 0;
  }

  .toggle.on {
    background: var(--accent);
  }

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

  .toggle.on .toggle-thumb {
    transform: translateX(18px);
  }

  /* Slider */
  .slider-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .bpm-result {
    font-size: 12px;
    color: var(--text-tertiary);
    font-variant-numeric: tabular-nums;
  }

  .slider {
    width: 100%;
    height: 4px;
    -webkit-appearance: none;
    appearance: none;
    background: var(--bg-surface);
    border-radius: 2px;
    outline: none;
  }

  .ext-input {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
    font-size: 12px;
    width: 120px;
    color: var(--text-primary);
  }

  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text-primary);
    cursor: pointer;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
  }
</style>
