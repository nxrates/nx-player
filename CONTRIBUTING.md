# Contributing to NX Player

Thank you for your interest in contributing to NX Player.

## Getting Started

### Prerequisites

- **Rust** (stable, via rustup)
- **Node.js** ≥ 20
- **pnpm** ≥ 10
- **Tauri CLI** v2 (`cargo install tauri-cli`)

### Setup

```bash
git clone <repo-url>
cd lightseek
pnpm install
pnpm tauri dev
```

### Build

```bash
pnpm tauri build
```

## Project Structure

```
src/                    # Svelte 5 frontend
  components/           # UI components
  stores/               # Reactive state (Svelte 5 runes)
  lib/                  # Utilities (audio engine, IPC, formatting)
  styles/               # Global CSS + theme tokens

src-tauri/              # Rust backend
  src/
    commands/           # Tauri IPC command handlers
    analyzer.rs         # BPM detection
    waveform.rs         # Waveform extraction
    covers.rs           # Embedded cover art extraction
    covers_fetch.rs     # iTunes API cover fetching
    scanner.rs          # Library scanner
    db.rs               # SQLite database
    models.rs           # Data structures
```

## Contribution Guidelines

- **Branch from `main`**, use descriptive branch names: `feat/milkdrop-presets`, `fix/crossfade-timing`
- **One concern per commit** — logical units, clear imperative messages
- **Conventional prefixes**: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`
- **Never mention AI tools** in commits or PRs
- Run `pnpm build` and `cargo check` before submitting
- Keep the codebase lean — avoid adding dependencies unless strictly necessary

## Code Style

- **Frontend**: Svelte 5 with runes (`$state`, `$derived`, `$effect`, `$props`)
- **Styles**: Scoped `<style>` blocks in components, CSS custom properties for theming
- **Rust**: Standard Rust formatting (`cargo fmt`), minimal dependencies
- **Icons**: Inline SVG using Lucide icon paths — no icon library dependency
- **DRY**: Reuse components aggressively (e.g., `TrackRow` for library and queue, `Waveform` for current and crossfade tracks)

## Architecture Decisions

- **No virtual DOM** — Svelte compiles to direct DOM updates
- **No CSS-in-JS** — scoped CSS with zero runtime cost
- **No router** — view switching via `$state` and `{#if}` blocks
- **Dual-deck audio** — Web Audio API with two HTMLAudioElements for crossfading
- **SQLite** — all metadata cached locally via rusqlite
- **Lazy computation** — waveforms and BPM computed on-demand, cached in DB

## License

MIT — see [LICENSE](LICENSE)
