<script lang="ts">
  import { getWaveform } from '../lib/ipc';
  import { getPosition, getDuration, seek, getCurrentTrack } from '../stores/player.svelte';
  import { formatDuration } from '../lib/format';

  let {
    overrideTrackId,
    overridePosition,
    overrideDuration,
    tint,
  }: {
    overrideTrackId?: string;
    overridePosition?: number;
    overrideDuration?: number;
    tint?: 'red' | 'green';
  } = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let containerEl: HTMLDivElement | undefined = $state();
  let waveformData = $state<number[] | null>(null);
  let isDragging = $state(false);

  const BAR_WIDTH = 2.5;
  const BAR_GAP = 1;
  const CANVAS_HEIGHT = 36;

  let lastTrackId = $state<string | null>(null);
  let canvasWidth = $state(360);

  // Determine which track/position/duration to use
  let effectiveTrackId = $derived(overrideTrackId ?? getCurrentTrack()?.id ?? null);
  let pos = $derived(overridePosition ?? getPosition());
  let dur = $derived(overrideDuration ?? getDuration());
  let remaining = $derived(dur > 0 ? dur - pos : 0);
  let isOverride = $derived(!!overrideTrackId);

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
    canvasWidth = containerEl.clientWidth;
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        canvasWidth = entry.contentRect.width;
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

  $effect(() => {
    if (!canvas) return;
    const _pos = pos;
    const _dur = dur;
    drawWaveform(_pos, _dur);
  });

  function drawWaveform(currentPos: number, duration: number) {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const w = canvasWidth;
    const h = CANVAS_HEIGHT;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, h);

    const progress = duration > 0 ? currentPos / duration : 0;

    // Colors based on tint
    let playedColor: string;
    let unplayedColor: string;
    if (tint === 'red') {
      playedColor = '#FF4444';
      unplayedColor = 'rgba(255, 68, 68, 0.25)';
    } else if (tint === 'green') {
      playedColor = '#44DD66';
      unplayedColor = 'rgba(68, 221, 102, 0.2)';
    } else {
      const styles = getComputedStyle(canvas);
      playedColor = styles.getPropertyValue('--text-primary').trim() || '#ffffff';
      unplayedColor = styles.getPropertyValue('--progress-bg').trim() || 'rgba(255,255,255,0.2)';
    }

    if (waveformData && waveformData.length > 0) {
      const barCount = Math.floor(w / (BAR_WIDTH + BAR_GAP));
      const progressX = progress * w;
      const maxBarHeight = h - 4;

      for (let i = 0; i < barCount; i++) {
        const dataIndex = Math.floor((i / barCount) * waveformData.length);
        const raw = waveformData[dataIndex] ?? 0;
        const jitter = 0.95 + Math.sin(i * 1.7) * 0.05;
        const normalized = (raw / 255) * jitter;
        const barHeight = Math.max(3, normalized * maxBarHeight);
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
          ctx.roundRect(x, y, playedWidth, barHeight, [1.5, 0, 0, 1.5]);
          ctx.fill();
          ctx.fillStyle = unplayedColor;
          ctx.beginPath();
          ctx.roundRect(x + playedWidth, y, BAR_WIDTH - playedWidth, barHeight, [0, 1.5, 1.5, 0]);
          ctx.fill();
          continue;
        }

        ctx.beginPath();
        ctx.roundRect(x, y, BAR_WIDTH, barHeight, [1.5, 1.5, 0, 0]);
        ctx.fill();
      }
    } else {
      // Fallback: thin progress bar
      const trackY = h - 3;
      const trackH = 2;
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
    if (isOverride) return; // Don't seek on the fading-out track's waveform
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

<div class="waveform-row" bind:this={containerEl}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <canvas
    bind:this={canvas}
    class="waveform-canvas"
    width={canvasWidth}
    height={CANVAS_HEIGHT}
    onmousedown={onMouseDown}
  ></canvas>
  <div class="time-labels">
    <span class="time">{formatDuration(pos)}</span>
    <span class="time">-{formatDuration(remaining)}</span>
  </div>
</div>

<style>
  .waveform-row {
    height: 56px;
    padding: 4px 0 0;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .waveform-canvas {
    width: 100%;
    height: 36px;
    cursor: pointer;
    display: block;
  }

  .time-labels {
    display: flex;
    justify-content: space-between;
    padding: 0 16px;
  }

  .time {
    font-size: 10px;
    font-weight: 500;
    color: var(--text-tertiary);
    font-variant-numeric: tabular-nums;
  }
</style>
