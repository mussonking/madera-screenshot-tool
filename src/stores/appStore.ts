import { create } from "zustand";

export interface PendingCapture {
  image_data: string;
  width: number;
  height: number;
  monitor_name: string;
}

export interface ScreenshotRecord {
  id: string;
  filename: string;
  thumbnail: string;
  created_at: string;
  width: number;
  height: number;
  saved_path: string | null;
}

// Unified history item types
export type HistoryItemType = "screenshot" | "clipboard_text" | "clipboard_image" | "color_pick";

export interface HistoryItem {
  id: string;
  item_type: HistoryItemType;
  created_at: string;
  // For screenshots and clipboard images
  filename: string | null;
  thumbnail: string | null;
  width: number | null;
  height: number | null;
  saved_path: string | null;
  // For clipboard text
  text_content: string | null;
  text_preview: string | null;
  // For color picks
  color_hex: string | null;
  color_rgb: string | null;
  color_hsl: string | null;
  // Metadata
  source_app: string | null;
  is_pinned: boolean;
}

export interface ColorInfo {
  hex: string;
  hex_lower: string;
  rgb: { r: number; g: number; b: number };
  hsl: { h: number; s: number; l: number };
}

export interface ColorPickSettings {
  format: "hex_upper" | "hex_lower" | "rgb" | "hsl";
  max_history: number;
  magnifier_size: number;
}

export interface AppSettings {
  hotkey: string;
  auto_copy: boolean;
  max_history: number;
  max_image_width: number | null;
}

export interface ClipboardSettings {
  enabled: boolean;
  max_items: number;
  excluded_apps: string[];
  auto_cleanup_days: number | null;
}

interface AppState {
  // Current capture state
  pendingCapture: PendingCapture | null;
  setPendingCapture: (capture: PendingCapture | null) => void;

  // Editor state
  currentImage: string | null;
  currentWidth: number;
  currentHeight: number;
  setCurrentImage: (image: string | null, width: number, height: number) => void;

  // Legacy history (screenshots only)
  history: ScreenshotRecord[];
  setHistory: (history: ScreenshotRecord[]) => void;

  // Unified history (screenshots + clipboard)
  unifiedHistory: HistoryItem[];
  setUnifiedHistory: (history: HistoryItem[]) => void;

  // Settings
  settings: AppSettings;
  setSettings: (settings: AppSettings) => void;

  // Clipboard settings
  clipboardSettings: ClipboardSettings;
  setClipboardSettings: (settings: ClipboardSettings) => void;

  // Filter state for history view
  historyFilter: HistoryItemType | null;
  setHistoryFilter: (filter: HistoryItemType | null) => void;

  // Search query
  searchQuery: string;
  setSearchQuery: (query: string) => void;
}

export const useAppStore = create<AppState>((set) => ({
  pendingCapture: null,
  setPendingCapture: (capture) => set({ pendingCapture: capture }),

  currentImage: null,
  currentWidth: 0,
  currentHeight: 0,
  setCurrentImage: (image, width, height) =>
    set({ currentImage: image, currentWidth: width, currentHeight: height }),

  history: [],
  setHistory: (history) => set({ history }),

  unifiedHistory: [],
  setUnifiedHistory: (unifiedHistory) => set({ unifiedHistory }),

  settings: {
    hotkey: "Ctrl+Shift+S",
    auto_copy: true,
    max_history: 150,
    max_image_width: 1568,
  },
  setSettings: (settings) => set({ settings }),

  clipboardSettings: {
    enabled: true,
    max_items: 200,
    excluded_apps: ["1Password", "LastPass", "Bitwarden", "KeePass", "Dashlane"],
    auto_cleanup_days: 30,
  },
  setClipboardSettings: (clipboardSettings) => set({ clipboardSettings }),

  historyFilter: null,
  setHistoryFilter: (historyFilter) => set({ historyFilter }),

  searchQuery: "",
  setSearchQuery: (searchQuery) => set({ searchQuery }),
}));
