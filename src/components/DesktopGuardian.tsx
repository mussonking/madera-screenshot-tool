import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { load } from "@tauri-apps/plugin-store";
import { format, parseISO } from "date-fns";
import {
  Monitor,
  Layout,
  Save,
  RotateCcw,
  Trash2,
  RefreshCw,
  Shield,
  Check,
  X,
  AlertTriangle,
  Clock,
  Maximize2,
  Minimize2,
  Square,
  Settings,
} from "lucide-react";

// Theme definitions - same as History
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

interface WindowPosition {
  hwnd: number;
  title: string;
  process_name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  monitor_index: number;
  is_maximized: boolean;
  is_minimized: boolean;
}

interface MonitorLayout {
  index: number;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  is_primary: boolean;
}

interface SavedLayout {
  id: string;
  name: string;
  created_at: string;
  windows: WindowPosition[];
  is_auto_save: boolean;
}

const STORE_PATH = "settings.json";

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

const loadLayoutsFromStore = async (forceReload = false): Promise<SavedLayout[]> => {
  try {
    const store = await load(STORE_PATH);
    // Force reload from disk if requested (e.g., after backend auto-save)
    if (forceReload) {
      await store.reload();
    }
    const saved = await store.get<SavedLayout[]>("desktop_layouts");
    return saved || [];
  } catch {
    return [];
  }
};

const saveLayoutsToStore = async (layouts: SavedLayout[]): Promise<void> => {
  try {
    const store = await load(STORE_PATH);
    await store.set("desktop_layouts", layouts);
    await store.save();
  } catch {
    console.error("Failed to save layouts");
  }
};

