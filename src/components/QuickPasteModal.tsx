import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { GripHorizontal, X, FileText, Image, PenLine, Trash2, Copy, Check, ArrowLeftRight, Monitor } from "lucide-react";
import { THEMES, ThemeName, loadThemeFromStore } from "../utils/theme";

export interface SnippetItem {
  id: string;
  title: string;
  content_type: string;
  content: string;
  created_at: string;
}

const QuickPasteModal = () => {
  const [snippets, setSnippets] = useState<SnippetItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");

  const isPasting = useRef(false);

  const loadSnippets = useCallback(async () => {
    try {
      setLoading(true);
      const items = await invoke<SnippetItem[]>("get_snippets");
      setSnippets(items);
    } catch (error) {
      console.error("Failed to load snippets:", error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSnippets();
    loadThemeFromStore().then(setCurrentTheme);

    // Key handlers
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        closeWindow();
      }
    };
    
    // Paste handler
    const handlePaste = async (e: ClipboardEvent) => {
      // Prevent capturing our own simulated Ctrl+V
      if (isPasting.current) return;

      const items = e.clipboardData?.items;
      if (!items) return;

      for (let i = 0; i < items.length; i++) {
        const item = items[i];
        if (item.type.startsWith("image/")) {
          const file = item.getAsFile();
          if (file) {
            const reader = new FileReader();
            reader.onload = async (ev) => {
              const base64 = (ev.target?.result as string)?.split(",")[1];
              if (base64) {
                await invoke("add_snippet", {
                  title: "New Image Snippet",
                  contentType: "image",
                  content: base64,
                });
                loadSnippets();
              }
            };
            reader.readAsDataURL(file);
          }
          break; // Stop at first valid item
        } else if (item.type === "text/plain") {
          item.getAsString(async (text) => {
            if (text.trim()) {
              await invoke("add_snippet", {
                title: "New Text Snippet",
                contentType: "text",
                content: text,
              });
              loadSnippets();
            }
          });
          break; // Stop at first valid item
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    document.addEventListener("paste", handlePaste);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("paste", handlePaste);
    };
  }, [loadSnippets]);

  const closeWindow = async () => {
    try {
      const win = getCurrentWindow();
      await win.close();
    } catch (e) {
      console.error("Failed to close window:", e);
    }
  };

  const handleCopyAndPaste = async (id: string) => {
    try {
      // Temporarily block paste events to avoid duplicates when simulating Ctrl+V
      isPasting.current = true;
      
      await invoke("paste_snippet_item", { itemId: id });
      setCopiedId(id);
      
      // Keep blocked long enough for the OS simulated paste to finish
      setTimeout(() => {
        setCopiedId(null);
        isPasting.current = false;
      }, 1000);
    } catch (error) {
      console.error("Failed to paste snippet:", error);
      isPasting.current = false;
    }
  };

  const handleCopyToClipboard = async (e: React.MouseEvent, id: string) => {
    e.preventDefault();
    try {
      await invoke("copy_snippet_to_clipboard", { itemId: id });
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 1000);
    } catch (error) {
      console.error("Failed to copy snippet:", error);
    }
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    try {
      await invoke("delete_snippet", { id });
      loadSnippets();
    } catch (error) {
      console.error("Failed to delete snippet:", error);
    }
  };

  const handleEditTitle = async (e: React.MouseEvent, snippet: SnippetItem) => {
    e.stopPropagation();
    const newTitle = prompt("Enter new title for this snippet:", snippet.title);
    if (newTitle !== null && newTitle.trim() !== "") {
      try {
        await invoke("update_snippet", {
          id: snippet.id,
          title: newTitle.trim(),
          content: snippet.content
        });
        loadSnippets();
      } catch (error) {
        console.error("Failed to update snippet title:", error);
      }
    }
  };

  const handleDragStart = async (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest('button')) return;
    await getCurrentWindow().startDragging();
  };

  const theme = THEMES[currentTheme];

  return (
    <div 
      className="h-full w-full rounded-lg shadow-2xl overflow-hidden flex flex-col font-sans border"
      style={{
        backgroundColor: theme.canvasBg,
        color: theme.textColor,
        borderColor: theme.toolbarBorder,
        fontFamily: theme.fontFamily,
      }}
    >
      {/* Header - draggable */}
      <div
        className="flex items-center justify-between px-3 py-2 cursor-move select-none border-b"
        style={{
          backgroundColor: theme.toolbar,
          borderColor: theme.toolbarBorder,
        }}
        onMouseDown={handleDragStart}
      >
        <div className="flex items-center gap-2">
          <GripHorizontal size={16} style={{ color: theme.textColor, opacity: 0.5 }} />
          <Copy size={16} style={{ color: theme.accentColor }} />
          <span className="text-sm font-medium" style={{ color: theme.textColor }}>Quick Prompt Snippets</span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => invoke('toggle_panel_side', { windowLabel: 'quickpaste' })}
            className="p-1 rounded transition-colors hover:opacity-80"
            style={{ color: theme.textColor, backgroundColor: 'transparent' }}
            onMouseOver={(e) => e.currentTarget.style.backgroundColor = theme.buttonBg}
            onMouseOut={(e) => e.currentTarget.style.backgroundColor = 'transparent'}
            title="Move left/right"
          >
            <ArrowLeftRight size={14} />
          </button>
          <button
            onClick={() => invoke('toggle_panel_monitor', { windowLabel: 'quickpaste' })}
            className="p-1 rounded transition-colors hover:opacity-80"
            style={{ color: theme.textColor, backgroundColor: 'transparent' }}
            onMouseOver={(e) => e.currentTarget.style.backgroundColor = theme.buttonBg}
            onMouseOut={(e) => e.currentTarget.style.backgroundColor = 'transparent'}
            title="Move to next monitor"
          >
            <Monitor size={14} />
          </button>
          <button
            onClick={closeWindow}
            className="p-1 rounded transition-colors hover:opacity-80"
            style={{ color: theme.textColor, backgroundColor: 'transparent' }}
            onMouseOver={(e) => e.currentTarget.style.backgroundColor = theme.buttonBg}
            onMouseOut={(e) => e.currentTarget.style.backgroundColor = 'transparent'}
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-full">
            <span style={{ color: theme.textColor, opacity: 0.5 }}>Loading snippets...</span>
          </div>
        ) : snippets.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full p-6 text-center" style={{ color: theme.textColor, opacity: 0.5 }}>
            <Copy size={32} className="mb-3 opacity-50" />
            <span className="text-sm font-medium mb-1">No snippets yet</span>
            <span className="text-xs" style={{ opacity: 0.7 }}>
              Press <kbd className="px-1 rounded mx-1" style={{ backgroundColor: theme.buttonBg }}>Ctrl+V</kbd> anywhere in this window to add a snippet from your clipboard.
            </span>
          </div>
        ) : (
          <div className="py-1">
            {snippets.map((snippet) => (
              <div
                key={snippet.id}
                onClick={() => handleCopyAndPaste(snippet.id)}
                onContextMenu={(e) => handleCopyToClipboard(e, snippet.id)}
                title={snippet.content_type === "text" ? snippet.content : snippet.title}
                className="w-full px-3 py-2 flex flex-col gap-1 border-b transition-all text-left cursor-pointer group hover:opacity-80"
                style={{
                  borderColor: theme.toolbarBorder,
                  ...(copiedId === snippet.id ? { backgroundColor: theme.accentColor + '40' } : {})
                }}
              >
                <div className="flex items-center justify-between w-full">
                  <div className="flex items-center gap-2 overflow-hidden flex-1">
                    {snippet.content_type === "text" ? (
                      <FileText size={14} style={{ color: theme.accentColor }} className="flex-shrink-0" />
                    ) : (
                      <Image size={14} style={{ color: theme.accentColor }} className="flex-shrink-0" />
                    )}
                    <span className="text-sm font-medium truncate" style={{ color: theme.textColor }}>
                      {snippet.title}
                    </span>
                  </div>

                  {/* Actions */}
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    <button
                      onClick={(e) => handleEditTitle(e, snippet)}
                      className="p-1 rounded transition-colors hover:opacity-80"
                      style={{ color: theme.textColor, backgroundColor: theme.buttonBg }}
                      title="Edit Title"
                    >
                      <PenLine size={12} />
                    </button>
                    <button
                      onClick={(e) => handleDelete(e, snippet.id)}
                      className="p-1 rounded transition-colors text-red-400 hover:opacity-80"
                      style={{ backgroundColor: theme.buttonBg }}
                      title="Delete Snippet"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                </div>

                <div className="flex items-center justify-between pl-6 gap-2">
                  <div className="flex-1 min-w-0 overflow-hidden">
                    {snippet.content_type === "text" ? (
                      <span className="text-xs truncate block max-w-full" style={{ color: theme.textColor, opacity: 0.7 }}>
                        {snippet.content}
                      </span>
                    ) : (
                      <div className="h-10 w-auto max-w-[150px] overflow-hidden rounded border bg-black/40" style={{ borderColor: theme.toolbarBorder }}>
                        <img
                          src={`data:image/jpeg;base64,${snippet.content}`}
                          alt={snippet.title}
                          className="h-full w-full object-contain"
                        />
                      </div>
                    )}
                  </div>
                  
                  <div className="flex flex-col items-end">
                    <span 
                      className={`text-xs font-semibold tracking-wide flex items-center gap-1 transition-all`}
                      style={{
                         color: copiedId === snippet.id ? theme.accentColor : theme.textColor,
                         opacity: copiedId === snippet.id ? 1 : 0,
                         transform: copiedId === snippet.id ? 'scale(1.1)' : 'scale(1)',
                      }}
                    >
                      <Check size={12} /> Pasted!
                    </span>
                    <span
                      className="text-xs font-semibold tracking-wide transition-all absolute group-hover:opacity-100 opacity-0"
                      style={{ color: theme.textColor, opacity: copiedId === snippet.id ? 0 : undefined }}
                    >
                      Paste
                    </span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Footer hint */}
      <div 
        className="px-3 py-1.5 border-t flex justify-between items-center"
        style={{
          backgroundColor: theme.toolbar,
          borderColor: theme.toolbarBorder,
        }}
      >
        <span className="text-xs" style={{ color: theme.textColor, opacity: 0.6 }}>
          Click = paste | Right-click = copy into clipboard
        </span>
        <span className="text-xs flex items-center gap-1" style={{ color: theme.accentColor }}>
          <kbd className="px-1 rounded" style={{ backgroundColor: theme.buttonBg, color: theme.textColor }}>Ctrl+V</kbd> to add
        </span>
      </div>
    </div>
  );
};

export default QuickPasteModal;
