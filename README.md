# Screenshot Tool

Advanced screenshot tool for Windows with annotation capabilities, optimized for AI communication.

## Features

- **Instant Capture**: Hotkey `Ctrl+Shift+S` captures screen instantly (preserves tooltips, dropdowns)
- **Region Selection**: Click and drag to select capture area
- **Annotation Tools**:
  - Pen & Highlighter
  - Arrows, Rectangles, Circles
  - Text & Numbered markers (perfect for "look at point 1, 2, 3")
  - Blur/Pixelate for privacy
- **History**: Keeps last 150 screenshots with thumbnails
- **Auto-copy**: Automatically copies to clipboard after annotation
- **System Tray**: Runs in background, minimal footprint

## Prerequisites

1. **Rust** - Install from https://rustup.rs/
   ```powershell
   winget install Rustlang.Rustup
   ```

2. **Node.js** (v18+) - https://nodejs.org/

3. **Visual Studio Build Tools** - Required for Rust on Windows
   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools
   ```

## Installation

```powershell
# Clone and enter directory
cd screenshot-tool

# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Keyboard Shortcuts

### Global
| Shortcut | Action |
|----------|--------|
| `Ctrl+Shift+S` | Capture screenshot |

### In Editor
| Shortcut | Action |
|----------|--------|
| `V` | Select tool |
| `P` | Pen |
| `H` | Highlighter |
| `A` | Arrow |
| `R` | Rectangle |
| `C` | Circle |
| `T` | Text |
| `N` | Numbered marker |
| `B` | Blur |
| `1-9` | Quick colors |
| `Ctrl+Z` | Undo |
| `Ctrl+Y` | Redo |
| `Ctrl+C` | Copy & close |
| `Ctrl+S` | Save to file |
| `Escape` | Cancel |

## Architecture

```
screenshot-tool/
├── src/                    # React frontend
│   ├── components/
│   │   ├── SelectionOverlay.tsx   # Region selection
│   │   ├── Editor.tsx             # Annotation canvas
│   │   └── History.tsx            # Screenshot history
│   └── stores/
│       └── appStore.ts            # Zustand state
├── src-tauri/              # Rust backend
│   └── src/
│       ├── lib.rs          # Main app + commands
│       ├── capture.rs      # Screen capture
│       ├── clipboard.rs    # Clipboard operations
│       └── history.rs      # SQLite history
```

## Data Storage

Screenshots and history are stored in:
```
%LOCALAPPDATA%\screenshot-tool\
├── screenshots/    # Full images (PNG)
├── thumbnails/     # Preview images (JPG)
└── history.db      # SQLite metadata
```
