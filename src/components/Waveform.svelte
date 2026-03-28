<script lang="ts">
  import { getWaveform } from '../lib/ipc';
  import { getPosition, getDuration, seek, getCurrentTrack } from '../stores/player.svelte';
  import { formatDuration } from '../lib/format';

  let {
    overrideTrackId,
    overridePosition,
    overrideDuration,
    tint,
    beatGrid,
    downbeats,
    title,
    artist,
    bpm,
    showOverlay = false,
  }: {
    overrideTrackId?: string;
    overridePosition?: number;
    overrideDuration?: number;
    tint?: 'red' | 'green';
    beatGrid?: number[] | null;
    downbeats?: number[] | null;
    title?: string;
    artist?: string;
    bpm?: number;
    showOverlay?: boolean;
  } = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let containerEl: HTMLDivElement | undefined = $state();
  let waveformData = $state<number[] | null>(null);
  let isDragging = $state(false);

  const BAR_WIDTH = 2.5;
  const BAR_GAP = 1;

  // Pre-computed jitter table — large enough for fullscreen widths (1920/3.5 ≈ 549 bars)
  const MAX_BARS = 1024;
  const JITTER_TABLE = Array.from({ length: MAX_BARS }, (_, i) => 0.95 + Math.sin(i * 1.7) * 0.05);

  let lastTrackId = $state<string | null>(null);
  let canvasWidth = $state(360);
  let canvasHeight = $state(36);

  // Determine which track/position/duration to use
  let effectiveTrackId = $derived(overrideTrackId ?? getCurrentTrack()?.id ?? null);
  let pos = $derived(overridePosition ?? getPosition());
  let dur = $derived(overrideDuration ?? getDuration());
  let remaining = $derived(dur > 0 ? dur - pos : 0);
  let isOverride = $derived(!!overrideTrackId);
  let progressPx = $derived(dur > 0 ? (pos / dur) * canvasWidth : 0);

  $effect(() => {
    const tid = effectiveTrackId;
    if (tid && tid !== lastTrackId) {
      lastTrackId = tid;
      loadWaveform(tid);
    } else if (!tid) {
      lastTrackId = null;
      waveformData = null;
    }
  });

  $effect(() => {
    if (!containerEl) return;
    const rect = containerEl.getBoundingClientRect();
    canvasWidth = rect.width;
    canvasHeight = rect.height;
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        canvasWidth = entry.contentRect.width;
        canvasHeight = entry.contentRect.height;
      }
    });
    ro.observe(containerEl);
    return () => ro.disconnect();
  });

  async function loadWaveform(trackId: string) {
    try {
      waveformData = await getWaveform(trackId);
    } catch {
      waveformData = null;
    }
  }

  // Build a Set of downbeat times for O(1) lookup
  let prevDownbeats: number[] | null | undefined = undefined;
  let downbeatSet = $state(new Set<number>());
  $effect(() => {
    if (downbeats !== prevDownbeats) {
      prevDownbeats = downbeats;
      downbeatSet = new Set((downbeats ?? []).map(t => Math.round(t * 1000)));
    }
  });

  // Cache computed styles
  let cachedPlayedColor = '#ffffff';
  let cachedUnplayedColor = 'rgba(255,255,255,0.2)';

  $effect(() => {
    if (tint === 'red') {
      cachedPlayedColor = '#FF4444';
      cachedUnplayedColor = 'rgba(255, 68, 68, 0.25)';
    } else if (tint === 'green') {
      cachedPlayedColor = '#44DD66';
      cachedUnplayedColor = 'rgba(68, 221, 102, 0.2)';
    } else if (canvas) {
      const styles = getComputedStyle(canvas);
      cachedPlayedColor = styles.getPropertyValue('--text-primary').trim() || '#ffffff';
      cachedUnplayedColor = styles.getPropertyValue('--progress-bg').trim() || 'rgba(255,255,255,0.2)';
    }
  });

  // Throttle drawing
  let drawRafId: number | null = null;
  let lastDrawnPos = -1;
  let lastDrawTime = 0;

  $effect(() => {
    if (!canvas) return;
    const _pos = pos;
    const _dur = dur;

    const now = performance.now();
    if (now - lastDrawTime < 250 && lastDrawnPos >= 0 && !isDragging) return;

    const barCount = Math.floor(canvasWidth / (BAR_WIDTH + BAR_GAP));
    const minDelta = _dur > 0 ? _dur / barCount / 2 : 0;
    if (Math.abs(_pos - lastDrawnPos) < minDelta && lastDrawnPos >= 0 && !isDragging) return;

    if (drawRafId === null) {
      drawRafId = requestAnimationFrame(() => {
        drawRafId = null;
        lastDrawnPos = _pos;
        lastDrawTime = performance.now();
        drawWaveform(_pos, _dur);
      });
    }

    return () => {
      if (drawRafId !== null) { cancelAnimationFrame(drawRafId); drawRafId = null; }
    };
  });

  function drawWaveform(currentPos: number, duration: number) {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const w = canvasWidth;
    const h = canvasHeight;
    if (w <= 0 || h <= 0) return;

    const targetW = Math.round(w * dpr);
    const targetH = Math.round(h * dpr);
    if (canvas.width !== targetW || canvas.height !== targetH) {
      canvas.width = targetW;
      canvas.height = targetH;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const progress = duration > 0 ? currentPos / duration : 0;
    const playedColor = cachedPlayedColor;
    const unplayedColor = cachedUnplayedColor;

    // Top margin: reserve space for time labels when showOverlay is active
    const topMargin = showOverlay ? 14 : 2;

    if (waveformData && waveformData.length > 0) {
      const barCount = Math.floor(w / (BAR_WIDTH + BAR_GAP));
      const progressX = progress * w;
      const maxBarHeight = h - topMargin - 2;

      for (let i = 0; i < barCount; i++) {
        const dataIndex = Math.floor((i / barCount) * waveformData.length);
        const raw = waveformData[dataIndex] ?? 0;
        const jitter = JITTER_TABLE[i % MAX_BARS];
        const normalized = (raw / 255) * jitter;
        const barHeight = Math.max(2, normalized * maxBarHeight);
        const x = i * (BAR_WIDTH + BAR_GAP);
        const y = h - barHeight;

        const barRight = x + BAR_WIDTH;
        if (barRight <= progressX) {
          ctx.fillStyle = playedColor;
        } else if (x >= progressX) {
          ctx.fillStyle = unplayedColor;
        } else {
          const playedWidth = progressX - x;
          ctx.fillStyle = playedColor;
          ctx.beginPath();
          ctx.roundRect(x, y, playedWidth, barHeight, [1.5, 0, 0, 0]);
          ctx.fill();
          ctx.fillStyle = unplayedColor;
          ctx.beginPath();
          ctx.roundRect(x + playedWidth, y, BAR_WIDTH - playedWidth, barHeight, [0, 1.5, 0, 0]);
          ctx.fill();
          continue;
        }

        ctx.beginPath();
        ctx.roundRect(x, y, BAR_WIDTH, barHeight, [1.5, 1.5, 0, 0]);
        ctx.fill();
        // Dark outline for contrast on any background
        ctx.strokeStyle = 'rgba(0, 0, 0, 0.25)';
        ctx.lineWidth = 0.5;
        ctx.stroke();
      }

      // Beat grid overlay — only render when beats are spaced enough to be readable.
      // At full zoom-out with a 5min track at 128bpm, beats are ~0.5px apart (useless).
      // Show beats only when average spacing > 6px (roughly zoomed to <30s visible).
      if (beatGrid && beatGrid.length > 1 && duration > 0) {
        const avgBeatSpacing = (w / duration) * (duration / beatGrid.length);
        if (avgBeatSpacing > 6) {
          for (const beatTime of beatGrid) {
            const beatX = (beatTime / duration) * w;
            if (beatX < 0 || beatX > w) continue;
            const isDownbeat = downbeatSet.has(Math.round(beatTime * 1000));
            ctx.strokeStyle = isDownbeat ? 'rgba(255,255,255,0.18)' : 'rgba(255,255,255,0.07)';
            ctx.lineWidth = isDownbeat ? 1.2 : 0.7;
            ctx.beginPath();
            ctx.moveTo(beatX, 2);
            ctx.lineTo(beatX, h);
            ctx.stroke();
          }
        }
      }
      // Playhead line is rendered via CSS .playhead-line::before (no canvas duplicate)
    } else {
      // Fallback: thin progress bar
      const trackH = 2;
      const trackY = h - trackH - 1;
      ctx.fillStyle = unplayedColor;
      ctx.beginPath();
      ctx.roundRect(0, trackY, w, trackH, 1);
      ctx.fill();
      if (progress > 0) {
        ctx.fillStyle = playedColor;
        ctx.beginPath();
        ctx.roundRect(0, trackY, w * progress, trackH, 1);
        ctx.fill();
      }
    }
  }

  function getSeekPosition(e: MouseEvent) {
    if (!canvas) return 0;
    const rect = canvas.getBoundingClientRect();
    const x = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    return x * (overrideDuration ?? getDuration());
  }

  function onMouseDown(e: MouseEvent) {
    if (isOverride) return;
    isDragging = true;
    seek(getSeekPosition(e));
  }

  function onMouseMove(e: MouseEvent) {
    if (!isDragging || isOverride) return;
    seek(getSeekPosition(e));
  }

  function onMouseUp() {
    isDragging = false;
  }
</script>

<svelte:window onmouseup={onMouseUp} onmousemove={onMouseMove} />

<div class="waveform-wrap" bind:this={containerEl}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <canvas
    bind:this={canvas}
    class="waveform-canvas"
    onmousedown={onMouseDown}
  ></canvas>

  {#if showOverlay}
    <!-- Playhead line + time counters in the top margin area -->
    <div class="playhead-line" style="left: {progressPx}px">
      <span class="ph-time ph-elapsed">{formatDuration(pos)}</span>
      <span class="ph-time ph-remaining">-{formatDuration(remaining)}</span>
    </div>

    <!-- Info pill: BPM / title / artist — floats at bottom, overlapping into transport area -->
    <div class="info-pill">
      <span class="pill-title">{title || 'No Track'}</span>
      {#if artist}
        <span class="pill-artist">{artist}</span>
      {/if}
    </div>

  {:else}
    <div class="time-labels">
      <span class="time">{formatDuration(pos)}</span>
      <span class="time">-{formatDuration(remaining)}</span>
    </div>
  {/if}
</div>

<style>
  .waveform-wrap {
    width: 100%;
    min-width: 0;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    position: relative;
    /* Ensure the wrap fills the parent fully including in fullscreen */
    box-sizing: border-box;
  }

  .waveform-canvas {
    width: 100%;
    flex: 1;
    min-height: 0;
    cursor: pointer;
    display: block;
    /* Fade waveform bars to transparent at the bottom edge */
    mask-image: linear-gradient(to bottom, black 60%, transparent 100%);
    -webkit-mask-image: linear-gradient(to bottom, black 60%, transparent 100%);
  }

  /* Info pill: positioned ABOVE the waveform, not overlapping */
  .info-pill {
    position: absolute;
    top: -4px;
    left: 50%;
    transform: translateX(-50%) translateY(-100%);
    z-index: 6;
    pointer-events: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0;
    padding: 4px 18px 5px;
    background: rgba(0, 0, 0, 0.4);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    border-radius: 14px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    max-width: 80%;
  }
  .pill-title {
    font-size: clamp(16px, 3.5vw, 28px);
    font-weight: 600;
    color: #fff;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
    line-height: 1.2;
  }
  .pill-artist {
    font-size: clamp(12px, 2.2vw, 18px);
    font-weight: 500;
    color: rgba(255, 255, 255, 0.55);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
    line-height: 1.3;
  }

  /* Transparency fade at bottom of waveform bars */
  .waveform-fade {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: 24px;
    background: linear-gradient(to bottom, transparent 0%, rgba(0, 0, 0, 0) 100%);
    mask-image: linear-gradient(to bottom, transparent, black);
    -webkit-mask-image: linear-gradient(to bottom, transparent, black);
    pointer-events: none;
    z-index: 1;
  }

  /* Playhead line: thin 1px vertical line spanning full height with time labels at top */
  .playhead-line {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 0;
    z-index: 5;
    pointer-events: none;
  }
  .playhead-line::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: -0.5px;
    width: 1px;
    background: rgba(255, 255, 255, 0.8);
  }

  /* Time counters: inside the top margin of the waveform, flanking the playhead */
  .ph-time {
    position: absolute;
    top: 1px;
    font-size: clamp(9px, 1.2vw, 13px);
    font-weight: 500;
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .ph-elapsed {
    right: 5px;
  }
  .ph-remaining {
    left: 5px;
  }

  /* Static time labels (mini/override mode) */
  .time-labels {
    display: flex;
    justify-content: space-between;
    padding: 2px 16px 0;
    flex-shrink: 0;
  }

  .time {
    font-size: 10px;
    font-weight: 500;
    color: var(--text-tertiary);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
