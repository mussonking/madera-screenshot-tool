import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { format, parseISO } from "date-fns";
import {
  GripHorizontal, X, FileText, Image, PenLine, Trash2, Copy, Check,
  ArrowLeftRight, Monitor, Plus, ChevronDown, ChevronRight, FolderOpen, Tag,
  Clipboard, Camera, Pipette, BookOpen, Clock, ArrowUp, ArrowDown
} from "lucide-react";
import { THEMES, ThemeName, loadThemeFromStore } from "../utils/theme";
import type { HistoryItem } from "../stores/appStore";

export interface SnippetItem {
  id: string;
  title: string;
  content_type: string;
  content: string;
  category: string;
  sort_order: number;
}

interface CategoryGroup {
  name: string;
  snippets: SnippetItem[];
}

type TabType = "snippets" | "history";

const InlinePrompt = ({ title, defaultValue, multiline, onSubmit, onCancel }: {
  title: string;
  defaultValue: string;
  multiline?: boolean;
  onSubmit: (value: string) => void;
  onCancel: () => void;
}) => {
  const [value, setValue] = useState(defaultValue);
  const inputRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (multiline) {
      textareaRef.current?.focus();
    } else {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [multiline]);

  const handleSubmit = () => {
    if (value.trim()) onSubmit(value.trim());
    else onCancel();
  };

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onCancel}>
      <div className="bg-[#2a2a3e] border border-[#444] rounded-lg p-4 w-[340px] shadow-xl" onClick={e => e.stopPropagation()}>
        <p className="text-sm text-gray-200 mb-3">{title}</p>
        {multiline ? (
          <textarea
            ref={textareaRef}
            value={value}
            onChange={e => setValue(e.target.value)}
            onKeyDown={e => { if (e.key === "Escape") onCancel(); if (e.key === "Enter" && e.ctrlKey) handleSubmit(); }}
            rows={6}
            className="w-full px-3 py-1.5 rounded bg-[#1a1a2e] border border-[#555] text-white text-sm outline-none focus:border-blue-400 resize-y"
          />
        ) : (
          <input
            ref={inputRef}
            type="text"
            value={value}
            onChange={e => setValue(e.target.value)}
            onKeyDown={e => { if (e.key === "Enter") handleSubmit(); if (e.key === "Escape") onCancel(); }}
            className="w-full px-3 py-1.5 rounded bg-[#1a1a2e] border border-[#555] text-white text-sm outline-none focus:border-blue-400"
          />
        )}
        <div className="flex justify-between items-center mt-3">
          {multiline ? <span className="text-xs text-gray-500">Ctrl+Enter to save</span> : <span />}
          <div className="flex gap-2">
            <button onClick={onCancel} className="px-3 py-1 text-xs text-gray-400 hover:text-white">Cancel</button>
            <button onClick={handleSubmit} className="px-3 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-500">OK</button>
          </div>
        </div>
      </div>
    </div>
  );
};


const SnippetEditor = ({ snippet, onSave, onCancel }: {
  snippet: { title: string; content: string; isText: boolean };
  onSave: (title: string, content: string) => void;
  onCancel: () => void;
}) => {
  const [title, setTitle] = useState(snippet.title);
  const [content, setContent] = useState(snippet.content);
  const titleRef = useRef<HTMLInputElement>(null);
  useEffect(() => { titleRef.current?.focus(); titleRef.current?.select(); }, []);
  const handleSave = () => { if (title.trim()) onSave(title.trim(), content); };
  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onCancel}>
      <div className="bg-[#2a2a3e] border border-[#444] rounded-lg p-4 w-[360px] shadow-xl" onClick={e => e.stopPropagation()}>
        <p className="text-sm text-gray-200 mb-3 font-medium">Edit Snippet</p>
        <label className="text-xs text-gray-400 mb-1 block">Title</label>
        <input ref={titleRef} type="text" value={title} onChange={e => setTitle(e.target.value)}
          onKeyDown={e => { if (e.key === "Escape") onCancel(); }}
          className="w-full px-3 py-1.5 rounded bg-[#1a1a2e] border border-[#555] text-white text-sm outline-none focus:border-blue-400 mb-3" />
        {snippet.isText && (
          <>
            <label className="text-xs text-gray-400 mb-1 block">Content</label>
            <textarea value={content} onChange={e => setContent(e.target.value)}
              onKeyDown={e => { if (e.key === "Escape") onCancel(); if (e.key === "Enter" && e.ctrlKey) handleSave(); }}
              rows={8}
              className="w-full px-3 py-1.5 rounded bg-[#1a1a2e] border border-[#555] text-white text-sm outline-none focus:border-blue-400 resize-y font-mono" />
          </>
        )}
        <div className="flex justify-between items-center mt-3">
          <span className="text-xs text-gray-500">Ctrl+Enter to save</span>
          <div className="flex gap-2">
            <button onClick={onCancel} className="px-3 py-1 text-xs text-gray-400 hover:text-white">Cancel</button>
            <button onClick={handleSave} className="px-3 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-500">Save</button>
          </div>
        </div>
      </div>
    </div>
  );
};

