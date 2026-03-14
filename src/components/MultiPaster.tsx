import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalPosition } from "@tauri-apps/api/window";
import { format, parseISO } from "date-fns";
import { GripHorizontal } from "lucide-react";
import {
  Image,
  FileText,
  Pipette,
  Camera,
  X,
  Clipboard,
  Check,
} from "lucide-react";
import type { HistoryItem } from "../stores/appStore";

const MultiPaster = () => {
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  // Pagination state
  const [offset, setOffset] = useState(0);
  const [hasMore, setHasMore] = useState(true);
  const LIMIT = 15;

  // Drag state
  const isDragging = useRef(false);
  const dragStart = useRef({ x: 0, y: 0 });
  const windowStart = useRef({ x: 0, y: 0 });

  const loadHistory = useCallback(async (reset = false) => {
    if (reset) setLoading(true);
    try {
      const currentOffset = reset ? 0 : offset;
      const history = await invoke<HistoryItem[]>("get_unified_history", {
        filterType: null,
        limit: LIMIT,
        offset: currentOffset,
      });

      if (reset) {
        setItems(history);
      } else {
        setItems((prev) => {
          const existingIds = new Set(prev.map(i => i.id));
          const newItems = history.filter(i => !existingIds.has(i.id));
          return [...prev, ...newItems];
        });
      }

      setOffset(currentOffset + LIMIT);
      setHasMore(history.length === LIMIT);
    } catch (error) {
      console.error("Failed to load history:", error);
    } finally {
      if (reset) setLoading(false);
    }
  }, [offset]);

  useEffect(() => {
    loadHistory(true);

    // Close on Escape
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        closeWindow();
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [loadHistory]);

  const closeWindow = async () => {
    try {
      await invoke("close_multi_paste");
    } catch {
      const win = getCurrentWindow();
      await win.close();
    }
  };

  const handlePaste = async (itemId: string) => {
    try {
      await invoke("paste_history_item", { itemId });
      // Show copied feedback
      setCopiedId(itemId);
      setTimeout(() => setCopiedId(null), 1000);
    } catch (error) {
      console.error("Failed to paste item:", error);
    }
  };

  // Right-click to copy to clipboard (without pasting)
  const handleCopyToClipboard = async (e: React.MouseEvent, itemId: string) => {
    e.preventDefault();
    try {
      await invoke("copy_history_item_to_clipboard", { itemId });
      setCopiedId(itemId);
      setTimeout(() => setCopiedId(null), 1000);
    } catch (error) {
      console.error("Failed to copy item:", error);
    }
  };

  // Manual drag implementation
  const handleDragStart = async (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest('button')) return;

    isDragging.current = true;
    dragStart.current = { x: e.screenX, y: e.screenY };

    try {
      const win = getCurrentWindow();
      const pos = await win.outerPosition();
      windowStart.current = { x: pos.x, y: pos.y };
    } catch {
      windowStart.current = { x: 0, y: 0 };
    }

    document.addEventListener('mousemove', handleDragMove);
    document.addEventListener('mouseup', handleDragEnd);
  };

  const handleDragMove = async (e: MouseEvent) => {
    if (!isDragging.current) return;

    const deltaX = e.screenX - dragStart.current.x;
    const deltaY = e.screenY - dragStart.current.y;

    try {
      const win = getCurrentWindow();
      await win.setPosition(new LogicalPosition(
        windowStart.current.x + deltaX,
        windowStart.current.y + deltaY
      ));
    } catch (error) {
      console.error("Failed to move window:", error);
    }
  };

  const handleDragEnd = () => {
    isDragging.current = false;
    document.removeEventListener('mousemove', handleDragMove);
    document.removeEventListener('mouseup', handleDragEnd);
  };

  const getItemIcon = (type: string) => {
    switch (type) {
      case "screenshot":
        return <Camera size={14} className="text-blue-400" />;
      case "clipboard_image":
        return <Image size={14} className="text-green-400" />;
      case "clipboard_text":
        return <FileText size={14} className="text-yellow-400" />;
      case "color_pick":
        return <Pipette size={14} className="text-purple-400" />;
      default:
        return <Clipboard size={14} className="text-gray-400" />;
    }
  };

  const getItemPreview = (item: HistoryItem) => {
    switch (item.item_type) {
      case "clipboard_text":
        return (
          <span className="text-gray-300 text-sm truncate">
            {item.text_preview || item.text_content?.slice(0, 50) || "Empty text"}
          </span>
        );
      case "color_pick":
        return (
          <div className="flex items-center gap-2">
            <div
              className="w-5 h-5 rounded border border-gray-600"
              style={{ backgroundColor: item.color_hex || "#000" }}
            />
            <span className="text-gray-300 text-sm font-mono">
              {item.color_hex}
            </span>
          </div>
        );
      case "screenshot":
      case "clipboard_image":
        return (
          <div className="flex items-center gap-2">
            {item.thumbnail && (
              <img
                src={`data:image/jpeg;base64,${item.thumbnail}`}
                alt="preview"
                className="w-10 h-10 object-cover rounded"
              />
            )}
            <span className="text-gray-400 text-xs">
              {item.width}x{item.height}
            </span>
          </div>
        );
      default:
        return <span className="text-gray-500 text-sm">Unknown</span>;
    }
  };

  const formatTime = (dateStr: string) => {
    try {
      return format(parseISO(dateStr), "HH:mm");
    } catch {
      return "";
    }
  };

  return (
    <div className="h-full w-full bg-[#1a1a2e] rounded-lg border border-[#0f3460] shadow-2xl overflow-hidden flex flex-col">
      {/* Header - draggable */}
      <div
        className="flex items-center justify-between px-3 py-2 bg-[#16213e] border-b border-[#0f3460] cursor-move select-none"
        onMouseDown={handleDragStart}
      >
        <div className="flex items-center gap-2">
          <GripHorizontal size={16} className="text-gray-500" />
          <Clipboard size={16} className="text-[#e94560]" />
          <span className="text-gray-200 text-sm font-medium">Quick Paste</span>
        </div>
        <button
          onClick={closeWindow}
          className="text-gray-400 hover:text-white p-1 rounded hover:bg-[#0f3460] transition-colors"
        >
          <X size={14} />
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-full">
            <span className="text-gray-500">Loading...</span>
          </div>
        ) : items.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-gray-500">
            <Clipboard size={32} className="mb-2 opacity-50" />
            <span className="text-sm">No clipboard history</span>
          </div>
        ) : (
          <div className="py-1">
            {items.map((item, index) => (
              <button
                key={item.id}
                onClick={() => handlePaste(item.id)}
                onContextMenu={(e) => handleCopyToClipboard(e, item.id)}
                className={`w-full px-3 py-2 flex items-center gap-3 hover:bg-[#0f3460] transition-all text-left group ${copiedId === item.id ? "bg-green-900/30" : ""
                  }`}
              >
                {/* Index number or check */}
                <span className="text-gray-600 text-xs w-4 text-right">
                  {copiedId === item.id ? (
                    <Check size={12} className="text-green-400" />
                  ) : (
                    index + 1
                  )}
                </span>

                {/* Icon */}
                {getItemIcon(item.item_type)}

                {/* Preview */}
                <div className="flex-1 min-w-0 overflow-hidden">
                  {getItemPreview(item)}
                </div>

                {/* Time or "Pasted!" */}
                <span className={`text-xs transition-opacity ${copiedId === item.id
                    ? "text-green-400 opacity-100"
                    : "text-gray-600 opacity-0 group-hover:opacity-100"
                  }`}>
                  {copiedId === item.id ? "Pasted!" : formatTime(item.created_at)}
                </span>
              </button>
            ))}

            {hasMore && items.length > 0 && (
              <button
                onClick={() => loadHistory(false)}
                className="w-full text-center py-3 text-xs text-blue-400 hover:text-blue-300 hover:bg-[#0f3460] transition-colors"
              >
                Load Older Items...
              </button>
            )}
          </div>
        )}
      </div>

      {/* Footer hint */}
      <div className="px-3 py-1.5 bg-[#16213e] border-t border-[#0f3460]">
        <span className="text-gray-500 text-xs">
          Click = paste | Right-click = copy | ESC = close
        </span>
      </div>
    </div>
  );
};

export default MultiPaster;
