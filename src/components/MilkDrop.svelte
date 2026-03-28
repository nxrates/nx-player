<script lang="ts">
  import { audioPlayer } from '../lib/audio';
  import { invoke } from '@tauri-apps/api/core';
  import { randomIndex } from '../lib/format';
  import { getStatus } from '../stores/player.svelte';

  let { expanded = false }: { expanded?: boolean } = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let containerEl: HTMLDivElement | undefined = $state();
  let visualizer: any = null;
  let animFrameId: number | null = null;
  let cycleTimer: ReturnType<typeof setInterval> | null = null;
  let presetName = $state('');
  let showPresetName = $state(false);
  let hideTimeout: ReturnType<typeof setTimeout> | null = null;

  // --- Settings: reactive, re-read from localStorage on each render loop tick ---
  // Pixel scale: multiplier on CSS container size for the render resolution.
  // 1.0 = CSS pixels (blurry on retina but fast), 2.0 = retina-quality.
  // Max render size is capped so going above the display resolution does nothing.
  const MAX_RENDER_DIM = 1920; // absolute cap — never exceed 1080p equivalent

  function getPixelScale(): number {
    return parseFloat(localStorage.getItem('nx-viz-scale') || '1');
  }
  function getTargetFps(): number {
    return parseInt(localStorage.getItem('nx-viz-fps') || '60');
  }

  // Reactive FPS — re-read every time the render loop restarts
  let targetFps = getTargetFps();
  let frameInterval = 1000 / targetFps;
  let lastRenderTime = 0;

  let presetKeys: string[] = [];
  let presets: Record<string, any> = {};
  let currentIndex = 0;

  let noAudio = $state(true);

  // Compute render dimensions from CSS size and pixel scale, capped at MAX_RENDER_DIM
  function renderSize(cssW: number, cssH: number): [number, number] {
    const scale = getPixelScale();
    let w = Math.round(cssW * scale);
    let h = Math.round(cssH * scale);
    // Cap to MAX_RENDER_DIM (no point rendering higher than 1080p for a visualizer)
    if (w > MAX_RENDER_DIM) { h = Math.round(h * MAX_RENDER_DIM / w); w = MAX_RENDER_DIM; }
    if (h > MAX_RENDER_DIM) { w = Math.round(w * MAX_RENDER_DIM / h); h = MAX_RENDER_DIM; }
    return [Math.max(1, w), Math.max(1, h)];
  }

  function setVisualizationActive(active: boolean) {
    invoke('audio_set_visualization', { active }).catch(() => {});
  }

  $effect(() => {
    if (!canvas) return;

    const ctx = audioPlayer.getContext();
    const analyser = audioPlayer.getAnalyser();
    if (!ctx || !analyser) {
      noAudio = true;
      return;
    }

    noAudio = false;
    audioPlayer.connectAnalyser();

    Promise.all([
      import('butterchurn'),
      import('butterchurn-presets'),
    ]).then(([butterchurnMod, presetsMod]) => {
      const butterchurn = butterchurnMod.default;
      presets = presetsMod.default.getPresets();
      presetKeys = Object.keys(presets);

      const [iw, ih] = renderSize(canvas!.clientWidth, canvas!.clientHeight);
      canvas!.width = iw;
      canvas!.height = ih;

      visualizer = butterchurn.createVisualizer(ctx, canvas!, {
        width: iw,
        height: ih,
        pixelRatio: 1,
        textureRatio: 1,
      });

      visualizer.connectAudio(analyser);

      const savedPreset = localStorage.getItem('nx-viz-preset');
      if (savedPreset && presets[savedPreset]) {
        currentIndex = presetKeys.indexOf(savedPreset);
        visualizer.loadPreset(presets[savedPreset], 0);
        presetName = savedPreset;
      } else {
        currentIndex = randomIndex(presetKeys.length);
        visualizer.loadPreset(presets[presetKeys[currentIndex]], 0);
        presetName = presetKeys[currentIndex];
      }

      visualizer.setRendererSize(iw, ih);
      startRender();
      startAutoCycle();
      setVisualizationActive(true);
    });

    return () => {
      stopRender();
      stopAutoCycle();
      setVisualizationActive(false);
      audioPlayer.disconnectAnalyser();
      if (visualizer) {
        try { visualizer.disconnectAudio?.(); } catch {}
        visualizer = null;
      }
      if (canvas) {
        const gl = canvas.getContext('webgl') || canvas.getContext('webgl2');
        if (gl) {
          const ext = gl.getExtension('WEBGL_lose_context');
          if (ext) ext.loseContext();
        }
        // Force canvas buffer deallocation
        canvas.width = 0;
        canvas.height = 0;
      }
      presets = {};
      presetKeys = [];
    };
  });

  // ResizeObserver — updates canvas + butterchurn render size
  $effect(() => {
    if (!containerEl || !canvas) return;
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const [pw, ph] = renderSize(entry.contentRect.width, entry.contentRect.height);
        if (pw > 0 && ph > 0) {
          canvas!.width = pw;
          canvas!.height = ph;
          if (visualizer) {
            visualizer.setRendererSize(pw, ph);
          }
        }
      }
    });
    ro.observe(containerEl);
    return () => ro.disconnect();
  });

  // Visibility change
  $effect(() => {
    if (typeof document === 'undefined') return;
    const handler = () => {
      if (document.hidden) {
        stopRender();
        setVisualizationActive(false);
      } else if (visualizer) {
        startRender();
        setVisualizationActive(true);
      }
    };
    document.addEventListener('visibilitychange', handler);
    return () => document.removeEventListener('visibilitychange', handler);
  });

  // Stop rendering when music stops
  $effect(() => {
    const playing = getStatus() === 'playing';
    if (!visualizer) return;
    if (playing) {
      startRender();
      setVisualizationActive(true);
    } else {
      stopRender();
      setVisualizationActive(false);
    }
  });

  function startRender() {
    if (animFrameId !== null) return;
    // Re-read FPS setting on each render restart (reactive to settings changes)
    targetFps = getTargetFps();
    frameInterval = 1000 / targetFps;
    lastRenderTime = 0;
    animFrameId = requestAnimationFrame(renderFrame);
  }

  function stopRender() {
    if (animFrameId !== null) {
      cancelAnimationFrame(animFrameId);
      animFrameId = null;
    }
  }

  function renderFrame(ts: number) {
    if (ts - lastRenderTime < frameInterval) {
      animFrameId = requestAnimationFrame(renderFrame);
      return;
    }
    // Compute elapsed time in seconds for butterchurn (frame-rate independent timing)
    const elapsed = lastRenderTime > 0 ? (ts - lastRenderTime) / 1000 : 1 / targetFps;
    lastRenderTime = ts - ((ts - lastRenderTime) % frameInterval);

    if (visualizer) {
      // Pass elapsedTime so butterchurn's internal time advances by real wall-clock delta,
      // not by 1/measured_fps. This ensures preset animation speed is identical
      // regardless of the FPS cap (15fps and 120fps look the same speed).
      visualizer.render({ elapsedTime: elapsed });
    }
    animFrameId = requestAnimationFrame(renderFrame);
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