export default function DesktopGuardian() {
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");
  const [monitors, setMonitors] = useState<MonitorLayout[]>([]);
  const [currentWindows, setCurrentWindows] = useState<WindowPosition[]>([]);
  const [savedLayouts, setSavedLayouts] = useState<SavedLayout[]>([]);
  const [selectedLayout, setSelectedLayout] = useState<SavedLayout | null>(null);
  const [loading, setLoading] = useState(true);
  const [restoreResults, setRestoreResults] = useState<Array<[string, string | null]>>([]);
  const [showResults, setShowResults] = useState(false);
  const [newLayoutName, setNewLayoutName] = useState("");
  const [showSettings, setShowSettings] = useState(false);
  const [autoSaveEnabled, setAutoSaveEnabled] = useState(true);
  const [autoSaveInterval, setAutoSaveInterval] = useState(5); // minutes

  const theme = THEMES[currentTheme];

  const loadMonitors = useCallback(async () => {
    try {
      const mons = await invoke<MonitorLayout[]>("get_desktop_monitors");
      setMonitors(mons);
    } catch (err) {
      console.error("Failed to load monitors:", err);
    }
  }, []);

  const loadCurrentWindows = useCallback(async () => {
    try {
      const windows = await invoke<WindowPosition[]>("get_current_window_layout");
      setCurrentWindows(windows);
    } catch (err) {
      console.error("Failed to load windows:", err);
    }
  }, []);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    await Promise.all([loadMonitors(), loadCurrentWindows()]);
    setLoading(false);
  }, [loadMonitors, loadCurrentWindows]);

  // Load/save guardian settings
  const loadGuardianSettings = async () => {
    try {
      const store = await load(STORE_PATH);
      const settings = await store.get<{ autoSaveEnabled?: boolean; autoSaveInterval?: number }>("guardian_settings");
      if (settings) {
        if (settings.autoSaveEnabled !== undefined) setAutoSaveEnabled(settings.autoSaveEnabled);
        if (settings.autoSaveInterval !== undefined) setAutoSaveInterval(settings.autoSaveInterval);
      }
    } catch (err) {
      console.error("Failed to load guardian settings:", err);
    }
  };

  const saveGuardianSettings = async (enabled: boolean, interval: number) => {
    try {
      const store = await load(STORE_PATH);
      await store.set("guardian_settings", { autoSaveEnabled: enabled, autoSaveInterval: interval });
      await store.save();
    } catch (err) {
      console.error("Failed to save guardian settings:", err);
    }
  };

  useEffect(() => {
    loadThemeFromStore().then(setCurrentTheme);
    loadLayoutsFromStore().then(setSavedLayouts);
    loadGuardianSettings();
    refreshAll();
  }, [refreshAll]);

  // Listen for layouts-updated event from backend auto-save
  useEffect(() => {
    const unlisten = listen("layouts-updated", async () => {
      console.log("[DesktopGuardian] Received layouts-updated event, reloading...");
      const layouts = await loadLayoutsFromStore(true); // Force reload from disk
      setSavedLayouts(layouts);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleSaveLayout = async () => {
    const name = newLayoutName.trim() || `Layout ${savedLayouts.filter((l) => !l.is_auto_save).length + 1}`;
    try {
      const layout = await invoke<SavedLayout>("save_window_layout", { name });
      const newLayouts = [layout, ...savedLayouts];
      setSavedLayouts(newLayouts);
      await saveLayoutsToStore(newLayouts);
      setNewLayoutName("");
    } catch (err) {
      console.error("Failed to save layout:", err);
    }
  };

  const handleRestoreLayout = async (layout: SavedLayout) => {
    try {
      const results = await invoke<Array<[string, string | null]>>("restore_window_layout", {
        layout,
      });
      setRestoreResults(results);
      setShowResults(true);
      // Refresh current windows after restore
      await loadCurrentWindows();
    } catch (err) {
      console.error("Failed to restore layout:", err);
    }
  };

  const handleDeleteLayout = async (layoutId: string) => {
    const newLayouts = savedLayouts.filter((l) => l.id !== layoutId);
    setSavedLayouts(newLayouts);
    await saveLayoutsToStore(newLayouts);
    if (selectedLayout?.id === layoutId) {
      setSelectedLayout(null);
    }
  };

  const formatDate = (dateStr: string) => {
    try {
      const date = parseISO(dateStr);
      return format(date, "MMM d, HH:mm");
    } catch {
      return dateStr;
    }
  };

  // Calculate virtual desktop bounds for rendering
  const getVirtualBounds = () => {
    if (monitors.length === 0) return { minX: 0, minY: 0, maxX: 1920, maxY: 1080 };
    const minX = Math.min(...monitors.map((m) => m.x));
    const minY = Math.min(...monitors.map((m) => m.y));
    const maxX = Math.max(...monitors.map((m) => m.x + m.width));
    const maxY = Math.max(...monitors.map((m) => m.y + m.height));
    return { minX, minY, maxX, maxY };
  };

  const virtualBounds = getVirtualBounds();
  const virtualWidth = virtualBounds.maxX - virtualBounds.minX;
  const virtualHeight = virtualBounds.maxY - virtualBounds.minY;
  const scale = Math.min(400 / virtualWidth, 250 / virtualHeight);

  const renderMonitorPreview = (windows: WindowPosition[], showWindows = true) => {
    return (
      <div
        className="relative"
        style={{
          width: virtualWidth * scale,
          height: virtualHeight * scale,
          backgroundColor: theme.canvasBg,
        }}
      >
        {/* Monitors */}
        {monitors.map((monitor) => (
          <div
            key={monitor.index}
            className="absolute border-2"
            style={{
              left: (monitor.x - virtualBounds.minX) * scale,
              top: (monitor.y - virtualBounds.minY) * scale,
              width: monitor.width * scale,
              height: monitor.height * scale,
              borderColor: monitor.is_primary ? theme.accentColor : theme.toolbarBorder,
              backgroundColor: `${theme.toolbar}80`,
            }}
          >
            <div
              className="absolute top-1 left-1 text-xs px-1 rounded"
              style={{ backgroundColor: theme.buttonBg, fontSize: "10px" }}
            >
              {monitor.is_primary ? "Primary" : `Monitor ${monitor.index + 1}`}
            </div>
          </div>
        ))}

        {/* Windows */}
        {showWindows &&
          windows.map((win, idx) => (
            <div
              key={idx}
              className="absolute border overflow-hidden"
              style={{
                left: (win.x - virtualBounds.minX) * scale,
                top: (win.y - virtualBounds.minY) * scale,
                width: Math.max(win.width * scale, 20),
                height: Math.max(win.height * scale, 10),
                borderColor: theme.accentColor,
                backgroundColor: `${theme.buttonActive}40`,
              }}
              title={`${win.title} (${win.process_name})`}
            >
              <div
                className="text-xs truncate px-0.5"
                style={{ fontSize: "8px", lineHeight: "10px" }}
              >
                {win.process_name.replace(".exe", "")}
              </div>
            </div>
          ))}
      </div>
    );
  };

  const manualLayouts = savedLayouts.filter((l) => !l.is_auto_save);
  const autoLayouts = savedLayouts.filter((l) => l.is_auto_save);

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
            <Shield size={28} style={{ color: theme.accentColor }} />
            <div>
              <h1 className="text-xl font-semibold">Desktop Guardian</h1>
              <p className="text-sm" style={{ opacity: 0.6 }}>
                Save & restore your window layouts
              </p>
            </div>
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
            <button
              onClick={refreshAll}
              style={{
                backgroundColor: theme.buttonBg,
                borderRadius: theme.borderRadius,
              }}
              className="p-2 hover:opacity-80 transition-colors"
              title="Refresh"
            >
              <RefreshCw size={18} />
            </button>
          </div>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4">
        {loading ? (
          <div className="flex items-center justify-center h-full" style={{ opacity: 0.6 }}>
            Loading...
          </div>
        ) : (
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {/* Current Layout */}
            <div
              className="p-4"
              style={{
                backgroundColor: theme.toolbar,
                borderRadius: theme.borderRadius,
                border: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
              }}
            >
              <div className="flex items-center justify-between mb-4">
                <div className="flex items-center gap-2">
                  <Monitor size={20} style={{ color: theme.accentColor }} />
                  <h2 className="font-semibold">Current Layout</h2>
                  <span className="text-sm" style={{ opacity: 0.6 }}>
                    ({currentWindows.length} windows)
                  </span>
                </div>
              </div>

              {/* Monitor Preview */}
              <div className="flex justify-center mb-4">
                {renderMonitorPreview(currentWindows)}
              </div>

              {/* Save Layout */}
              <div className="flex gap-2">
                <input
                  type="text"
                  placeholder="Layout name (optional)"
                  value={newLayoutName}
                  onChange={(e) => setNewLayoutName(e.target.value)}
                  style={{
                    backgroundColor: theme.buttonBg,
                    color: theme.textColor,
                    borderRadius: theme.borderRadius,
                    border: `1px ${theme.borderStyle} ${theme.toolbarBorder}`,
                  }}
                  className="flex-1 px-3 py-2 outline-none text-sm"
                />
                <button
                  onClick={handleSaveLayout}
                  style={{
                    backgroundColor: theme.accentColor,
                    borderRadius: theme.borderRadius,
                  }}
                  className="px-4 py-2 hover:opacity-80 transition-colors flex items-center gap-2 text-white"
                >
                  <Save size={16} />
                  Save
                </button>
              </div>

              {/* Current Windows List */}
              <div className="mt-4 max-h-48 overflow-auto">
                <p className="text-xs mb-2" style={{ opacity: 0.6 }}>
                  Active windows:
                </p>
                <div className="space-y-1">
                  {currentWindows.slice(0, 10).map((win, idx) => (
                    <div
                      key={idx}
                      className="flex items-center gap-2 p-2 text-sm"
                      style={{
                        backgroundColor: theme.buttonBg,
                        borderRadius: theme.borderRadius,
                      }}
                    >
                      {win.is_maximized ? (
                        <Maximize2 size={14} style={{ color: theme.accentColor }} />
                      ) : win.is_minimized ? (
                        <Minimize2 size={14} style={{ opacity: 0.5 }} />
                      ) : (
                        <Square size={14} style={{ opacity: 0.5 }} />
                      )}
                      <span className="truncate flex-1">{win.title}</span>
                      <span className="text-xs" style={{ opacity: 0.5 }}>
                        {win.process_name.replace(".exe", "")}
                      </span>
                    </div>
                  ))}
                  {currentWindows.length > 10 && (
                    <p className="text-xs text-center" style={{ opacity: 0.5 }}>
                      +{currentWindows.length - 10} more
                    </p>
                  )}
                </div>
              </div>
            </div>

            {/* Saved Layouts */}
            <div
              className="p-4"
              style={{
                backgroundColor: theme.toolbar,
                borderRadius: theme.borderRadius,
                border: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
              }}
            >
              <div className="flex items-center gap-2 mb-4">
                <Layout size={20} style={{ color: theme.accentColor }} />
                <h2 className="font-semibold">Saved Layouts</h2>
              </div>

              {savedLayouts.length === 0 ? (
                <div className="text-center py-8" style={{ opacity: 0.6 }}>
                  <Layout size={48} className="mx-auto mb-2 opacity-50" />
                  <p>No saved layouts yet</p>
                  <p className="text-sm">Save your current layout to restore it later</p>
                </div>
              ) : (
                <div className="space-y-4">
                  {/* Manual Saves */}
                  {manualLayouts.length > 0 && (
                    <div>
                      <p className="text-xs mb-2" style={{ opacity: 0.6 }}>
                        Manual saves:
                      </p>
                      <div className="space-y-2">
                        {manualLayouts.map((layout) => (
                          <div
                            key={layout.id}
                            className="p-3 cursor-pointer hover:opacity-90 transition-all"
                            style={{
                              backgroundColor:
                                selectedLayout?.id === layout.id
                                  ? `${theme.buttonActive}40`
                                  : theme.buttonBg,
                              borderRadius: theme.borderRadius,
                              border: `2px ${theme.borderStyle} ${
                                selectedLayout?.id === layout.id
                                  ? theme.accentColor
                                  : "transparent"
                              }`,
                            }}
                            onClick={() => setSelectedLayout(layout)}
                          >
                            <div className="flex items-center justify-between">
                              <div>
                                <p className="font-medium">{layout.name}</p>
                                <p className="text-xs" style={{ opacity: 0.6 }}>
                                  {formatDate(layout.created_at)} • {layout.windows.length} windows
                                </p>
                              </div>
                              <div className="flex gap-1">
                                <button
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    handleRestoreLayout(layout);
                                  }}
                                  style={{
                                    backgroundColor: theme.accentColor,
                                    borderRadius: theme.borderRadius,
                                  }}
                                  className="p-2 hover:opacity-80 transition-colors text-white"
                                  title="Restore layout"
                                >
                                  <RotateCcw size={14} />
                                </button>
                                <button
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    handleDeleteLayout(layout.id);
                                  }}
                                  className="p-2 bg-red-500/20 hover:bg-red-500/40 text-red-400 transition-colors"
                                  style={{ borderRadius: theme.borderRadius }}
                                  title="Delete layout"
                                >
                                  <Trash2 size={14} />
                                </button>
                              </div>
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Auto Saves */}
                  {autoLayouts.length > 0 && (
                    <div>
                      <p className="text-xs mb-2 flex items-center gap-1" style={{ opacity: 0.6 }}>
                        <Clock size={12} /> Auto-saves:
                      </p>
                      <div className="space-y-1">
                        {autoLayouts.map((layout) => (
                          <div
                            key={layout.id}
                            className="p-2 flex items-center justify-between text-sm"
                            style={{
                              backgroundColor: theme.buttonBg,
                              borderRadius: theme.borderRadius,
                            }}
                          >
                            <span>
                              {formatDate(layout.created_at)} ({layout.windows.length} win)
                            </span>
                            <button
                              onClick={() => handleRestoreLayout(layout)}
                              style={{ color: theme.accentColor }}
                              className="hover:opacity-80 transition-colors text-xs"
                            >
                              Restore
                            </button>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}

              {/* Selected Layout Preview */}
              {selectedLayout && (
                <div className="mt-4 pt-4" style={{ borderTop: `1px ${theme.borderStyle} ${theme.toolbarBorder}` }}>
                  <p className="text-xs mb-2" style={{ opacity: 0.6 }}>
                    Preview: {selectedLayout.name}
                  </p>
                  <div className="flex justify-center">
                    {renderMonitorPreview(selectedLayout.windows)}
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {/* Restore Results Modal */}
      {showResults && (
        <div
          className="fixed inset-0 bg-black/80 flex items-center justify-center z-50 p-8"
          onClick={() => setShowResults(false)}
        >
          <div
            className="max-w-lg w-full p-6 relative max-h-[80vh] overflow-auto"
            style={{
              backgroundColor: theme.toolbar,
              borderRadius: theme.borderRadius,
              border: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              onClick={() => setShowResults(false)}
              className="absolute top-4 right-4 hover:opacity-80"
            >
              <X size={24} />
            </button>

            <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
              <RotateCcw size={20} style={{ color: theme.accentColor }} />
              Restore Results
            </h3>

            <div className="space-y-2">
              {restoreResults.map(([name, error], idx) => (
                <div
                  key={idx}
                  className="flex items-center gap-2 p-2"
                  style={{
                    backgroundColor: theme.buttonBg,
                    borderRadius: theme.borderRadius,
                  }}
                >
                  {error ? (
                    <AlertTriangle size={16} className="text-yellow-500 flex-shrink-0" />
                  ) : (
                    <Check size={16} className="text-green-500 flex-shrink-0" />
                  )}
                  <div className="flex-1 min-w-0">
                    <p className="truncate text-sm">{name}</p>
                    {error && (
                      <p className="text-xs text-yellow-500 truncate">{error}</p>
                    )}
                  </div>
                </div>
              ))}
            </div>

            <div className="mt-4 text-center">
              <p className="text-sm" style={{ opacity: 0.6 }}>
                {restoreResults.filter(([_, err]) => !err).length} of{" "}
                {restoreResults.length} windows restored
              </p>
            </div>
          </div>
        </div>
      )}

      {/* Settings Modal */}
      {showSettings && (
        <div
          className="fixed inset-0 bg-black/80 flex items-center justify-center z-50 p-8"
          onClick={() => setShowSettings(false)}
        >
          <div
            className="max-w-md w-full p-6 relative"
            style={{
              backgroundColor: theme.toolbar,
              borderRadius: theme.borderRadius,
              border: `2px ${theme.borderStyle} ${theme.toolbarBorder}`,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <button
              onClick={() => setShowSettings(false)}
              className="absolute top-4 right-4 hover:opacity-80"
            >
              <X size={24} />
            </button>

            <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
              <Settings size={20} style={{ color: theme.accentColor }} />
              Settings
            </h3>

            <div className="space-y-4">
              {/* Auto-save toggle */}
              <div
                className="flex items-center justify-between p-3"
                style={{
                  backgroundColor: theme.buttonBg,
                  borderRadius: theme.borderRadius,
                }}
              >
                <div>
                  <p className="font-medium">Auto-save layouts</p>
                  <p className="text-xs" style={{ opacity: 0.6 }}>
                    Automatically save window positions
                  </p>
                </div>
                <button
                  onClick={() => {
                    const newEnabled = !autoSaveEnabled;
                    setAutoSaveEnabled(newEnabled);
                    saveGuardianSettings(newEnabled, autoSaveInterval);
                  }}
                  className="w-12 h-6 rounded-full relative transition-colors"
                  style={{
                    backgroundColor: autoSaveEnabled ? theme.accentColor : theme.toolbarBorder,
                  }}
                >
                  <div
                    className="absolute top-1 w-4 h-4 rounded-full bg-white transition-all"
                    style={{
                      left: autoSaveEnabled ? "calc(100% - 20px)" : "4px",
                    }}
                  />
                </button>
              </div>

              {/* Auto-save interval */}
              <div
                className="p-3"
                style={{
                  backgroundColor: theme.buttonBg,
                  borderRadius: theme.borderRadius,
                }}
              >
                <div className="flex items-center justify-between mb-2">
                  <p className="font-medium">Auto-save interval</p>
                  <span style={{ color: theme.accentColor }}>{autoSaveInterval} min</span>
                </div>
                <input
                  type="range"
                  min="1"
                  max="30"
                  step="1"
                  value={autoSaveInterval}
                  onChange={(e) => {
                    const newInterval = Number(e.target.value);
                    setAutoSaveInterval(newInterval);
                    saveGuardianSettings(autoSaveEnabled, newInterval);
                  }}
                  className="w-full"
                  style={{ accentColor: theme.accentColor }}
                  disabled={!autoSaveEnabled}
                />
                <div className="flex justify-between text-xs mt-1" style={{ opacity: 0.5 }}>
                  <span>1 min</span>
                  <span>30 min</span>
                </div>
              </div>

              <div className="pt-4 text-center" style={{ opacity: 0.5, borderTop: `1px ${theme.borderStyle} ${theme.toolbarBorder}` }}>
                <p className="text-sm">Desktop Guardian protects your workflow</p>
                <p className="text-xs mt-1">Never lose your window layout again!</p>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