const CategoryPicker = ({ title, categories, current, onSelect, onCancel }: {
  title: string;
  categories: string[];
  current: string;
  onSelect: (category: string) => void;
  onCancel: () => void;
}) => {
  const [newName, setNewName] = useState("");
  const [showNew, setShowNew] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (showNew) inputRef.current?.focus();
  }, [showNew]);

  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onCancel}>
      <div className="bg-[#2a2a3e] border border-[#444] rounded-lg p-4 w-[300px] shadow-xl" onClick={e => e.stopPropagation()}>
        <p className="text-sm text-gray-200 mb-3">{title}</p>
        <div className="flex flex-col gap-1 max-h-[200px] overflow-y-auto mb-2">
          {categories.map(cat => (
            <button
              key={cat}
              onClick={() => onSelect(cat)}
              className="w-full text-left px-3 py-1.5 rounded text-sm transition-colors flex items-center justify-between"
              style={{
                backgroundColor: cat === current ? "#3b82f640" : "#1a1a2e",
                color: cat === current ? "#93c5fd" : "#d1d5db",
                border: cat === current ? "1px solid #3b82f6" : "1px solid #333",
              }}
            >
              <span className="flex items-center gap-2">
                <FolderOpen size={12} />
                {cat}
              </span>
              {cat === current && <span className="text-xs opacity-60">(current)</span>}
            </button>
          ))}
        </div>
        {showNew ? (
          <div className="flex gap-2">
            <input
              ref={inputRef}
              type="text"
              value={newName}
              onChange={e => setNewName(e.target.value)}
              onKeyDown={e => {
                if (e.key === "Enter" && newName.trim()) onSelect(newName.trim());
                if (e.key === "Escape") { setShowNew(false); setNewName(""); }
              }}
              placeholder="Category name..."
              className="flex-1 px-3 py-1.5 rounded bg-[#1a1a2e] border border-[#555] text-white text-sm outline-none focus:border-blue-400"
            />
            <button
              onClick={() => { if (newName.trim()) onSelect(newName.trim()); }}
              className="px-3 py-1.5 text-xs bg-blue-600 text-white rounded hover:bg-blue-500"
            >OK</button>
          </div>
        ) : (
          <button
            onClick={() => setShowNew(true)}
            className="w-full text-left px-3 py-1.5 rounded text-sm text-blue-400 hover:bg-[#1a1a2e] transition-colors flex items-center gap-2 border border-dashed border-[#555]"
          >
            <Plus size={12} /> New category...
          </button>
        )}
        <div className="flex justify-end mt-3">
          <button onClick={onCancel} className="px-3 py-1 text-xs text-gray-400 hover:text-white">Cancel</button>
        </div>
      </div>
    </div>
  );
};

