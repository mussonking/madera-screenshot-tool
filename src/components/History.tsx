import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
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
  Pipette,
} from "lucide-react";
import type { HistoryItem, HistoryItemType } from "../stores/appStore";
import { THEMES, ThemeName, loadThemeFromStore } from "../utils/theme";



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
  const [offset, setOffset] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const LIMIT = 30;

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchDate, setSearchDate] = useState("");
  const [filterType, setFilterType] = useState<HistoryItemType | null>(null);
  const [loading, setLoading] = useState(true);
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");
  const [expandedTextId, setExpandedTextId] = useState<string | null>(null);

  // Clear confirmation state
  const [showClearConfirm, setShowClearConfirm] = useState(false);

  const theme = THEMES[currentTheme];

  const loadHistory = useCallback(async (reset = false) => {
    if (reset) {
      setLoading(true);
    }
    try {
      const currentOffset = reset ? 0 : offset;
      const history = await invoke<HistoryItem[]>("get_unified_history", {
        filterType: filterType,
        limit: LIMIT,
        offset: currentOffset,
      });

      if (reset) {
        setItems(history);
      } else {
        setItems((prev) => {
          // Prevent duplicates by checking IDs
          const existingIds = new Set(prev.map(i => i.id));
          const newItems = history.filter(i => !existingIds.has(i.id));
          return [...prev, ...newItems];
        });
      }

      setOffset(currentOffset + LIMIT);
      setHasMore(history.length === LIMIT);
    } catch (err) {
      console.error("Failed to load history:", err);
    }
    setLoading(false);
  }, [filterType, offset]);



  useEffect(() => {
    loadThemeFromStore().then(setCurrentTheme);
    loadHistory(true);

    // Listen for clipboard changes
    const unlisten = listen("clipboard-changed", () => {
      loadHistory(true);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Reload when filter type changes
  useEffect(() => {
    loadHistory(true);
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



  return (
    <div
      className="flex-1 overflow-hidden w-full flex flex-col relative"
      style={{
        backgroundColor: theme.canvasBg,
        color: theme.textColor,
        fontFamily: theme.fontFamily,
      }}
    >
      {/* Main Content */}
      <div className="flex-1 overflow-y-auto w-full p-6 space-y-8">
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
                  onClick={() => loadHistory(true)}
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
                      border: `2px ${theme.borderStyle} ${selectedId === item.id
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

            {filteredItems.length > 0 && hasMore && !searchQuery && !searchDate && (
              <div className="mt-8 mb-4 flex justify-center">
                <button
                  onClick={() => loadHistory(false)}
                  className="px-6 py-2 hover:opacity-80 transition-colors font-medium flex items-center gap-2"
                  style={{
                    backgroundColor: theme.buttonBg,
                    color: theme.textColor,
                    borderRadius: theme.borderRadius,
                    border: `1px ${theme.borderStyle} ${theme.toolbarBorder}`,
                  }}
                >
                  <RefreshCw size={16} />
                  Load More
                </button>
              </div>
            )}
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
    </div>
  );
}
