# NX Player -- Architecture

Lightweight, high-performance music player with auto-mixing, MilkDrop visualization, and mobile-first UI.

**Scope boundary:** NX Player is a standalone desktop application. It has no dependency on or access to NX backend services, financial data, or communications infrastructure.

---

## Stack

- **Backend:** Tauri v2 (Rust)
- **Frontend:** Svelte 5
- **Audio:** Web Audio API (dual-deck crossfade engine)
- **Visualization:** Butterchurn WebGL (MilkDrop)
- **Data:** SQLite metadata cache

---

## Features

### Playback
- Dual-deck crossfade engine with equal-power gain curves
- Auto-mix with configurable crossfade duration (0-30s)
- BPM detection (energy-based onset analysis with octave correction)
- BPM matching (adjusts playback rate to sync tempos during crossfade)
- Shuffle with deterministic back-navigation

### Visualization
- SoundCloud-style waveform (pre-computed RMS amplitude bars)
- MilkDrop visualizer (200+ presets, auto-cycling)
- Fullscreen mode with auto-hiding controls

### Library
- Auto-scan of OS-default music directories
- Album cover retrieval (embedded extraction + iTunes API fallback)
- Optimized cover cache (400x400 JPEG)
- SQLite metadata cache for instant loading

### UI
- 3 view modes: Normal (album art), Mini (compact), MilkDrop (visualizer)
- Glass controls with backdrop blur
- Soulseek P2P integration (see `docs/SOULSEEK_PROTOCOL_SPEC.md`)

---

## Source Code

```
nx-player/
├── src/              # Svelte frontend
├── src-tauri/        # Rust backend (Tauri)
├── static/           # Static assets
├── docs/             # This documentation
└── patches/          # Dependency patches
```
