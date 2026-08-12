import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import {
  Camera, ScrollText, Palette, Clipboard, ArrowRight, Clock, Image, FileText, Pipette,
  Edit3, Copy, Save, Pin, PinOff, Trash2, Check
} from "lucide-react";
import { THEMES, ThemeName, loadThemeFromStore } from "../utils/theme";
import type { HistoryItem } from "../stores/appStore";
import type { MainTab } from "./AppShell";

const isImageLike = (item: HistoryItem) =>
  item.item_type === "screenshot" || item.item_type === "clipboard_image";

export default function CaptureHome({ onNavigate }: { onNavigate: (tab: MainTab) => void }) {
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");
  const [recents, setRecents] = useState<HistoryItem[]>([]);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const theme = THEMES[currentTheme];

  const loadRecents = useCallback(async () => {
    try {
      const items = await invoke<HistoryItem[]>("get_unified_history", {
        filterType: null,
        limit: 6,
        offset: 0,
      });
      setRecents(items);
    } catch (err) {
      console.error("Failed to load recents:", err);
    }
  }, []);

  useEffect(() => {
    loadThemeFromStore().then(setCurrentTheme);
    loadRecents();

    const unlistenClipboard = listen("clipboard-changed", () => loadRecents());
    window.addEventListener("focus", loadRecents);
    return () => {
      unlistenClipboard.then((fn) => fn());
      window.removeEventListener("focus", loadRecents);
    };
  }, [loadRecents]);

  const copyItem = async (item: HistoryItem) => {
    try {
      if (item.item_type === "clipboard_text" && item.text_content) {
        await invoke("copy_text_to_clipboard", { text: item.text_content });
      } else if (item.item_type === "color_pick" && item.color_hex) {
        await invoke("copy_text_to_clipboard", { text: item.color_hex });
      } else {
        const imageData = await invoke<string | null>("get_history_item_image", { id: item.id });
        if (imageData) {
          await invoke("copy_to_clipboard", { imageData });
        }
      }
      setCopiedId(item.id);
      setTimeout(() => setCopiedId(null), 1200);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  };

  const openInEditor = async (item: HistoryItem) => {
    try {
      const imageData = await invoke<string | null>("get_history_item_image", { id: item.id });
      if (imageData && item.width && item.height) {
        await invoke("open_editor_with_image", {
          imageData,
          width: item.width,
          height: item.height,
        });
      }
    } catch (err) {
      console.error("Failed to open in editor:", err);
    }
  };

  const saveToFile = async (item: HistoryItem) => {
    try {
      if (!item.filename) return;
      const path = await save({
        filters: [{ name: "PNG Image", extensions: ["png"] }],
        defaultPath: item.filename,
      });
      if (path) {
        const imageData = await invoke<string | null>("get_history_item_image", { id: item.id });
        if (imageData) {
          await invoke("save_image_to_file", { imageData, path });
        }
      }
    } catch (err) {
      console.error("Failed to save:", err);
    }
  };

  const togglePin = async (item: HistoryItem) => {
    try {
      await invoke("toggle_pin_item", { id: item.id });
      loadRecents();
    } catch (err) {
      console.error("Failed to toggle pin:", err);
    }
  };

  const deleteItem = async (item: HistoryItem) => {
    try {
      await invoke("delete_history_item", { id: item.id });
      loadRecents();
    } catch (err) {
      console.error("Failed to delete:", err);
    }
  };

  const renderRecentPreview = (item: HistoryItem) => {
    if (item.item_type === "clipboard_text") {
      return (
        <div className="w-full h-full flex items-center justify-center p-2 overflow-hidden" style={{ backgroundColor: theme.buttonBg }}>
          <p className="text-[10px] line-clamp-3 w-full" style={{ opacity: 0.85, wordBreak: "break-word" }}>
            {item.text_preview || item.text_content?.slice(0, 80)}
          </p>
        </div>
      );
    }
    if (item.item_type === "color_pick" && item.color_hex) {
      return (
        <div className="w-full h-full flex items-center justify-center" style={{ backgroundColor: item.color_hex }}>
          <span className="px-1.5 py-0.5 rounded text-[10px] font-mono font-bold" style={{ backgroundColor: "rgba(0,0,0,0.7)", color: "#fff" }}>
            {item.color_hex}
          </span>
        </div>
      );
    }
    if (item.thumbnail) {
      return <img src={`data:image/jpeg;base64,${item.thumbnail}`} alt="" className="w-full h-full object-cover" />;
    }
    return <Image size={20} style={{ opacity: 0.4 }} />;
  };

  const recentIcon = (item: HistoryItem) => {
    switch (item.item_type) {
      case "screenshot": return <Camera size={11} style={{ color: "#60a5fa" }} />;
      case "clipboard_text": return <FileText size={11} style={{ color: "#4ade80" }} />;
      case "clipboard_image": return <Clipboard size={11} style={{ color: "#c084fc" }} />;
      case "color_pick": return <Pipette size={11} style={{ color: "#facc15" }} />;
      default: return <Clock size={11} style={{ opacity: 0.5 }} />;
    }
  };

  const tools = [
    {
      id: "scroll-capture",
      name: "Scroll Capture",
      description: "Capture a scrolling page",
      icon: ScrollText,
      action: () => invoke("capture_scrolling", { scrollCount: 5, scrollDelayMs: 400 }),
    },
    {
      id: "color-picker",
      name: "Color Picker",
      description: "Pick any color on screen",
      icon: Palette,
      action: () => invoke("trigger_color_picker"),
    },
    {
      id: "quick-paste",
      name: "Quick Paste",
      description: "Paste history near your cursor",
      icon: Clipboard,
      action: () => invoke("open_quick_paste_panel", { tab: "history" }),
    },
  ];

  const shortcuts = [
    { keys: "Ctrl+Shift+S", label: "Capture a region" },
    { keys: "Ctrl+Shift+H", label: "Open History" },
    { keys: "Ctrl+Shift+X", label: "Color picker" },
    { keys: "Ctrl+Alt+V", label: "Quick Paste panel" },
  ];

  return (
    <div className="flex-1 min-h-0 w-full flex flex-col relative" style={{ backgroundColor: theme.canvasBg, color: theme.textColor, fontFamily: theme.fontFamily }}>
      <div className="flex-1 min-h-0 overflow-y-auto w-full">
        <div className="max-w-4xl mx-auto p-8 flex flex-col gap-8">
          {/* Hero: primary capture action */}
          <div
            className="rounded-xl p-10 flex flex-col items-center text-center"
            style={{ backgroundColor: theme.toolbar, border: `2px ${theme.borderStyle} ${theme.toolbarBorder}` }}
          >
            <button
              onClick={() => invoke("trigger_capture")}
              className="flex items-center gap-3 px-10 py-5 text-xl font-bold text-white transition-transform hover:scale-[1.03] active:scale-[0.98]"
              style={{ backgroundColor: theme.accentColor, borderRadius: theme.borderRadius }}
            >
              <Camera size={28} />
              Take a Screenshot
            </button>
            <p className="mt-4 text-sm" style={{ opacity: 0.6 }}>
              or press <b>Ctrl+Shift+S</b> anywhere, in any app
            </p>
          </div>

          {/* Secondary tools */}
          <div>
            <h3 className="text-sm font-semibold uppercase tracking-wider mb-3" style={{ opacity: 0.6 }}>
              More tools
            </h3>
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
              {tools.map((tool) => {
                const Icon = tool.icon;
                return (
                  <button
                    key={tool.id}
                    onClick={tool.action}
                    className="flex items-start gap-3 p-4 text-left hover:opacity-85 transition-opacity"
                    style={{ backgroundColor: theme.toolbar, borderRadius: theme.borderRadius, border: `2px ${theme.borderStyle} ${theme.toolbarBorder}` }}
                  >
                    <div className="p-2 rounded-lg flex-shrink-0" style={{ backgroundColor: theme.buttonBg }}>
                      <Icon size={18} style={{ color: theme.accentColor }} />
                    </div>
                    <div className="min-w-0">
                      <p className="text-sm font-semibold">{tool.name}</p>
                      <p className="text-xs mt-0.5" style={{ opacity: 0.55 }}>{tool.description}</p>
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Recent activity */}
          <div>
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-sm font-semibold uppercase tracking-wider" style={{ opacity: 0.6 }}>
                Recent activity
              </h3>
              <button
                onClick={() => onNavigate("history")}
                className="flex items-center gap-1.5 text-sm font-medium hover:opacity-80 transition-opacity"
                style={{ color: theme.accentColor }}
              >
                View all <ArrowRight size={14} />
              </button>
            </div>

            {recents.length === 0 ? (
              <div
                className="rounded-lg p-8 text-center text-sm"
                style={{ backgroundColor: theme.toolbar, border: `2px ${theme.borderStyle} ${theme.toolbarBorder}`, opacity: 0.6 }}
              >
                Nothing captured yet. Take a screenshot or copy something to get started!
              </div>
            ) : (
              <div className="grid grid-cols-3 sm:grid-cols-6 gap-3">
                {recents.map((item) => (
                  <div
                    key={item.id}
                    className="group relative overflow-hidden aspect-square cursor-pointer"
                    title="Click to copy back to clipboard"
                    style={{
                      backgroundColor: theme.toolbar,
                      borderRadius: theme.borderRadius,
                      border: `2px ${theme.borderStyle} ${item.is_pinned ? theme.buttonActive : theme.toolbarBorder}`,
                    }}
                    onClick={() => copyItem(item)}
                  >
                    {renderRecentPreview(item)}

                    {/* Type badge */}
                    <div className="absolute top-1 left-1 p-1 rounded pointer-events-none" style={{ backgroundColor: `${theme.buttonBg}dd` }}>
                      {recentIcon(item)}
                    </div>

                    {/* Pinned indicator */}
                    {item.is_pinned && (
                      <div className="absolute top-1 right-1 pointer-events-none" style={{ color: theme.buttonActive }}>
                        <Pin size={12} />
                      </div>
                    )}

                    {/* Copied feedback */}
                    {copiedId === item.id && (
                      <div className="absolute inset-0 bg-black/60 flex items-center justify-center pointer-events-none">
                        <span className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-full text-xs font-semibold text-white" style={{ backgroundColor: theme.accentColor }}>
                          <Check size={13} /> Copied!
                        </span>
                      </div>
                    )}

                    {/* Hover quick actions */}
                    {copiedId !== item.id && (
                      <div
                        className="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center gap-1 flex-wrap p-1"
                        onClick={(e) => e.stopPropagation()}
                      >
                        {isImageLike(item) && (
                          <button
                            onClick={() => openInEditor(item)}
                            className="p-1.5 hover:opacity-80 transition-opacity text-white"
                            style={{ backgroundColor: theme.accentColor, borderRadius: theme.borderRadius }}
                            title="Edit"
                          >
                            <Edit3 size={12} />
                          </button>
                        )}
                        <button
                          onClick={() => copyItem(item)}
                          className="p-1.5 hover:opacity-80 transition-opacity"
                          style={{ backgroundColor: theme.buttonBg, borderRadius: theme.borderRadius }}
                          title="Copy to clipboard"
                        >
                          <Copy size={12} />
                        </button>
                        {isImageLike(item) && (
                          <button
                            onClick={() => saveToFile(item)}
                            className="p-1.5 hover:opacity-80 transition-opacity"
                            style={{ backgroundColor: theme.buttonBg, borderRadius: theme.borderRadius }}
                            title="Save to file"
                          >
                            <Save size={12} />
                          </button>
                        )}
                        <button
                          onClick={() => togglePin(item)}
                          className="p-1.5 hover:opacity-80 transition-opacity"
                          style={{
                            backgroundColor: item.is_pinned ? theme.buttonActive : theme.buttonBg,
                            borderRadius: theme.borderRadius,
                          }}
                          title={item.is_pinned ? "Unpin" : "Pin to top"}
                        >
                          {item.is_pinned ? <PinOff size={12} /> : <Pin size={12} />}
                        </button>
                        <button
                          onClick={() => deleteItem(item)}
                          className="p-1.5 bg-red-500/50 hover:bg-red-500 transition-colors text-white"
                          style={{ borderRadius: theme.borderRadius }}
                          title="Delete"
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Shortcuts cheat sheet */}
          <div
            className="rounded-lg p-4 grid grid-cols-1 sm:grid-cols-2 gap-x-8 gap-y-2"
            style={{ backgroundColor: theme.toolbar, border: `2px ${theme.borderStyle} ${theme.toolbarBorder}` }}
          >
            {shortcuts.map((s) => (
              <div key={s.keys} className="flex items-center justify-between text-sm py-1">
                <span style={{ opacity: 0.7 }}>{s.label}</span>
                <kbd
                  className="px-2 py-0.5 rounded text-xs font-mono font-semibold"
                  style={{ backgroundColor: theme.buttonBg, border: `1px ${theme.borderStyle} ${theme.toolbarBorder}` }}
                >
                  {s.keys}
                </kbd>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
