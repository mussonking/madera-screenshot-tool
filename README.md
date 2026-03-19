<p align="center">
  <img src="https://img.shields.io/badge/Built_with-Tauri_2-FFC131?style=for-the-badge&logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/Frontend-React_18-61DAFB?style=for-the-badge&logo=react&logoColor=white" alt="React" />
  <img src="https://img.shields.io/badge/Backend-Rust-DEA584?style=for-the-badge&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/Canvas-Fabric.js_6-FF6600?style=for-the-badge" alt="Fabric.js" />
  <img src="https://img.shields.io/badge/License-MIT-green?style=for-the-badge" alt="MIT License" />
</p>

<h1 align="center">Madera Screenshot Tool</h1>

<p align="center">
  <strong>The ultimate screenshot & annotation tool built for the AI era.</strong>
  <br />
  Capture. Annotate. Paste. Share with AI. All in one keystroke.
  <br />
  <br />
  <a href="#installation">Install</a> &middot; <a href="#features">Features</a> &middot; <a href="#themes">Themes</a> &middot; <a href="#quickpaste">QuickPaste</a> &middot; <a href="#architecture">Architecture</a>
</p>

---

## Why Madera?

Every AI conversation starts the same way: *"here's a screenshot of..."*

Madera was built for developers who live in AI-assisted workflows. One hotkey captures your screen, opens a full annotation editor, and copies the result to your clipboard -- ready to paste into Claude, ChatGPT, or any AI tool. No save dialogs. No file management. Just capture and paste.

But Madera goes further. It's a **clipboard powerhouse** with unified history, QuickPaste snippets, a color picker, and a desktop window layout manager -- all wrapped in 11 gorgeous themes and running at native speed thanks to Tauri 2 + Rust.

**~6MB binary. Zero Electron. Pure performance.**

## Features

### Screenshot Capture & Editor

- **Region selection** -- click and drag to capture any area of any monitor
- **Full annotation suite** -- pen, highlighter, arrows, rectangles, circles, text, numbered markers, blur/pixelate
- **Smart canvas sizing** -- image automatically fills the editor, adapts on resize
- **One-click copy** -- `Ctrl+C` copies the annotated image to clipboard instantly
- **Multi-monitor support** -- works across all your displays
- **Auto-resize for AI** -- optionally downscales to optimal resolution for AI vision models

### Unified Clipboard History

- **Everything in one place** -- screenshots, text clips, and color picks in a single searchable timeline
- **Pin important items** -- keep frequently used clips at the top
- **Background monitoring** -- silently tracks clipboard changes
- **SQLite-backed** -- fast search across thousands of entries

### QuickPaste Snippets

- **Instant access** -- `Ctrl+Alt+V` opens the snippet board overlay
- **Categories** -- organize snippets by project, language, or workflow
- **Text & image snippets** -- paste code blocks, boilerplate, signatures, or image assets
- **Auto-paste** -- selects a snippet and immediately types it into the active window
- **Wayland-native paste** -- uses `wtype` for reliable input on modern Linux

### Color Picker

- **Pixel-perfect sampling** -- magnified view for precise color selection
- **All formats** -- HEX (upper/lower), RGB, HSL
- **Color history** -- every pick is saved and searchable

### Desktop Guardian

- **Save window layouts** -- snapshot the position and size of every window
- **One-click restore** -- get your workspace back after a reboot or monitor change
- **Auto-save** -- configurable interval keeps your layout safe automatically
- **Multi-monitor aware** -- tracks which window belongs to which display

### 11 Themes

Pick your vibe. Every theme applies across the entire app.

| Theme | Style |
|-------|-------|
| **Default** | Clean dark blue |
| **Cyberpunk 2077** | Yellow-on-black, sharp edges |
| **Terminal** | Green phosphor CRT |
| **Candy Pop** | Pink pastel, rounded corners |
| **Sketch** | Light pencil-on-paper, dashed borders |
| **Neon Glow** | Purple/cyan gradients |
| **Midnight** | Deep navy blue |
| **Obsidian** | Warm dark with gold accents |
| **Nord** | Arctic blue palette |
| **Dracula** | Purple-accented dark |
| **Monokai** | Classic editor green |

