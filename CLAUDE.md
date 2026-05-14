# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> **Environment note:** This is a Windows desktop app (Tauri). Build and run happen on Windows (PowerShell / `npm run tauri ...`). WSL is used only to edit source files via Claude Code -- do not attempt `cargo` / `npm install` from WSL.

## Build & Run

```bash
npm run tauri dev          # Dev mode (Vite HMR on :1420, Tauri hot-reload)
npm run release:check      # Verifies release metadata before publishing
npm run tauri build        # Production build (OS-specific Tauri bundles)

# Install after build:
sudo cp src-tauri/target/release/madera-ss /usr/bin/madera-ss

# IMPORTANT: Clear WebKit cache after frontend changes or theme won't update:
rm -rf ~/.cache/madera-ss/ ~/.local/share/madera-ss/storage
```

No automated test suite exists yet. Verify release changes with `npm run release:check`, `npm run build`, and a platform build.

## Architecture

**Tauri 2 app** with Rust backend (`src-tauri/src/`) and React/TypeScript frontend (`src/`).

### Frontend

- **Routing**: Hash-based in `App.tsx` (no React Router). Views: `dashboard`, `editor`, `history`, `colorpicker`, `settings`, `quickpaste`, `selection`, `pin`
- **Canvas editor**: Fabric.js v6 in `Editor.tsx` -- annotation tools (pen, highlighter, arrows, shapes, text, blur, numbered markers)
- **Theme system**: Single source of truth in `src/utils/theme.ts` (11 themes). All components import from there. Theme selected via Settings only, stored in `settings.json` via `tauri-plugin-store`.
- **State**: Component-local `useState` + Tauri store for persistence. No global state manager.

### Backend (Rust)

- **`lib.rs`**: Core app orchestration -- Tauri commands, window management, tray menu, global shortcuts, single-instance CLI dispatch
- **`platform/`**: OS boundary. `windows.rs` owns Win32 focus/paste/monitor behavior; `linux.rs` owns X11/Wayland/layer-shell/paste tooling; `other.rs` provides no-op fallback behavior.
- **`snippet_manager.rs`**: JSON-based snippet CRUD (`{APP_DATA_DIR}/snippets.json`)
- **`history.rs`**: SQLite for unified history (screenshots, clipboard, color picks)
- **`native_selection.rs`**: Platform-specific region selection (Win32 / slurp / slop / xcap)
- **`clipboard_monitor.rs`**: Background thread watching clipboard changes
- **`wayland_focus.rs`**: Linux focus tracking for Wayland/X11 paste workflows

### Window Management

Each feature opens its own Tauri WebView window. Editor windows are capped at 5 (oldest auto-closes). Windows are created in `lib.rs::open_editor_window()`, `open_main_window()`, etc. On Wayland, `inner_size()` may be ignored by the compositor -- use `set_size()` with Physical pixels post-build as workaround.

### CLI Flags & Global Shortcuts

Single-instance app. CLI flags (`--capture`, `--history`, `--colorpicker`, `--quickpaste`, `--snippets`) dispatch to the running instance. COSMIC desktop custom shortcuts call the binary with these flags since `tauri-plugin-global-shortcut` doesn't work on Wayland.

## Platform Notes

- **Official targets**: Windows 10/11 and Linux (X11 + Wayland)
- **Primary Linux target**: Pop!_OS 24.04 / COSMIC desktop / Wayland (scale factor 1)
- **macOS**: Not officially packaged yet
- **Platform boundary**: New OS-specific desktop behavior should go under `src-tauri/src/platform/`; `lib.rs` should call `platform::...` wrappers rather than importing Win32/X11/Wayland APIs directly.
- **Wayland overlays**: Use `gtk-layer-shell` for always-on-top panels. Standard `always_on_top` is ignored by Wayland compositors.
- **Text paste on Wayland**: `wtype` (not `xdotool`)
- **WebKit cache location**: `~/.cache/madera-ss/` and `~/.local/share/madera-ss/storage` -- must clear after rebuilds if frontend changes don't appear

## Key Patterns

- Editor canvas sizing: `initCanvas()` measures the container via `containerRef.current.clientWidth/Height`, scales the image to fit. A `ResizeObserver` re-fits the canvas on window resize.
- Theme loading is async (Tauri store). Editor waits for `themeLoaded` before initializing canvas to avoid layout race conditions.
- App settings are saved inside the current Tauri app config `settings.json` under `app_settings`; startup also migrates legacy settings from previous identifiers before falling back to defaults.
- Drawing colors palette is fixed (theme-independent) in `Editor.tsx::DRAWING_COLORS`.
