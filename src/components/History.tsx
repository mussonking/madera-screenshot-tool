import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { load } from "@tauri-apps/plugin-store";
import { format, parseISO } from "date-fns";
import {
  Copy,
  Save,
  Trash2,
  Edit3,
  Calendar,
  Image,
  RefreshCw,
  Trash,
  FileText,
  Pin,
  PinOff,
  Search,
  X,
  Camera,
  Clipboard,
  Filter,
  Settings,
  Palette,
  Keyboard,
  Power,
  ClipboardCheck,
  Check,
  Pipette,
  Shield,
  LayoutDashboard,
} from "lucide-react";
import type { HistoryItem, HistoryItemType } from "../stores/appStore";

// Theme definitions - same as Editor
type ThemeName = "default" | "cyberpunk" | "retro" | "candy" | "sketch" | "neon";

interface Theme {
  name: string;
  toolbar: string;
  toolbarBorder: string;
  buttonBg: string;
  buttonActive: string;
  canvasBg: string;
  textColor: string;
  accentColor: string;
  fontFamily: string;
  borderRadius: string;
  borderStyle: string;
}

const THEMES: Record<ThemeName, Theme> = {
  default: {
    name: "Default",
    toolbar: "#16213e",
    toolbarBorder: "#0f3460",
    buttonBg: "#0f3460",
    buttonActive: "#e94560",
    canvasBg: "#1a1a2e",
    textColor: "#e5e5e5",
    accentColor: "#e94560",
    fontFamily: "'Segoe UI', system-ui, sans-serif",
    borderRadius: "8px",
    borderStyle: "solid",
  },
  cyberpunk: {
    name: "Cyberpunk 2077",
    toolbar: "#0c0c0c",
    toolbarBorder: "#fcee0a",
    buttonBg: "#1a1a1a",
    buttonActive: "#fcee0a",
    canvasBg: "#0a0a0a",
    textColor: "#fcee0a",
    accentColor: "#00f0ff",
    fontFamily: "'Orbitron', 'Share Tech Mono', monospace",
    borderRadius: "0px",
    borderStyle: "solid",
  },
  retro: {
    name: "Terminal",
    toolbar: "#0a140a",
    toolbarBorder: "#1a3a1a",
    buttonBg: "#0d1f0d",
    buttonActive: "#33bb33",
    canvasBg: "#050d05",
    textColor: "#33bb33",
    accentColor: "#22aa22",
    fontFamily: "'VT323', 'Courier New', monospace",
    borderRadius: "0px",
    borderStyle: "solid",
  },
  candy: {
    name: "Candy Pop",
    toolbar: "#fff0f5",
    toolbarBorder: "#ffb6c1",
    buttonBg: "#ffe4ec",
    buttonActive: "#ff6b9d",
    canvasBg: "#fff5f8",
    textColor: "#c44569",
    accentColor: "#ff6b9d",
    fontFamily: "'Comic Sans MS', cursive",
    borderRadius: "20px",
    borderStyle: "solid",
  },
  sketch: {
    name: "Sketch",
    toolbar: "#fefefe",
    toolbarBorder: "#ccc",
    buttonBg: "#f0f0f0",
    buttonActive: "#2d3436",
    canvasBg: "#f8f8f8",
    textColor: "#2d3436",
    accentColor: "#0984e3",
    fontFamily: "'Segoe Print', cursive",
    borderRadius: "4px",
    borderStyle: "dashed",
  },
  neon: {
    name: "Neon Glow",
    toolbar: "#0a0015",
    toolbarBorder: "#ff00ff",
    buttonBg: "#150025",
    buttonActive: "#ff00ff",
    canvasBg: "#050010",
    textColor: "#ff88ff",
    accentColor: "#00ffff",
    fontFamily: "'Audiowide', sans-serif",
    borderRadius: "12px",
    borderStyle: "solid",
  },
};

const STORE_PATH = "settings.json";

interface ClipboardSettings {
  enabled: boolean;
  max_items: number;
  excluded_apps: string[];
  auto_cleanup_days: number | null;
}

const loadThemeFromStore = async (): Promise<ThemeName> => {
  try {
    const store = await load(STORE_PATH);
    const saved = await store.get<string>("theme");
    if (saved && saved in THEMES) {
      return saved as ThemeName;
    }
  } catch {
    // Store not available
  }
  return "default";
};

const saveThemeToStore = async (theme: ThemeName): Promise<void> => {
  try {
    const store = await load(STORE_PATH);
    await store.set("theme", theme);
    await store.save();
  } catch {
    // Store not available
  }
};

const getItemTypeIcon = (type: HistoryItemType) => {
  switch (type) {
    case "screenshot":
      return <Camera size={14} className="text-blue-400" />;
    case "clipboard_text":
      return <FileText size={14} className="text-green-400" />;
    case "clipboard_image":
      return <Clipboard size={14} className="text-purple-400" />;
    case "color_pick":
      return <Pipette size={14} className="text-yellow-400" />;
  }
};