const QuickPasteModal = () => {
  // Read initial tab from URL hash query (e.g. #quickpaste?tab=history)
  const initialTab = (() => {
    const hash = window.location.hash;
    const match = hash.match(/[?&]tab=(\w+)/);
    if (match && (match[1] === "snippets" || match[1] === "history")) return match[1] as TabType;
    return "snippets" as TabType;
  })();
  const [activeTab, setActiveTab] = useState<TabType>(initialTab);
  const [snippets, setSnippets] = useState<SnippetItem[]>([]);
  const [historyItems, setHistoryItems] = useState<HistoryItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(new Set());
  const [historyOffset, setHistoryOffset] = useState(0);
  const [hasMoreHistory, setHasMoreHistory] = useState(true);
  const [promptState, setPromptState] = useState<{ title: string; defaultValue: string; multiline?: boolean; onSubmit: (v: string) => void } | null>(null);
  const [editState, setEditState] = useState<{ id: string; title: string; content: string; isText: boolean } | null>(null);
  const [catPickerState, setCatPickerState] = useState<{ snippetId: string; snippetTitle: string; current: string } | null>(null);
  const isPasting = useRef(false);
  const kbEnabled = useRef(false);
  const HISTORY_LIMIT = 20;

  const loadSnippets = useCallback(async () => {
    try {
      const items = await invoke<SnippetItem[]>("get_snippets");
      setSnippets(items);
    } catch (error) {
      console.error("Failed to load snippets:", error);
    }
  }, []);

  const loadHistory = useCallback(async (reset = false) => {
    try {
      const currentOffset = reset ? 0 : historyOffset;
      const items = await invoke<HistoryItem[]>("get_unified_history", { filterType: null, limit: HISTORY_LIMIT, offset: currentOffset });
      if (reset) {
        setHistoryItems(items);
        setHistoryOffset(HISTORY_LIMIT);
      } else {
        setHistoryItems(prev => {
          const ids = new Set(prev.map(i => i.id));
          return [...prev, ...items.filter(i => !ids.has(i.id))];
        });
        setHistoryOffset(currentOffset + HISTORY_LIMIT);
      }
      setHasMoreHistory(items.length === HISTORY_LIMIT);
    } catch (error) {
      console.error("Failed to load history:", error);
    }
  }, [historyOffset]);

  useEffect(() => {
    (async () => {
      setLoading(true);
      await Promise.all([loadSnippets(), loadHistory(true)]);
      setLoading(false);
    })();
    loadThemeFromStore().then(setCurrentTheme);

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // Modals handle their own Escape via onCancel/onKeyDown
        // Only close the window if no modal is open
        if (!kbEnabled.current) {
          closeWindow();
        }
      }
    };
    const onPaste = async (e: ClipboardEvent) => {
      if (isPasting.current) return;
      const items = e.clipboardData?.items;
      if (!items) return;
      for (let i = 0; i < items.length; i++) {
        const item = items[i];
        if (item.type.startsWith("image/")) {
          const file = item.getAsFile();
          if (!file) break;
          const reader = new FileReader();
          reader.onload = async (ev) => {
            const b64 = (ev.target?.result as string)?.split(",")[1];
            if (b64) { await invoke("add_snippet", { title: "New Image Snippet", contentType: "image", content: b64 }); loadSnippets(); }
          };
          reader.readAsDataURL(file);
          break;
        } else if (item.type === "text/plain") {
          item.getAsString(async (text) => {
            if (text.trim()) { await invoke("add_snippet", { title: "New Text Snippet", contentType: "text", content: text }); loadSnippets(); }
          });
          break;
        }
      }
    };
    window.addEventListener("keydown", onKey);
    document.addEventListener("paste", onPaste);
    return () => { window.removeEventListener("keydown", onKey); document.removeEventListener("paste", onPaste); };
  }, []);

  const closeWindow = async () => { try { await getCurrentWindow().close(); } catch {} };
  const enableKb = () => { kbEnabled.current = true; return invoke("set_panel_keyboard", { windowLabel: "quickpaste", enabled: true }).catch(() => {}); };
  const disableKb = () => { kbEnabled.current = false; return invoke("set_panel_keyboard", { windowLabel: "quickpaste", enabled: false }).catch(() => {}); };


  const promptResolveRef = useRef<((v: string | null) => void) | null>(null);

  const showPrompt = (title: string, defaultValue: string, multiline = false): Promise<string | null> => {
    if (promptResolveRef.current) {
      promptResolveRef.current(null);
      promptResolveRef.current = null;
    }
    return new Promise((resolve) => {
      promptResolveRef.current = resolve;
      enableKb();
      setPromptState({
        title,
        defaultValue: defaultValue || "",
        multiline,
        onSubmit: (v) => {
          setPromptState(null);
          promptResolveRef.current = null;
          resolve(v);
        },
      });
    });
  };

  const cancelPrompt = () => {
    setPromptState(null);
    disableKb();
    if (promptResolveRef.current) {
      promptResolveRef.current(null);
      promptResolveRef.current = null;
    }
  };

  const handleSnippetPaste = async (id: string) => {
    try {
      isPasting.current = true;
      await invoke("paste_snippet_item", { itemId: id });
      setCopiedId(id);
      setTimeout(() => { setCopiedId(null); isPasting.current = false; }, 1000);
    } catch (error) { console.error("Failed to paste snippet:", error); isPasting.current = false; }
  };

  const handleHistoryPaste = async (itemId: string) => {
    try {
      isPasting.current = true;
      await invoke("paste_history_item", { itemId });
      setCopiedId(itemId);
      setTimeout(() => { setCopiedId(null); isPasting.current = false; }, 1000);
    } catch (error) { console.error("Failed to paste history item:", error); isPasting.current = false; }
  };

  const handleCopySnippet = async (e: React.MouseEvent, id: string) => {
    e.preventDefault();
    try { await invoke("copy_snippet_to_clipboard", { itemId: id }); setCopiedId(id); setTimeout(() => setCopiedId(null), 1000); }
    catch (error) { console.error(error); }
  };

  const handleCopyHistory = async (e: React.MouseEvent, itemId: string) => {
    e.preventDefault();
    try { await invoke("copy_history_item_to_clipboard", { itemId }); setCopiedId(itemId); setTimeout(() => setCopiedId(null), 1000); }
    catch (error) { console.error(error); }
  };

  const handleSaveAsSnippet = async (e: React.MouseEvent, item: HistoryItem) => {
    e.stopPropagation();
    try {
      if (item.item_type === "clipboard_text" && item.text_content) {
        const p = item.text_content.slice(0, 40);
        await invoke("add_snippet", { title: item.text_content.length > 40 ? p + "..." : p, contentType: "text", content: item.text_content });
      } else if ((item.item_type === "screenshot" || item.item_type === "clipboard_image") && item.thumbnail) {
        await invoke("add_snippet", { title: item.filename || "Image from history", contentType: "image", content: item.thumbnail });
      } else if (item.item_type === "color_pick" && item.color_hex) {
        await invoke("add_snippet", { title: "Color " + item.color_hex, contentType: "text", content: item.color_hex });
      }
      loadSnippets(); setCopiedId(item.id); setTimeout(() => setCopiedId(null), 1000);
    } catch (error) { console.error(error); }
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    await invoke("delete_snippet", { id }); loadSnippets();
  };

  const handleEditSnippet = (e: React.MouseEvent, snippet: SnippetItem) => {
    e.stopPropagation();
    enableKb();
    setEditState({
      id: snippet.id,
      title: snippet.title,
      content: snippet.content,
      isText: snippet.content_type === "text",
    });
  };

  const handleEditSave = async (title: string, content: string) => {
    if (!editState) return;
    await invoke("update_snippet", { id: editState.id, title, content });
    setEditState(null);
    disableKb();
    loadSnippets();
  };

  const handleEditCancel = () => {
    setEditState(null);
    disableKb();
  };

  const handleChangeCategory = (e: React.MouseEvent, snippet: SnippetItem) => {
    e.stopPropagation();
    enableKb();
    setCatPickerState({
      snippetId: snippet.id,
      snippetTitle: snippet.title,
      current: snippet.category || "General",
    });
  };

  const handleCatPickerSelect = async (category: string) => {
    if (!catPickerState) return;
    const { snippetId, current } = catPickerState;
    setCatPickerState(null);
    disableKb();
    if (category !== current) {
      await invoke("update_snippet_category", { id: snippetId, category });
      loadSnippets();
    }
  };

  const handleCatPickerCancel = () => {
    setCatPickerState(null);
    disableKb();
  };

  const handleAddCategory = async () => {
    const result = await showPrompt("New category name:", "");
    disableKb();
    if (result) {
      await invoke("add_snippet_with_category", { title: "New Snippet", contentType: "text", content: "", category: result });
      loadSnippets();
    }
  };

  const handleRenameCategory = async (e: React.MouseEvent, name: string) => {
    e.stopPropagation();
    if (name === "General") return;
    const result = await showPrompt("Rename category:", name);
    disableKb();
    if (result && result !== name) {
      await invoke("rename_snippet_category", { oldName: name, newName: result });
      loadSnippets();
    }
  };

  const handleMoveSnippet = async (e: React.MouseEvent, snippet: SnippetItem, dir: "up" | "down") => {
    e.stopPropagation();
    const cat = snippets.filter(s => s.category === snippet.category);
    const idx = cat.findIndex(s => s.id === snippet.id);
    const swapIdx = dir === "up" ? idx - 1 : idx + 1;
    if (swapIdx < 0 || swapIdx >= cat.length) return;
    const ids = cat.map(s => s.id);
    [ids[idx], ids[swapIdx]] = [ids[swapIdx], ids[idx]];
    await invoke("reorder_snippets", { orderedIds: ids });
    await loadSnippets();
  };

  const handleWindowDrag = async (e: React.MouseEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    await getCurrentWindow().startDragging();
  };

  const toggleCategory = (name: string) => {
    setCollapsedCategories(prev => { const n = new Set(prev); n.has(name) ? n.delete(name) : n.add(name); return n; });
  };

  const theme = THEMES[currentTheme];

  const groupedSnippets: CategoryGroup[] = (() => {
    const map = new Map<string, SnippetItem[]>();
    for (const s of snippets) { const c = s.category || "General"; if (!map.has(c)) map.set(c, []); map.get(c)!.push(s); }
    return Array.from(map.entries())
      .sort(([a], [b]) => a === "General" ? -1 : b === "General" ? 1 : a.localeCompare(b))
      .map(([name, snips]) => ({ name, snippets: snips }));
  })();

  const multiCat = groupedSnippets.length > 1;

  const histIcon = (t: string) => {
    if (t === "screenshot") return <Camera size={14} style={{ color: "#60a5fa" }} />;
    if (t === "clipboard_image") return <Image size={14} style={{ color: "#4ade80" }} />;
    if (t === "clipboard_text") return <FileText size={14} style={{ color: "#facc15" }} />;
    if (t === "color_pick") return <Pipette size={14} style={{ color: "#c084fc" }} />;
    return <Clipboard size={14} style={{ color: theme.textColor, opacity: 0.5 }} />;
  };

  const histPreview = (item: HistoryItem) => {
    if (item.item_type === "clipboard_text") return <span className="text-xs truncate" style={{ color: theme.textColor, opacity: 0.7 }}>{item.text_preview || item.text_content?.slice(0, 60) || "Empty"}</span>;
    if (item.item_type === "color_pick") return <div className="flex items-center gap-2"><div className="w-4 h-4 rounded border" style={{ backgroundColor: item.color_hex || "#000", borderColor: theme.toolbarBorder }} /><span className="text-xs font-mono" style={{ color: theme.textColor, opacity: 0.7 }}>{item.color_hex}</span></div>;
    if (item.item_type === "screenshot" || item.item_type === "clipboard_image") return <div className="flex items-center gap-2">{item.thumbnail && <img src={"data:image/jpeg;base64," + item.thumbnail} alt="" className="w-8 h-8 object-cover rounded" />}<span className="text-xs" style={{ color: theme.textColor, opacity: 0.5 }}>{item.width}x{item.height}</span></div>;
    return <span className="text-xs" style={{ color: theme.textColor, opacity: 0.5 }}>Unknown</span>;
  };

  const fmtTime = (d: string) => { try { return format(parseISO(d), "HH:mm"); } catch { return ""; } };

  return (
    <div className="h-full w-full rounded-lg shadow-2xl overflow-hidden flex flex-col font-sans border relative" style={{ backgroundColor: theme.canvasBg, color: theme.textColor, borderColor: theme.toolbarBorder, fontFamily: theme.fontFamily }}>
      {promptState && <InlinePrompt title={promptState.title} defaultValue={promptState.defaultValue} multiline={promptState.multiline} onSubmit={promptState.onSubmit} onCancel={cancelPrompt} />}
      {editState && <SnippetEditor snippet={editState} onSave={handleEditSave} onCancel={handleEditCancel} />}
      {catPickerState && <CategoryPicker title={`Move "${catPickerState.snippetTitle}" to:`} categories={[...new Set(snippets.map(s => s.category || "General"))].sort((a, b) => a === "General" ? -1 : b === "General" ? 1 : a.localeCompare(b))} current={catPickerState.current} onSelect={handleCatPickerSelect} onCancel={handleCatPickerCancel} />}

      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2 cursor-move select-none border-b" style={{ backgroundColor: theme.toolbar, borderColor: theme.toolbarBorder }} onMouseDown={handleWindowDrag}>
        <div className="flex items-center gap-2">
          <GripHorizontal size={16} style={{ color: theme.textColor, opacity: 0.5 }} />
          <Copy size={16} style={{ color: theme.accentColor }} />
          <span className="text-sm font-medium" style={{ color: theme.textColor }}>Quick Paste</span>
        </div>
        <div className="flex items-center gap-1">
          <button onClick={() => invoke("toggle_panel_side", { windowLabel: "quickpaste" })} className="p-1 rounded transition-colors hover:opacity-80" style={{ color: theme.textColor, backgroundColor: "transparent" }} onMouseOver={e => e.currentTarget.style.backgroundColor = theme.buttonBg} onMouseOut={e => e.currentTarget.style.backgroundColor = "transparent"} title="Move left/right"><ArrowLeftRight size={14} /></button>
          <button onClick={() => invoke("toggle_panel_monitor", { windowLabel: "quickpaste" })} className="p-1 rounded transition-colors hover:opacity-80" style={{ color: theme.textColor, backgroundColor: "transparent" }} onMouseOver={e => e.currentTarget.style.backgroundColor = theme.buttonBg} onMouseOut={e => e.currentTarget.style.backgroundColor = "transparent"} title="Move to next monitor"><Monitor size={14} /></button>
          <button onClick={closeWindow} className="p-1 rounded transition-colors hover:opacity-80" style={{ color: theme.textColor, backgroundColor: "transparent" }} onMouseOver={e => e.currentTarget.style.backgroundColor = theme.buttonBg} onMouseOut={e => e.currentTarget.style.backgroundColor = "transparent"}><X size={14} /></button>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex border-b" style={{ borderColor: theme.toolbarBorder, backgroundColor: theme.toolbar }}>
        {(["snippets", "history"] as TabType[]).map(tab => (
          <button key={tab} onClick={() => setActiveTab(tab)} className="flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 text-xs font-medium transition-colors" style={{ color: activeTab === tab ? theme.accentColor : theme.textColor, opacity: activeTab === tab ? 1 : 0.6, borderBottom: activeTab === tab ? "2px solid " + theme.accentColor : "2px solid transparent" }}>
            {tab === "snippets" ? <><BookOpen size={12} /> Snippets ({snippets.length})</> : <><Clock size={12} /> History ({historyItems.length})</>}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-full"><span style={{ color: theme.textColor, opacity: 0.5 }}>Loading...</span></div>
        ) : activeTab === "snippets" ? (
          snippets.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full p-6 text-center" style={{ color: theme.textColor, opacity: 0.5 }}>
              <Copy size={32} className="mb-3 opacity-50" />
              <span className="text-sm font-medium mb-1">No snippets yet</span>
              <span className="text-xs" style={{ opacity: 0.7 }}>Save items from the History tab or use "Add from clipboard" below.</span>
            </div>
          ) : (
            <div className="py-1">
              {groupedSnippets.map(group => {
                const cats = group.snippets;
                return (
                  <div key={group.name}>
                    {(multiCat || group.name !== "General") && (
                      <div className="flex items-center justify-between px-3 py-1.5 cursor-pointer select-none" style={{ backgroundColor: theme.toolbar + "80" }} onClick={() => toggleCategory(group.name)}>
                        <div className="flex items-center gap-1.5">
                          {collapsedCategories.has(group.name) ? <ChevronRight size={12} style={{ color: theme.textColor, opacity: 0.6 }} /> : <ChevronDown size={12} style={{ color: theme.textColor, opacity: 0.6 }} />}
                          <FolderOpen size={12} style={{ color: theme.accentColor }} />
                          <span className="text-xs font-semibold uppercase tracking-wider" style={{ color: theme.textColor, opacity: 0.7 }}>{group.name}</span>
                          <span className="text-xs" style={{ color: theme.textColor, opacity: 0.4 }}>({cats.length})</span>
                        </div>
                        {group.name !== "General" && <button onClick={e => handleRenameCategory(e, group.name)} className="p-0.5 rounded" style={{ color: theme.textColor, opacity: 0.4 }} onMouseOver={e => { e.currentTarget.style.backgroundColor = theme.buttonBg; e.currentTarget.style.opacity = "1"; }} onMouseOut={e => { e.currentTarget.style.backgroundColor = "transparent"; e.currentTarget.style.opacity = "0.4"; }} title="Rename"><PenLine size={10} /></button>}
                      </div>
                    )}
                    {!collapsedCategories.has(group.name) && cats.map((s, idx) => (
                      <div key={s.id} onClick={() => handleSnippetPaste(s.id)} onContextMenu={e => handleCopySnippet(e, s.id)} title={s.content_type === "text" ? s.content : s.title} className="w-full px-3 py-2 flex flex-col gap-1 border-b transition-all text-left cursor-pointer group hover:opacity-80" style={{ borderColor: theme.toolbarBorder, ...(copiedId === s.id ? { backgroundColor: theme.accentColor + "40" } : {}) }}>
                        <div className="flex items-center justify-between w-full">
                          <div className="flex items-center gap-2 overflow-hidden flex-1">
                            <div className="flex flex-col flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                              <button onClick={e => handleMoveSnippet(e, s, "up")} className="p-0 leading-none hover:opacity-80 disabled:opacity-20" style={{ color: theme.textColor }} disabled={idx === 0} title="Move up"><ArrowUp size={10} /></button>
                              <button onClick={e => handleMoveSnippet(e, s, "down")} className="p-0 leading-none hover:opacity-80 disabled:opacity-20" style={{ color: theme.textColor }} disabled={idx === cats.length - 1} title="Move down"><ArrowDown size={10} /></button>
                            </div>
                            {s.content_type === "text" ? <FileText size={14} style={{ color: theme.accentColor }} className="flex-shrink-0" /> : <Image size={14} style={{ color: theme.accentColor }} className="flex-shrink-0" />}
                            <span className="text-sm font-medium truncate" style={{ color: theme.textColor }}>{s.title}</span>
                          </div>
                          <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                            <button onClick={e => handleChangeCategory(e, s)} className="p-1 rounded hover:opacity-80" style={{ color: theme.textColor, backgroundColor: theme.buttonBg }} title="Change Category"><Tag size={12} /></button>
                            <button onClick={e => handleEditSnippet(e, s)} className="p-1 rounded hover:opacity-80" style={{ color: theme.textColor, backgroundColor: theme.buttonBg }} title="Edit Snippet"><PenLine size={12} /></button>
                            <button onClick={e => handleDelete(e, s.id)} className="p-1 rounded text-red-400 hover:opacity-80" style={{ backgroundColor: theme.buttonBg }} title="Delete"><Trash2 size={12} /></button>
                          </div>
                        </div>
                        <div className="flex items-center justify-between pl-7 gap-2">
                          <div className="flex-1 min-w-0 overflow-hidden">
                            {s.content_type === "text" ? <span className="text-xs truncate block" style={{ color: theme.textColor, opacity: 0.7 }}>{s.content}</span> : <div className="h-10 w-auto max-w-[150px] overflow-hidden rounded border bg-black/40" style={{ borderColor: theme.toolbarBorder }}><img src={"data:image/jpeg;base64," + s.content} alt={s.title} className="h-full w-full object-contain" /></div>}
                          </div>
                          <span className="text-xs font-semibold flex items-center gap-1 transition-all" style={{ color: copiedId === s.id ? theme.accentColor : theme.textColor, opacity: copiedId === s.id ? 1 : 0 }}><Check size={12} /> Pasted!</span>
                        </div>
                      </div>
                    ))}
                  </div>
                );
              })}
            </div>
          )
        ) : (
          historyItems.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full p-6 text-center" style={{ color: theme.textColor, opacity: 0.5 }}>
              <Clipboard size={32} className="mb-3 opacity-50" />
              <span className="text-sm font-medium mb-1">No clipboard history</span>
            </div>
          ) : (
            <div className="py-1">
              {historyItems.map((item, i) => (
                <div key={item.id} onClick={() => handleHistoryPaste(item.id)} onContextMenu={e => handleCopyHistory(e, item.id)} className="w-full px-3 py-2 flex items-center gap-2 border-b transition-all text-left cursor-pointer group hover:opacity-80" style={{ borderColor: theme.toolbarBorder, ...(copiedId === item.id ? { backgroundColor: theme.accentColor + "40" } : {}) }}>
                  <span className="text-xs w-4 text-right flex-shrink-0" style={{ color: theme.textColor, opacity: 0.4 }}>{copiedId === item.id ? <Check size={12} style={{ color: theme.accentColor }} /> : i + 1}</span>
                  {histIcon(item.item_type)}
                  <div className="flex-1 min-w-0 overflow-hidden">{histPreview(item)}</div>
                  <div className="flex items-center gap-1">
                    <button onClick={e => handleSaveAsSnippet(e, item)} className="p-1 rounded hover:opacity-80 opacity-0 group-hover:opacity-100" style={{ color: theme.accentColor, backgroundColor: theme.buttonBg }} title="Save as snippet"><BookOpen size={12} /></button>
                    <span className="text-xs flex-shrink-0" style={{ color: theme.accentColor, opacity: copiedId === item.id ? 1 : 0 }}>{copiedId === item.id ? "Pasted!" : ""}</span>
                    <span className="text-xs opacity-0 group-hover:opacity-60 flex-shrink-0" style={{ color: theme.textColor }}>{fmtTime(item.created_at)}</span>
                  </div>
                </div>
              ))}
              {hasMoreHistory && <button onClick={() => loadHistory(false)} className="w-full text-center py-3 text-xs hover:opacity-80" style={{ color: theme.accentColor }}>Load older items...</button>}
            </div>
          )
        )}
      </div>

      {/* Footer */}
      <div className="px-3 py-1.5 border-t flex justify-between items-center" style={{ backgroundColor: theme.toolbar, borderColor: theme.toolbarBorder }}>
        <span className="text-xs" style={{ color: theme.textColor, opacity: 0.6 }}>Click = paste | Right-click = copy</span>
        {activeTab === "snippets" && (
          <div className="flex items-center gap-1">
            <button onClick={handleAddCategory} className="flex items-center gap-1 px-2 py-0.5 rounded hover:opacity-80 text-xs" style={{ backgroundColor: theme.buttonBg, color: theme.accentColor }} title="New category"><FolderOpen size={12} /></button>
            <button onClick={async () => { try { await invoke("add_snippet_from_clipboard"); loadSnippets(); } catch {} }} className="flex items-center gap-1 px-2 py-0.5 rounded hover:opacity-80 text-xs" style={{ backgroundColor: theme.buttonBg, color: theme.accentColor }} title="Add clipboard content as snippet"><Plus size={12} /> Add from clipboard</button>
          </div>
        )}
        {activeTab === "history" && <button onClick={() => loadHistory(true)} className="flex items-center gap-1 px-2 py-0.5 rounded hover:opacity-80 text-xs" style={{ backgroundColor: theme.buttonBg, color: theme.accentColor }}><Clock size={12} /> Refresh</button>}
      </div>
    </div>
  );
};

export default QuickPasteModal;
