<script lang="ts">
  import { audioPlayer } from '../lib/audio';

  let { expanded = false }: { expanded?: boolean } = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let containerEl: HTMLDivElement | undefined = $state();
  let visualizer: any = $state(null);
  let animFrameId: number | null = null;
  let cycleTimer: ReturnType<typeof setInterval> | null = null;
  let presetName = $state('');
  let showPresetName = $state(false);
  let hideTimeout: ReturnType<typeof setTimeout> | null = null;

  const TARGET_FPS = parseInt(localStorage.getItem('nx-viz-fps') || '60');
  const FRAME_INTERVAL = 1000 / TARGET_FPS;
  let lastRenderTime = 0;

  let presetKeys: string[] = [];
  let presets: Record<string, any> = {};
  let currentIndex = 0;

  let noAudio = $state(true);

  $effect(() => {
    if (!canvas) return;

    const ctx = audioPlayer.getContext();
    const analyser = audioPlayer.getAnalyser();
    if (!ctx || !analyser) {
      noAudio = true;
      return;
    }

    noAudio = false;

    Promise.all([
      import('butterchurn'),
      import('butterchurn-presets'),
    ]).then(([butterchurnMod, presetsMod]) => {
      const butterchurn = butterchurnMod.default;
      presets = presetsMod.default.getPresets();
      presetKeys = Object.keys(presets);

      visualizer = butterchurn.createVisualizer(ctx, canvas!, {
        width: canvas!.width,
        height: canvas!.height,
        pixelRatio: window.devicePixelRatio || 1,
        textureRatio: 1,
      });

      visualizer.connectAudio(analyser);

      // Load saved preset or random initial preset
      const savedPreset = localStorage.getItem('nx-viz-preset');
      if (savedPreset && presets[savedPreset]) {
        currentIndex = presetKeys.indexOf(savedPreset);
        visualizer.loadPreset(presets[savedPreset], 0);
        presetName = savedPreset;
      } else {
        currentIndex = Math.floor(Math.random() * presetKeys.length);
        visualizer.loadPreset(presets[presetKeys[currentIndex]], 0);
        presetName = presetKeys[currentIndex];
      }

      visualizer.setRendererSize(canvas!.width, canvas!.height);
      startRender();
      startAutoCycle();
    });

    return () => {
      stopRender();
      stopAutoCycle();
    };
  });

  // ResizeObserver for container size changes
  $effect(() => {
    if (!containerEl || !canvas) return;
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const w = Math.floor(entry.contentRect.width);
        const h = Math.floor(entry.contentRect.height);
        if (w > 0 && h > 0) {
          canvas!.width = w * (window.devicePixelRatio || 1);
          canvas!.height = h * (window.devicePixelRatio || 1);
          if (visualizer) {
            visualizer.setRendererSize(w, h);
          }
        }
      }
    });
    ro.observe(containerEl);
    return () => ro.disconnect();
  });

  function startRender() {
    if (animFrameId !== null) return;
    animFrameId = requestAnimationFrame(renderFrame);
  }

  function stopRender() {
    if (animFrameId !== null) {
      cancelAnimationFrame(animFrameId);
      animFrameId = null;
    }
  }

  function renderFrame(ts: number) {
    animFrameId = requestAnimationFrame(renderFrame);
    if (ts - lastRenderTime < FRAME_INTERVAL) return;
    lastRenderTime = ts - ((ts - lastRenderTime) % FRAME_INTERVAL);
    if (visualizer) visualizer.render();
  }

  function startAutoCycle() {
    stopAutoCycle();
    const interval = parseInt(localStorage.getItem('nx-viz-cycle') || '20') * 1000;
    cycleTimer = setInterval(() => nextPreset(), interval);
  }

  function stopAutoCycle() {
    if (cycleTimer) { clearInterval(cycleTimer); cycleTimer = null; }
  }

  function nextPreset() {
    if (!visualizer || presetKeys.length === 0) return;
    currentIndex = (currentIndex + 1) % presetKeys.length;
    presetName = presetKeys[currentIndex];
    visualizer.loadPreset(presets[presetName], 2.7);
    flashPresetName();
  }

  function flashPresetName() {
    showPresetName = true;
    if (hideTimeout) clearTimeout(hideTimeout);
    hideTimeout = setTimeout(() => { showPresetName = false; }, 3000);
  }

  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', () => {
      if (document.hidden) stopRender();
      else if (visualizer) startRender();
    });
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="milkdrop" bind:this={containerEl} onclick={nextPreset}>
  {#if noAudio}
    <div class="placeholder">
      <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round">
        <path d="M9 18V5l12-2v13"/>
        <circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>
      </svg>
      <span>Play a track to start visualizer</span>
    </div>
  {/if}
  <canvas bind:this={canvas}></canvas>
  {#if showPresetName}
    <div class="preset-label">{presetName}</div>
  {/if}
</div>

<style>
  .milkdrop {
    position: relative;
    width: 100%;
    height: 100%;
    cursor: pointer;
    overflow: hidden;
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
  .preset-label {
    position: absolute;
    top: 8px;
    left: 8px;
    font-size: 10px;
    color: var(--text-secondary);
    text-shadow: 0 1px 3px rgba(0,0,0,0.8);
    pointer-events: none;
    transition: opacity 300ms;
  }
  .placeholder {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--text-tertiary);
    z-index: 1;
  }
  .placeholder span {
    font-size: 12px;
    opacity: 0.6;
  }
</style>
