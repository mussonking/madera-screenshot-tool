# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
npm run tauri dev          # Dev mode (Vite HMR on :1420, Tauri hot-reload)
npm run tauri build        # Production build (bundles .deb, .rpm, .AppImage)

# Install after build:
sudo cp src-tauri/target/release/madera-tools /usr/bin/madera-tools

# IMPORTANT: Clear WebKit cache after frontend changes or theme won't update:
rm -rf ~/.cache/madera-tools/ ~/.local/share/madera-tools/storage
```

No test suite exists. Verify changes by building and manually testing.

## Architecture

**Tauri 2 app** with Rust backend (`src-tauri/src/`) and React/TypeScript frontend (`src/`).

### Frontend

- **Routing**: Hash-based in `App.tsx` (no React Router). Views: `dashboard`, `editor`, `history`, `colorpicker`, `desktop-guardian`, `settings`, `quickpaste`, `selection`
- **Canvas editor**: Fabric.js v6 in `Editor.tsx` -- annotation tools (pen, highlighter, arrows, shapes, text, blur, numbered markers)
- **Theme system**: Single source of truth in `src/utils/theme.ts` (11 themes). All components import from there. Theme selected via Settings only, stored in `settings.json` via `tauri-plugin-store`.
- **State**: Component-local `useState` + Tauri store for persistence. No global state manager.

### Backend (Rust)

- **`lib.rs`** (~2900 lines): Core app -- all 50+ Tauri commands, window management, tray menu, global shortcuts, single-instance CLI dispatch
- **`snippet_manager.rs`**: JSON-based snippet CRUD (`{APP_DATA_DIR}/snippets.json`)
- **`history.rs`**: SQLite for unified history (screenshots, clipboard, color picks)
- **`native_selection.rs`**: Platform-specific region selection (Win32 / xcap)
- **`clipboard_monitor.rs`**: Background thread watching clipboard changes
- **`window_layout.rs`**: Desktop Guardian -- enumerate/save/restore window positions

### Window Management

Each feature opens its own Tauri WebView window. Editor windows are capped at 5 (oldest auto-closes). Windows are created in `lib.rs::open_editor_window()`, `open_main_window()`, etc. On Wayland, `inner_size()` may be ignored by the compositor -- use `set_size()` with Physical pixels post-build as workaround.

### CLI Flags & Global Shortcuts

Single-instance app. CLI flags (`--capture`, `--history`, `--colorpicker`, `--quickpaste`, `--snippets`) dispatch to the running instance. COSMIC desktop custom shortcuts call the binary with these flags since `tauri-plugin-global-shortcut` doesn't work on Wayland.

## Platform Notes

- **Primary target**: Pop!_OS 24.04 / COSMIC desktop / Wayland (scale factor 1)
- **Wayland overlays**: Use `gtk-layer-shell` for always-on-top panels. Standard `always_on_top` is ignored by Wayland compositors.
- **Text paste on Wayland**: `wtype` (not `xdotool`)
- **WebKit cache location**: `~/.cache/madera-tools/` and `~/.local/share/madera-tools/storage` -- must clear after rebuilds if frontend changes don't appear

## Key Patterns

- Editor canvas sizing: `initCanvas()` measures the container via `containerRef.current.clientWidth/Height`, scales the image to fit. A `ResizeObserver` re-fits the canvas on window resize.
- Theme loading is async (Tauri store). Editor waits for `themeLoaded` before initializing canvas to avoid layout race conditions.
- Drawing colors palette is fixed (theme-independent) in `Editor.tsx::DRAWING_COLORS`.