## Installation

### From Source

**Prerequisites:** Node.js 18+, Rust toolchain, system dependencies for Tauri 2

```bash
# Clone
git clone https://github.com/mussonking/madera-screenshot-tool.git
cd madera-screenshot-tool

# Install dependencies
npm install

# Development mode (hot-reload)
npm run tauri dev

# Production build
npm run tauri build

# Install the binary (Linux)
sudo cp src-tauri/target/release/madera-tools /usr/bin/madera-tools
```

### Linux System Dependencies

```bash
# Ubuntu/Pop!_OS/Debian
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  libxdo-dev libx11-dev libxrandr-dev libxcomposite-dev libxdamage-dev \
  wtype  # for Wayland paste support
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Shift+S` | Capture screenshot |
| `Ctrl+Shift+H` | Open history |
| `Ctrl+Shift+X` | Color picker |
| `Ctrl+Alt+V` | QuickPaste snippets |
| `Ctrl+C` | Copy annotated image (in editor) |
| `Ctrl+S` | Save to file (in editor) |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `V` `P` `H` `A` `R` `C` `T` `N` `B` | Tool shortcuts (in editor) |
| `Escape` | Close current window |

### COSMIC / Wayland Setup

Global shortcuts don't work natively on Wayland. Set up COSMIC custom shortcuts to call the binary with flags:

```
madera-tools --capture      # Screenshot
madera-tools --history      # History panel
madera-tools --colorpicker  # Color picker
madera-tools --quickpaste   # QuickPaste overlay
```

## Architecture

```
madera-screenshot-tool/
  src/                          # React frontend
    components/
      Editor.tsx                # Fabric.js annotation canvas
      Dashboard.tsx             # Main app screen
      History.tsx               # Unified clipboard history
      QuickPasteModal.tsx       # Snippet selector overlay
      DesktopGuardian.tsx       # Window layout manager
      SettingsModal.tsx         # App preferences + theme picker
      ColorPicker.tsx           # Pixel color sampler
    utils/
      theme.ts                  # 11 theme definitions (single source of truth)
    App.tsx                     # Hash-based view router
  src-tauri/src/                # Rust backend
    lib.rs                      # Core: 50+ commands, windows, tray, shortcuts
    snippet_manager.rs          # Snippet CRUD (JSON storage)
    history.rs                  # SQLite history management
    clipboard_monitor.rs        # Background clipboard watcher
    native_selection.rs         # Platform-specific screen capture
    window_layout.rs            # Desktop window enumeration/restore
    color_picker.rs             # Pixel sampling + magnifier
```

### Tech Stack

| Layer | Technology |
|-------|-----------|
| Framework | Tauri 2 |
| Backend | Rust |
| Frontend | React 18 + TypeScript |
| Canvas | Fabric.js 6 |
| Styling | Tailwind CSS |
| Build | Vite 6 |
| Database | SQLite (rusqlite) |
| Icons | Lucide React |

## CLI Usage

Madera runs as a single-instance app. Subsequent calls dispatch to the running instance:

```bash
madera-tools                  # Open dashboard
madera-tools --capture        # Trigger screenshot capture
madera-tools --history        # Open history panel
madera-tools --colorpicker    # Open color picker
madera-tools --quickpaste     # Open QuickPaste overlay
madera-tools --snippets       # Open snippet manager
```

## Cross-Platform

| Platform | Status |
|----------|--------|
| Linux (Wayland/COSMIC) | Full support (primary target) |
| Linux (X11) | Full support |
| Windows | Full support |
| macOS | Partial (no layer-shell overlays) |

## Contributing

Contributions welcome! This project is built with Claude Code and designed to be AI-friendly:

- `CLAUDE.md` contains architecture docs for AI assistants
- Hash-based routing (no framework overhead)
- Clean Tauri command interface between frontend and backend
- Single theme system with clear separation

## License

MIT

---

<p align="center">
  Built with Rust, React, and too much coffee by <a href="https://github.com/mussonking">@mussonking</a>
  <br />
  <sub>Powered by <a href="https://tauri.app">Tauri 2</a> -- because Electron was never the answer.</sub>
</p>