const getItemTypeLabel = (type: HistoryItemType) => {
  switch (type) {
    case "screenshot":
      return "Screenshot";
    case "clipboard_text":
      return "Text";
    case "clipboard_image":
      return "Image";
    case "color_pick":
      return "Color";
  }
};

export default function History() {
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [filteredItems, setFilteredItems] = useState<HistoryItem[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchDate, setSearchDate] = useState("");
  const [filterType, setFilterType] = useState<HistoryItemType | null>(null);
  const [loading, setLoading] = useState(true);
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");
  const [expandedTextId, setExpandedTextId] = useState<string | null>(null);

  // Settings panel state
  const [showSettings, setShowSettings] = useState(false);
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const [clipboardSettings, setClipboardSettings] = useState<ClipboardSettings>({
    enabled: true,
    max_items: 200,
    excluded_apps: [],
    auto_cleanup_days: 30,
  });
  const [isAutostart, setIsAutostart] = useState(false);
  const [isClipboardMonitoring, setIsClipboardMonitoring] = useState(true);

  const theme = THEMES[currentTheme];

  const loadHistory = useCallback(async () => {
    setLoading(true);
    try {
      const history = await invoke<HistoryItem[]>("get_unified_history", {
        filterType: filterType,
      });
      setItems(history);
    } catch (err) {
      console.error("Failed to load history:", err);
    }
    setLoading(false);
  }, [filterType]);

  // Load settings on mount
  const loadSettings = useCallback(async () => {
    try {
      // Load clipboard settings
      const settings = await invoke<ClipboardSettings>("get_clipboard_settings");
      setClipboardSettings(settings);
      setIsClipboardMonitoring(settings.enabled);

      // Load autostart status
      const autostart = await invoke<boolean>("is_autostart_enabled");
      setIsAutostart(autostart);
    } catch (err) {
      console.error("Failed to load settings:", err);
    }
  }, []);

  // Initial load and clipboard listener - only run once on mount
  useEffect(() => {
    loadThemeFromStore().then(setCurrentTheme);
    loadHistory();
    loadSettings();

    // Listen for clipboard changes
    const unlisten = listen("clipboard-changed", () => {
      loadHistory();
    });

    return () => {
      unlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Reload when filter type changes
  useEffect(() => {
    loadHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filterType]);

  // Keyboard shortcut: Ctrl+V to copy selected item
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === "v" && selectedId) {
        e.preventDefault();
        const selectedItem = items.find((item) => item.id === selectedId);
        if (selectedItem) {
          copyToClipboard(selectedItem);
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [selectedId, items]);

  useEffect(() => {
    let filtered = items;

    // Filter by search query (text content)
    if (searchQuery.trim()) {
      filtered = filtered.filter(
        (item) =>
          item.text_content?.toLowerCase().includes(searchQuery.toLowerCase()) ||
          item.text_preview?.toLowerCase().includes(searchQuery.toLowerCase())
      );
    }

    // Filter by date
    if (searchDate) {
      filtered = filtered.filter((item) => item.created_at.startsWith(searchDate));
    }

    setFilteredItems(filtered);
  }, [searchQuery, searchDate, items]);

  const openInEditor = async (id: string) => {
    try {
      const imageData = await invoke<string | null>("get_history_item_image", { id });
      if (imageData) {
        const record = items.find((s) => s.id === id);
        if (record && record.width && record.height) {
          await invoke("open_editor_with_image", {
            imageData,
            width: record.width,
            height: record.height,
          });
        }
      }
    } catch (err) {
      console.error("Failed to open in editor:", err);
    }
  };

  const copyToClipboard = async (item: HistoryItem) => {
    try {
      if (item.item_type === "clipboard_text" && item.text_content) {
        await invoke("copy_text_to_clipboard", { text: item.text_content });
      } else if (item.item_type === "color_pick" && item.color_hex) {
        await invoke("copy_text_to_clipboard", { text: item.color_hex });
      } else {
        const imageData = await invoke<string | null>("get_history_item_image", {
          id: item.id,
        });
        if (imageData) {
          await invoke("copy_to_clipboard", { imageData });
        }
      }
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  };

  const saveToFile = async (id: string) => {
    try {
      const record = items.find((s) => s.id === id);
      if (!record || !record.filename) return;

      const path = await save({
        filters: [{ name: "PNG Image", extensions: ["png"] }],
        defaultPath: record.filename,
      });

      if (path) {
        const imageData = await invoke<string | null>("get_history_item_image", { id });
        if (imageData) {
          await invoke("save_image_to_file", { imageData, path });
        }
      }
    } catch (err) {
      console.error("Failed to save:", err);
    }
  };

  const deleteItem = async (id: string) => {
    try {
      await invoke("delete_history_item", { id });
      setItems((prev) => prev.filter((s) => s.id !== id));
      if (selectedId === id) {
        setSelectedId(null);
      }
    } catch (err) {
      console.error("Failed to delete:", err);
    }
  };

  const togglePin = async (id: string) => {
    try {
      const newPinned = await invoke<boolean>("toggle_pin_item", { id });
      setItems((prev) =>
        prev.map((item) => (item.id === id ? { ...item, is_pinned: newPinned } : item))
      );
    } catch (err) {
      console.error("Failed to toggle pin:", err);
    }
  };

  const clearAllHistory = async () => {
    setShowClearConfirm(true);
  };

  const confirmClearHistory = async () => {
    try {
      await invoke("clear_unified_history");
      setItems([]);
      setSelectedId(null);
      setShowClearConfirm(false);
    } catch (err) {
      console.error("Failed to clear history:", err);
    }
  };

  // Settings handlers
  const handleThemeChange = async (newTheme: ThemeName) => {
    setCurrentTheme(newTheme);
    await saveThemeToStore(newTheme);
  };

  const handleToggleAutostart = async () => {
    try {
      await invoke("toggle_autostart");
      setIsAutostart(!isAutostart);
    } catch (err) {
      console.error("Failed to toggle autostart:", err);
    }
  };

  const handleToggleClipboardMonitoring = async () => {
    try {
      const newSettings = {
        ...clipboardSettings,
        enabled: !clipboardSettings.enabled,
      };
      await invoke("update_clipboard_settings", { settings: newSettings });
      setClipboardSettings(newSettings);
      setIsClipboardMonitoring(!isClipboardMonitoring);
    } catch (err) {
      console.error("Failed to toggle clipboard monitoring:", err);
    }
  };

  const handleMaxItemsChange = async (value: number) => {
    try {
      const newSettings = {
        ...clipboardSettings,
        max_items: value,
      };
      await invoke("update_clipboard_settings", { settings: newSettings });
      setClipboardSettings(newSettings);
    } catch (err) {
      console.error("Failed to update max items:", err);
    }
  };

  const triggerCapture = async () => {
    try {
      await invoke("trigger_capture");
    } catch (err) {
      console.error("Failed to trigger capture:", err);
    }
  };

  const triggerColorPicker = async () => {
    try {
      await invoke("trigger_color_picker");
    } catch (err) {
      console.error("Failed to trigger color picker:", err);
    }
  };

  const formatDate = (dateStr: string) => {
    try {
      const date = parseISO(dateStr);
      return format(date, "MMM d, yyyy HH:mm:ss");
    } catch {
      return dateStr;
    }
  };

  const formatDateShort = (dateStr: string) => {
    try {
      const date = parseISO(dateStr);
      return format(date, "MMM d, HH:mm");
    } catch {
      return dateStr;
    }
  };

  const handleItemClick = (item: HistoryItem) => {
    setSelectedId(item.id);
    // Single click copies to clipboard
    copyToClipboard(item);
  };

  const handleItemDoubleClick = (item: HistoryItem) => {
    if (item.item_type === "clipboard_text") {
      setExpandedTextId(expandedTextId === item.id ? null : item.id);
    } else {
      openInEditor(item.id);
    }
  };

  const renderItemPreview = (item: HistoryItem) => {
    if (item.item_type === "clipboard_text") {
      return (
        <div
          className="w-full h-full flex items-center justify-center p-3 text-left overflow-hidden"
          style={{ backgroundColor: theme.buttonBg }}
        >
          <p
            className="text-xs line-clamp-4 w-full"
            style={{ opacity: 0.9, wordBreak: "break-word" }}
          >
            {item.text_preview || item.text_content?.slice(0, 100)}
          </p>
        </div>
      );
    }

    if (item.item_type === "color_pick" && item.color_hex) {
      return (
        <div
          className="w-full h-full flex flex-col items-center justify-center"
          style={{ backgroundColor: item.color_hex }}
        >
          <div
            className="px-2 py-1 rounded text-sm font-mono font-bold shadow-lg"
            style={{
              backgroundColor: "rgba(0,0,0,0.7)",
              color: "#fff",
            }}
          >
            {item.color_hex}
          </div>
        </div>
      );
    }

    if (item.thumbnail) {
      return (
        <img
          src={`data:image/jpeg;base64,${item.thumbnail}`}
          alt={item.item_type === "screenshot" ? "Screenshot" : "Clipboard Image"}
          className="w-full h-full object-cover"
        />
      );
    }

    return <Image size={32} style={{ opacity: 0.4 }} />;
  };

  const FilterButton = ({
    type,
    label,
    icon,
  }: {
    type: HistoryItemType | null;
    label: string;
    icon: React.ReactNode;
  }) => (
    <button
      onClick={() => setFilterType(type === filterType ? null : type)}
      style={{
        backgroundColor: filterType === type ? theme.buttonActive : theme.buttonBg,
        borderRadius: theme.borderRadius,
        color: filterType === type ? (theme.name === "Candy Pop" ? "#fff" : theme.canvasBg) : theme.textColor,
      }}
      className="px-3 py-1.5 flex items-center gap-1.5 text-sm hover:opacity-80 transition-all"
    >
      {icon}
      {label}
    </button>
  );

  const ToolButton = ({
    title,
    description,
    icon,
    onClick,
    hotkey,
    color,
  }: {
    title: string;
    description: string;
    icon: React.ReactNode;
    onClick: () => void;
    hotkey: string;
    color: string;
  }) => (
    <button
      onClick={onClick}
      style={{
        backgroundColor: theme.toolbar,
        borderRadius: theme.borderRadius,
        border: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
        '--tool-color': color,
      } as React.CSSProperties}
      className="p-6 text-left group relative overflow-hidden transition-all hover:border-[var(--tool-color)]"
    >
      <div className="flex items-start gap-4">
        <div
          className="p-3 rounded-lg"
          style={{
            backgroundColor: theme.buttonBg,
            border: `1px solid ${theme.toolbarBorder}`,
          }}
        >
          {icon}
        </div>
        <div>
          <h3 className="text-lg font-semibold">{title}</h3>
          <p className="text-sm mt-1" style={{ opacity: 0.7 }}>
            {description}
          </p>
        </div>
      </div>
      <kbd
        className="absolute bottom-3 right-4 px-2 py-1 rounded text-xs font-mono"
        style={{
          backgroundColor: theme.buttonBg,
          border: `1px solid ${theme.toolbarBorder}`,
          opacity: 0.6,
        }}
      >
        {hotkey}
      </kbd>
      <div
        className="absolute -bottom-1/2 -right-1/4 w-48 h-48 rounded-full transition-all opacity-0 group-hover:opacity-10"
        style={{ backgroundColor: 'var(--tool-color)', filter: 'blur(40px)' }}
      />
    </button>
  );

  return (
    <div
      className="h-full flex flex-col"
      style={{
        backgroundColor: theme.canvasBg,
        color: theme.textColor,
        fontFamily: theme.fontFamily,
      }}
    >
      {/* Header */}
      <div
        className="p-4"
        style={{
          backgroundColor: theme.toolbar,
          borderBottom: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
        }}
      >
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <LayoutDashboard size={28} style={{ color: theme.accentColor }} />
            <h1 className="text-2xl font-semibold">Madera.Tools</h1>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => setShowSettings(true)}
              style={{
                backgroundColor: theme.buttonBg,
                borderRadius: theme.borderRadius,
              }}
              className="p-2 hover:opacity-80 transition-colors"
              title="Settings"
            >
              <Settings size={18} />
            </button>
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 overflow-auto p-6 space-y-8">
        {/* Tools Section */}
        <div>
          <h2 className="text-xl font-semibold mb-4">Tools</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            <ToolButton
              title="New Screenshot"
              description="Capture a region of your screen."
              icon={<Camera size={24} className="text-blue-400" />}
              onClick={triggerCapture}
              hotkey="Ctrl+Shift+S"
              color="#3b82f6"
            />
            <ToolButton
              title="Color Picker"
              description="Pick any color from your screen."
              icon={<Pipette size={24} className="text-yellow-400" />}
              onClick={triggerColorPicker}
              hotkey="Ctrl+Shift+X"
              color="#f59e0b"
            />
            <ToolButton
              title="Desktop Guardian"
              description="Save and restore window layouts."
              icon={<Shield size={24} className="text-cyan-400" />}
              onClick={async () => await invoke("open_desktop_guardian")}
              hotkey="N/A"
              color="#06b6d4"
            />
          </div>
        </div>

        {/* Recent Activity Section */}
        <div>
          {/* History Header */}
          <div
            className="p-4 rounded-t-lg"
            style={{
              backgroundColor: theme.toolbar,
              borderBottom: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
            }}
          >
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-2">
                <h2 className="text-xl font-semibold">Recent Activity</h2>
                <span style={{ opacity: 0.6 }} className="text-sm">
                  ({filteredItems.length} items)
                </span>
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={loadHistory}
                  style={{
                    backgroundColor: theme.buttonBg,
                    borderRadius: theme.borderRadius,
                  }}
                  className="p-2 hover:opacity-80 transition-colors"
                  title="Refresh"
                >
                  <RefreshCw size={18} />
                </button>
                <button
                  onClick={clearAllHistory}
                  style={{ borderRadius: theme.borderRadius }}
                  className="p-2 bg-red-500/20 hover:bg-red-500/40 text-red-400 transition-colors"
                  title="Clear all history"
                >
                  <Trash size={18} />
                </button>
              </div>
            </div>

            {/* Search and Filter */}
            <div className="flex flex-col gap-3">
              <div className="flex flex-wrap items-center gap-2">
                {/* Search input */}
                <div className="relative flex-1 min-w-[200px]">
                  <Search
                    size={16}
                    className="absolute left-3 top-1/2 -translate-y-1/2"
                    style={{ opacity: 0.5 }}
                  />
                  <input
                    type="text"
                    placeholder="Search text content..."
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    style={{
                      backgroundColor: theme.buttonBg,
                      color: theme.textColor,
                      borderRadius: theme.borderRadius,
                      border: `1px ${theme.borderStyle} ${theme.toolbarBorder}`,
                    }}
                    className="w-full pl-9 pr-8 py-2 outline-none text-sm"
                  />
                  {searchQuery && (
                    <button
                      onClick={() => setSearchQuery("")}
                      className="absolute right-2 top-1/2 -translate-y-1/2 hover:opacity-80"
                    >
                      <X size={16} />
                    </button>
                  )}
                </div>

                {/* Date filter */}
                <div className="flex items-center gap-2">
                  <Calendar size={16} style={{ opacity: 0.6 }} />
                  <input
                    type="date"
                    value={searchDate}
                    onChange={(e) => setSearchDate(e.target.value)}
                    style={{
                      backgroundColor: theme.buttonBg,
                      color: theme.textColor,
                      borderRadius: theme.borderRadius,
                      border: `1px ${theme.borderStyle} ${theme.toolbarBorder}`,
                    }}
                    className="px-3 py-1.5 outline-none text-sm"
                  />
                  {searchDate && (
                    <button
                      onClick={() => setSearchDate("")}
                      className="hover:opacity-80 text-sm"
                      style={{ color: theme.accentColor }}
                    >
                      Clear
                    </button>
                  )}
                </div>
              </div>

              {/* Type filters */}
              <div className="flex items-center gap-2">
                <Filter size={16} style={{ opacity: 0.6 }} />
                <FilterButton type={null} label="All" icon={null} />
                <FilterButton
                  type="screenshot"
                  label="Screenshots"
                  icon={<Camera size={14} />}
                />
                <FilterButton
                  type="clipboard_text"
                  label="Text"
                  icon={<FileText size={14} />}
                />
                <FilterButton
                  type="clipboard_image"
                  label="Images"
                  icon={<Clipboard size={14} />}
                />
                <FilterButton
                  type="color_pick"
                  label="Colors"
                  icon={<Pipette size={14} />}
                />
              </div>
            </div>
          </div>

          {/* History Content */}
          <div className="p-4">
            {loading ? (
              <div
                className="flex items-center justify-center h-48"
                style={{ opacity: 0.6 }}
              >
                Loading...
              </div>
            ) : filteredItems.length === 0 ? (
              <div
                className="flex flex-col items-center justify-center h-48"
                style={{ opacity: 0.6 }}
              >
                <Clipboard size={48} className="mb-4 opacity-50" />
                <p className="text-lg">No activity yet.</p>
                <p className="text-sm mt-2">
                  Use the tools above or copy something to get started!
                </p>
              </div>
            ) : (
              <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4">
                {filteredItems.map((item) => (
                  <div
                    key={item.id}
                    className="group relative overflow-hidden cursor-pointer transition-all"
                    style={{
                      backgroundColor: theme.toolbar,
                      borderRadius: theme.borderRadius,
                      border: `2px ${theme.borderStyle} ${
                        selectedId === item.id
                          ? theme.accentColor
                          : item.is_pinned
                          ? theme.buttonActive
                          : theme.toolbarBorder
                      }`,
                    }}
                    onClick={() => handleItemClick(item)}
                    onDoubleClick={() => handleItemDoubleClick(item)}
                  >
                    {/* Type badge */}
                    <div
                      className="absolute top-2 left-2 z-10 flex items-center gap-1 px-2 py-0.5 rounded text-xs"
                      style={{
                        backgroundColor: `${theme.buttonBg}dd`,
                        backdropFilter: "blur(4px)",
                      }}
                    >
                      {getItemTypeIcon(item.item_type)}
                      <span style={{ opacity: 0.8 }}>{getItemTypeLabel(item.item_type)}</span>
                    </div>

                    {/* Pin indicator */}
                    {item.is_pinned && (
                      <div
                        className="absolute top-2 right-2 z-10"
                        style={{ color: theme.buttonActive }}
                      >
                        <Pin size={16} />
                      </div>
                    )}

                    {/* Preview */}
                    <div
                      className="aspect-video flex items-center justify-center overflow-hidden"
                      style={{ backgroundColor: theme.buttonBg }}
                    >
                      {renderItemPreview(item)}
                    </div>

                    {/* Info */}
                    <div className="p-2">
                      <p className="text-xs truncate" style={{ opacity: 0.7 }}>
                        {formatDateShort(item.created_at)}
                      </p>
                      {item.width && item.height && (
                        <p className="text-xs" style={{ opacity: 0.5 }}>
                          {item.width} × {item.height}
                        </p>
                      )}
                      {item.text_content && (
                        <p className="text-xs" style={{ opacity: 0.5 }}>
                          {item.text_content.length} chars
                        </p>
                      )}
                    </div>

                    {/* Hover Actions */}
                    <div className="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center gap-2">
                      {item.item_type !== "clipboard_text" && (
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            openInEditor(item.id);
                          }}
                          style={{
                            backgroundColor: theme.accentColor,
                            borderRadius: theme.borderRadius,
                          }}
                          className="p-2 hover:opacity-80 transition-colors text-white"
                          title="Edit"
                        >
                          <Edit3 size={16} />
                        </button>
                      )}
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          copyToClipboard(item);
                        }}
                        style={{
                          backgroundColor: theme.buttonBg,
                          borderRadius: theme.borderRadius,
                        }}
                        className="p-2 hover:opacity-80 transition-colors"
                        title="Copy to clipboard"
                      >
                        <Copy size={16} />
                      </button>
                      {item.item_type !== "clipboard_text" && (
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            saveToFile(item.id);
                          }}
                          style={{
                            backgroundColor: theme.buttonBg,
                            borderRadius: theme.borderRadius,
                          }}
                          className="p-2 hover:opacity-80 transition-colors"
                          title="Save to file"
                        >
                          <Save size={16} />
                        </button>
                      )}
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          togglePin(item.id);
                        }}
                        style={{
                          backgroundColor: item.is_pinned
                            ? theme.buttonActive
                            : theme.buttonBg,
                          borderRadius: theme.borderRadius,
                        }}
                        className="p-2 hover:opacity-80 transition-colors"
                        title={item.is_pinned ? "Unpin" : "Pin to top"}
                      >
                        {item.is_pinned ? <PinOff size={16} /> : <Pin size={16} />}
                      </button>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteItem(item.id);
                        }}
                        style={{ borderRadius: theme.borderRadius }}
                        className="p-2 bg-red-500/50 hover:bg-red-500 transition-colors"
                        title="Delete"
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Expanded text view */}
      {expandedTextId && (
        <div
          className="fixed inset-0 bg-black/80 flex items-center justify-center z-50 p-8"
          onClick={() => setExpandedTextId(null)}
        >
          <div
            className="max-w-3xl max-h-[80vh] w-full overflow-auto p-6 relative"
            style={{
              backgroundColor: theme.toolbar,
              borderRadius: theme.borderRadius,
              border: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              onClick={() => setExpandedTextId(null)}
              className="absolute top-4 right-4 hover:opacity-80"
            >
              <X size={24} />
            </button>
            <pre
              className="whitespace-pre-wrap text-sm"
              style={{ fontFamily: "monospace" }}
            >
              {items.find((i) => i.id === expandedTextId)?.text_content}
            </pre>
          </div>
        </div>
      )}

      {/* Selected Item Details Footer */}
      {selectedId && (
        <div
          className="p-4"
          style={{
            backgroundColor: theme.toolbar,
            borderTop: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
          }}
        >
          {(() => {
            const selected = items.find((s) => s.id === selectedId);
            if (!selected) return null;
            return (
              <div className="flex items-center justify-between">
                <div>
                  <div className="flex items-center gap-2">
                    {getItemTypeIcon(selected.item_type)}
                    <p className="text-sm" style={{ opacity: 0.8 }}>
                      {formatDate(selected.created_at)}
                    </p>
                    {selected.is_pinned && (
                      <span
                        className="text-xs px-2 py-0.5 rounded"
                        style={{ backgroundColor: theme.buttonActive }}
                      >
                        Pinned
                      </span>
                    )}
                  </div>
                  <p className="text-xs" style={{ opacity: 0.5 }}>
                    {selected.item_type === "clipboard_text"
                      ? `${selected.text_content?.length || 0} characters`
                      : selected.item_type === "color_pick"
                      ? `${selected.color_hex} • rgb(${selected.color_rgb}) • hsl(${selected.color_hsl})`
                      : `${selected.width} × ${selected.height} • ${selected.filename}`}
                  </p>
                </div>
                <div className="flex gap-2">
                  {selected.item_type === "color_pick" && selected.color_hex && (
                    <div
                      className="w-10 h-10 rounded border-2"
                      style={{
                        backgroundColor: selected.color_hex,
                        borderColor: theme.toolbarBorder,
                      }}
                    />
                  )}
                  {selected.item_type !== "clipboard_text" && selected.item_type !== "color_pick" && (
                    <button
                      onClick={() => openInEditor(selectedId)}
                      style={{
                        backgroundColor: theme.accentColor,
                        borderRadius: theme.borderRadius,
                      }}
                      className="px-4 py-2 text-white hover:opacity-80 transition-colors flex items-center gap-2"
                    >
                      <Edit3 size={16} />
                      Edit
                    </button>
                  )}
                  <button
                    onClick={() => copyToClipboard(selected)}
                    style={{
                      backgroundColor: theme.buttonBg,
                      borderRadius: theme.borderRadius,
                    }}
                    className="px-4 py-2 hover:opacity-80 transition-colors flex items-center gap-2"
                  >
                    <Copy size={16} />
                    Copy
                  </button>
                </div>
              </div>
            );
          })()}
        </div>
      )}

      {/* Settings Modal */}
      {showSettings && (
        <div
          className="fixed inset-0 bg-black/80 flex items-center justify-center z-50 p-8"
          onClick={() => setShowSettings(false)}
        >
          <div
            className="max-w-2xl w-full max-h-[85vh] overflow-auto relative"
            style={{
              backgroundColor: theme.toolbar,
              borderRadius: theme.borderRadius,
              border: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            {/* Settings Header */}
            <div
              className="sticky top-0 p-4 flex items-center justify-between"
              style={{
                backgroundColor: theme.toolbar,
                borderBottom: `1px ${theme.borderStyle} ${theme.toolbarBorder}`,
              }}
            >
              <div className="flex items-center gap-2">
                <Settings size={24} style={{ color: theme.accentColor }} />
                <h2 className="text-xl font-semibold">Settings</h2>
              </div>
              <button
                onClick={() => setShowSettings(false)}
                className="p-2 hover:opacity-80 transition-colors"
                style={{
                  backgroundColor: theme.buttonBg,
                  borderRadius: theme.borderRadius,
                }}
              >
                <X size={20} />
              </button>
            </div>

            <div className="p-6 space-y-6">
              {/* Theme Section */}
              <div>
                <div className="flex items-center gap-2 mb-3">
                  <Palette size={18} style={{ color: theme.accentColor }} />
                  <h3 className="font-medium">Theme</h3>
                </div>
                <div className="grid grid-cols-3 gap-2">
                  {(Object.keys(THEMES) as ThemeName[]).map((themeName) => (
                    <button
                      key={themeName}
                      onClick={() => handleThemeChange(themeName)}
                      style={{
                        backgroundColor:
                          currentTheme === themeName
                            ? theme.buttonActive
                            : theme.buttonBg,
                        borderRadius: theme.borderRadius,
                        border: `2px ${theme.borderStyle} ${
                          currentTheme === themeName
                            ? theme.accentColor
                            : theme.toolbarBorder
                        }`,
                      }}
                      className="p-3 text-left hover:opacity-80 transition-all relative"
                    >
                      <div className="flex items-center justify-between">
                        <span className="text-sm font-medium">
                          {THEMES[themeName].name}
                        </span>
                        {currentTheme === themeName && (
                          <Check size={16} style={{ color: theme.accentColor }} />
                        )}
                      </div>
                      <div className="mt-2 flex gap-1">
                        <div
                          className="w-4 h-4 rounded"
                          style={{ backgroundColor: THEMES[themeName].toolbar }}
                        />
                        <div
                          className="w-4 h-4 rounded"
                          style={{ backgroundColor: THEMES[themeName].accentColor }}
                        />
                        <div
                          className="w-4 h-4 rounded"
                          style={{ backgroundColor: THEMES[themeName].canvasBg }}
                        />
                      </div>
                    </button>
                  ))}
                </div>
              </div>

              {/* Clipboard Monitoring Section */}
              <div>
                <div className="flex items-center gap-2 mb-3">
                  <ClipboardCheck size={18} style={{ color: theme.accentColor }} />
                  <h3 className="font-medium">Clipboard Monitoring</h3>
                </div>
                <div className="space-y-3">
                  <div
                    className="flex items-center justify-between p-3"
                    style={{
                      backgroundColor: theme.buttonBg,
                      borderRadius: theme.borderRadius,
                    }}
                  >
                    <div>
                      <p className="font-medium">Enable Monitoring</p>
                      <p className="text-xs" style={{ opacity: 0.6 }}>
                        Automatically save copied text and images
                      </p>
                    </div>
                    <button
                      onClick={handleToggleClipboardMonitoring}
                      className="w-12 h-6 rounded-full relative transition-colors"
                      style={{
                        backgroundColor: isClipboardMonitoring
                          ? theme.accentColor
                          : theme.toolbarBorder,
                      }}
                    >
                      <div
                        className="absolute top-1 w-4 h-4 rounded-full bg-white transition-all"
                        style={{
                          left: isClipboardMonitoring ? "calc(100% - 20px)" : "4px",
                        }}
                      />
                    </button>
                  </div>

                  <div
                    className="p-3"
                    style={{
                      backgroundColor: theme.buttonBg,
                      borderRadius: theme.borderRadius,
                    }}
                  >
                    <div className="flex items-center justify-between mb-2">
                      <p className="font-medium">Max History Items</p>
                      <span style={{ color: theme.accentColor }}>
                        {clipboardSettings.max_items}
                      </span>
                    </div>
                    <input
                      type="range"
                      min="50"
                      max="500"
                      step="50"
                      value={clipboardSettings.max_items}
                      onChange={(e) => handleMaxItemsChange(Number(e.target.value))}
                      className="w-full"
                      style={{ accentColor: theme.accentColor }}
                    />
                    <div
                      className="flex justify-between text-xs mt-1"
                      style={{ opacity: 0.5 }}
                    >
                      <span>50</span>
                      <span>500</span>
                    </div>
                  </div>
                </div>
              </div>

              {/* Keyboard Shortcuts Section */}
              <div>
                <div className="flex items-center gap-2 mb-3">
                  <Keyboard size={18} style={{ color: theme.accentColor }} />
                  <h3 className="font-medium">Keyboard Shortcuts</h3>
                </div>
                <div
                  className="space-y-2 p-3"
                  style={{
                    backgroundColor: theme.buttonBg,
                    borderRadius: theme.borderRadius,
                  }}
                >
                  <div className="flex items-center justify-between">
                    <span>Take Screenshot</span>
                    <kbd
                      className="px-2 py-1 rounded text-xs font-mono"
                      style={{
                        backgroundColor: theme.toolbar,
                        border: `1px solid ${theme.toolbarBorder}`,
                      }}
                    >
                      Ctrl+Shift+S
                    </kbd>
                  </div>
                  <div className="flex items-center justify-between">
                    <span>Open History</span>
                    <kbd
                      className="px-2 py-1 rounded text-xs font-mono"
                      style={{
                        backgroundColor: theme.toolbar,
                        border: `1px solid ${theme.toolbarBorder}`,
                      }}
                    >
                      Ctrl+Shift+V
                    </kbd>
                  </div>
                  <div className="flex items-center justify-between">
                    <span>Copy (in Editor)</span>
                    <kbd
                      className="px-2 py-1 rounded text-xs font-mono"
                      style={{
                        backgroundColor: theme.toolbar,
                        border: `1px solid ${theme.toolbarBorder}`,
                      }}
                    >
                      Ctrl+Shift+C
                    </kbd>
                  </div>
                </div>
              </div>

              {/* System Section */}
              <div>
                <div className="flex items-center gap-2 mb-3">
                  <Power size={18} style={{ color: theme.accentColor }} />
                  <h3 className="font-medium">System</h3>
                </div>
                <div
                  className="flex items-center justify-between p-3"
                  style={{
                    backgroundColor: theme.buttonBg,
                    borderRadius: theme.borderRadius,
                  }}
                >
                  <div>
                    <p className="font-medium">Start with Windows</p>
                    <p className="text-xs" style={{ opacity: 0.6 }}>
                      Launch app automatically on system startup
                    </p>
                  </div>
                  <button
                    onClick={handleToggleAutostart}
                    className="w-12 h-6 rounded-full relative transition-colors"
                    style={{
                      backgroundColor: isAutostart
                        ? theme.accentColor
                        : theme.toolbarBorder,
                    }}
                  >
                    <div
                      className="absolute top-1 w-4 h-4 rounded-full bg-white transition-all"
                      style={{
                        left: isAutostart ? "calc(100% - 20px)" : "4px",
                      }}
                    />
                  </button>
                </div>
              </div>

              {/* App Info */}
              <div
                className="text-center pt-4"
                style={{
                  borderTop: `1px ${theme.borderStyle} ${theme.toolbarBorder}`,
                  opacity: 0.5,
                }}
              >
                <p className="text-sm">Madera.Tools v1.0.0</p>
                <p className="text-xs mt-1">
                  Built with Tauri + React
                </p>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Clear History Confirmation Modal */}
      {showClearConfirm && (
        <div
          className="fixed inset-0 bg-black/80 flex items-center justify-center z-50 p-8"
          onClick={() => setShowClearConfirm(false)}
        >
          <div
            className="max-w-md w-full p-6 relative"
            style={{
              backgroundColor: theme.toolbar,
              borderRadius: theme.borderRadius,
              border: `3px solid #ef4444`,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center gap-3 mb-4">
              <div className="p-3 rounded-full bg-red-500/20">
                <Trash size={28} className="text-red-500" />
              </div>
              <div>
                <h3 className="text-lg font-bold text-red-400">Clear All History?</h3>
                <p className="text-sm" style={{ opacity: 0.7 }}>This action cannot be undone</p>
              </div>
            </div>

            <div
              className="p-4 mb-4 rounded"
              style={{
                backgroundColor: "rgba(239, 68, 68, 0.1)",
                border: "1px solid rgba(239, 68, 68, 0.3)",
              }}
            >
              <p className="text-sm mb-2 font-medium text-red-400">WARNING: This will permanently delete:</p>
              <ul className="text-sm space-y-1" style={{ opacity: 0.8 }}>
                <li>• All screenshots ({items.filter(i => i.item_type === "screenshot").length} items)</li>
                <li>• All clipboard text ({items.filter(i => i.item_type === "clipboard_text").length} items)</li>
                <li>• All clipboard images ({items.filter(i => i.item_type === "clipboard_image").length} items)</li>
                <li>• All saved colors ({items.filter(i => i.item_type === "color_pick").length} items)</li>
              </ul>
            </div>

            <div className="flex gap-3">
              <button
                onClick={() => setShowClearConfirm(false)}
                style={{
                  backgroundColor: theme.buttonBg,
                  borderRadius: theme.borderRadius,
                  border: `1px ${theme.borderStyle} ${theme.toolbarBorder}`,
                }}
                className="flex-1 px-4 py-3 hover:opacity-80 transition-colors font-medium"
              >
                Cancel
              </button>
              <button
                onClick={confirmClearHistory}
                className="flex-1 px-4 py-3 bg-red-500 hover:bg-red-600 transition-colors text-white font-medium flex items-center justify-center gap-2"
                style={{ borderRadius: theme.borderRadius }}
              >
                <Trash size={18} />
                Delete Everything
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
