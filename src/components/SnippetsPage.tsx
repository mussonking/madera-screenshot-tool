import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  BookOpen, FileText, Image, PenLine, Trash2, Check, Plus,
  ChevronDown, ChevronRight, FolderOpen, Tag, ArrowUp, ArrowDown, Copy
} from "lucide-react";
import { THEMES, ThemeName, loadThemeFromStore } from "../utils/theme";
import { InlinePrompt, SnippetEditor, CategoryPicker } from "./SnippetDialogs";

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

const sortCategories = (entries: [string, SnippetItem[]][]) =>
  entries.sort(([a], [b]) => (a === "General" ? -1 : b === "General" ? 1 : a.localeCompare(b)));

export default function SnippetsPage() {
  const [snippets, setSnippets] = useState<SnippetItem[]>([]);
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(new Set());
  const [promptState, setPromptState] = useState<{ title: string; defaultValue: string; multiline?: boolean; onSubmit: (v: string) => void } | null>(null);
  const [editState, setEditState] = useState<{ id: string; title: string; content: string; isText: boolean } | null>(null);
  const [catPickerState, setCatPickerState] = useState<{ snippetId: string; snippetTitle: string; current: string } | null>(null);
  const promptResolveRef = useRef<((v: string | null) => void) | null>(null);

  const theme = THEMES[currentTheme];

  const loadSnippets = useCallback(async () => {
    try {
      const items = await invoke<SnippetItem[]>("get_snippets");
      setSnippets(items);
    } catch (error) {
      console.error("Failed to load snippets:", error);
    }
  }, []);

  useEffect(() => {
    loadSnippets();
    loadThemeFromStore().then(setCurrentTheme);
  }, [loadSnippets]);

  // Paste anywhere on the page = save as snippet (unless typing in a dialog input)
  useEffect(() => {
    const onPaste = async (e: ClipboardEvent) => {
      const active = document.activeElement;
      if (active && (active.tagName === "INPUT" || active.tagName === "TEXTAREA")) return;
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
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, [loadSnippets]);

  const showPrompt = (title: string, defaultValue: string, multiline = false): Promise<string | null> => {
    if (promptResolveRef.current) {
      promptResolveRef.current(null);
      promptResolveRef.current = null;
    }
    return new Promise((resolve) => {
      promptResolveRef.current = resolve;
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
    if (promptResolveRef.current) {
      promptResolveRef.current(null);
      promptResolveRef.current = null;
    }
  };

  const handleCopySnippet = async (id: string) => {
    try {
      await invoke("copy_snippet_to_clipboard", { itemId: id });
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 1200);
    } catch (error) {
      console.error(error);
    }
  };

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    await invoke("delete_snippet", { id });
    loadSnippets();
  };

  const handleEditSnippet = (e: React.MouseEvent, snippet: SnippetItem) => {
    e.stopPropagation();
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
    loadSnippets();
  };

  const handleChangeCategory = (e: React.MouseEvent, snippet: SnippetItem) => {
    e.stopPropagation();
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
    if (category !== current) {
      await invoke("update_snippet_category", { id: snippetId, category });
      loadSnippets();
    }
  };

  const handleAddCategory = async () => {
    const result = await showPrompt("New category name:", "");
    if (result) {
      await invoke("add_snippet_with_category", { title: "New Snippet", contentType: "text", content: "", category: result });
      loadSnippets();
    }
  };

  const handleRenameCategory = async (e: React.MouseEvent, name: string) => {
    e.stopPropagation();
    if (name === "General") return;
    const result = await showPrompt("Rename category:", name);
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

  const toggleCategory = (name: string) => {
    setCollapsedCategories(prev => { const n = new Set(prev); n.has(name) ? n.delete(name) : n.add(name); return n; });
  };

  const groupedSnippets: CategoryGroup[] = (() => {
    const map = new Map<string, SnippetItem[]>();
    for (const s of snippets) { const c = s.category || "General"; if (!map.has(c)) map.set(c, []); map.get(c)!.push(s); }
    return sortCategories(Array.from(map.entries())).map(([name, snips]) => ({ name, snippets: snips }));
  })();

  const allCategories = [...new Set(snippets.map(s => s.category || "General"))]
    .sort((a, b) => (a === "General" ? -1 : b === "General" ? 1 : a.localeCompare(b)));

  return (
    <div className="flex-1 min-h-0 w-full flex flex-col relative" style={{ backgroundColor: theme.canvasBg, color: theme.textColor, fontFamily: theme.fontFamily }}>
      {promptState && <InlinePrompt title={promptState.title} defaultValue={promptState.defaultValue} multiline={promptState.multiline} onSubmit={promptState.onSubmit} onCancel={cancelPrompt} />}
      {editState && <SnippetEditor snippet={editState} onSave={handleEditSave} onCancel={() => setEditState(null)} />}
      {catPickerState && (
        <CategoryPicker
          title={`Move "${catPickerState.snippetTitle}" to:`}
          categories={allCategories}
          current={catPickerState.current}
          onSelect={handleCatPickerSelect}
          onCancel={() => setCatPickerState(null)}
        />
      )}

      <div className="flex-1 min-h-0 overflow-y-auto w-full">
        <div className="max-w-3xl mx-auto p-6">
          {/* Header */}
          <div className="flex items-center justify-between mb-4">
            <div className="flex items-center gap-2">
              <BookOpen size={20} style={{ color: theme.accentColor }} />
              <h2 className="text-xl font-semibold">Prompt Snippets</h2>
              <span style={{ opacity: 0.6 }} className="text-sm">({snippets.length})</span>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={handleAddCategory}
                style={{ backgroundColor: theme.buttonBg, borderRadius: theme.borderRadius }}
                className="flex items-center gap-1.5 px-3 py-2 hover:opacity-80 transition-colors text-sm"
                title="New category"
              >
                <FolderOpen size={14} /> New category
              </button>
              <button
                onClick={async () => { try { await invoke("add_snippet_from_clipboard"); loadSnippets(); } catch { } }}
                style={{ backgroundColor: theme.buttonBg, borderRadius: theme.borderRadius }}
                className="flex items-center gap-1.5 px-3 py-2 hover:opacity-80 transition-colors text-sm"
                title="Add clipboard content as snippet"
              >
                <Plus size={14} /> Add from clipboard
              </button>
            </div>
          </div>

          {/* Hint */}
          <div
            className="mb-4 px-4 py-2.5 rounded text-sm flex items-center gap-2"
            style={{ backgroundColor: theme.toolbar, border: `1px ${theme.borderStyle} ${theme.toolbarBorder}`, opacity: 0.85 }}
          >
            <Copy size={14} style={{ color: theme.accentColor }} />
            <span>Click a snippet to copy it. To paste directly into your apps, press <b>Ctrl+Alt+V</b> anywhere.</span>
          </div>

          {/* Content */}
          {snippets.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-24 text-center" style={{ opacity: 0.55 }}>
              <BookOpen size={48} className="mb-4 opacity-50" />
              <p className="text-lg">No snippets yet</p>
              <p className="text-sm mt-2">Copy something, then press Ctrl+V on this page to save it as a snippet.<br />Or use "Add from clipboard" above.</p>
            </div>
          ) : (
            <div className="rounded-lg overflow-hidden" style={{ border: `2px ${theme.borderStyle} ${theme.toolbarBorder}`, backgroundColor: theme.toolbar }}>
              {groupedSnippets.map(group => {
                const cats = group.snippets;
                return (
                  <div key={group.name}>
                    <div
                      className="flex items-center justify-between px-4 py-2 cursor-pointer select-none"
                      style={{ backgroundColor: theme.buttonBg }}
                      onClick={() => toggleCategory(group.name)}
                    >
                      <div className="flex items-center gap-2">
                        {collapsedCategories.has(group.name) ? <ChevronRight size={14} style={{ opacity: 0.6 }} /> : <ChevronDown size={14} style={{ opacity: 0.6 }} />}
                        <FolderOpen size={14} style={{ color: theme.accentColor }} />
                        <span className="text-sm font-semibold">{group.name}</span>
                        <span className="text-xs" style={{ opacity: 0.5 }}>({cats.length})</span>
                      </div>
                      {group.name !== "General" && (
                        <button
                          onClick={e => handleRenameCategory(e, group.name)}
                          className="p-1 rounded hover:opacity-80"
                          style={{ opacity: 0.5 }}
                          title="Rename category"
                        >
                          <PenLine size={12} />
                        </button>
                      )}
                    </div>
                    {!collapsedCategories.has(group.name) && cats.map((s, idx) => (
                      <div
                        key={s.id}
                        onClick={() => handleCopySnippet(s.id)}
                        title="Click to copy"
                        className="w-full px-4 py-3 flex flex-col gap-1.5 border-t transition-all text-left cursor-pointer group hover:opacity-80"
                        style={{ borderColor: theme.toolbarBorder, ...(copiedId === s.id ? { backgroundColor: theme.accentColor + "30" } : {}) }}
                      >
                        <div className="flex items-center justify-between w-full">
                          <div className="flex items-center gap-2 overflow-hidden flex-1">
                            <div className="flex flex-col flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                              <button onClick={e => handleMoveSnippet(e, s, "up")} className="p-0 leading-none hover:opacity-80 disabled:opacity-20" disabled={idx === 0} title="Move up"><ArrowUp size={10} /></button>
                              <button onClick={e => handleMoveSnippet(e, s, "down")} className="p-0 leading-none hover:opacity-80 disabled:opacity-20" disabled={idx === cats.length - 1} title="Move down"><ArrowDown size={10} /></button>
                            </div>
                            {s.content_type === "text"
                              ? <FileText size={15} style={{ color: theme.accentColor }} className="flex-shrink-0" />
                              : <Image size={15} style={{ color: theme.accentColor }} className="flex-shrink-0" />}
                            <span className="text-sm font-medium truncate">{s.title}</span>
                            {copiedId === s.id && (
                              <span className="text-xs font-semibold flex items-center gap-1 flex-shrink-0" style={{ color: theme.accentColor }}>
                                <Check size={12} /> Copied!
                              </span>
                            )}
                          </div>
                          <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                            <button onClick={e => handleChangeCategory(e, s)} className="p-1.5 rounded hover:opacity-80" style={{ backgroundColor: theme.buttonBg }} title="Change category"><Tag size={13} /></button>
                            <button onClick={e => handleEditSnippet(e, s)} className="p-1.5 rounded hover:opacity-80" style={{ backgroundColor: theme.buttonBg }} title="Edit snippet"><PenLine size={13} /></button>
                            <button onClick={e => handleDelete(e, s.id)} className="p-1.5 rounded text-red-400 hover:opacity-80" style={{ backgroundColor: theme.buttonBg }} title="Delete"><Trash2 size={13} /></button>
                          </div>
                        </div>
                        <div className="pl-7 overflow-hidden">
                          {s.content_type === "text"
                            ? <span className="text-xs truncate block" style={{ opacity: 0.65 }}>{s.content}</span>
                            : <div className="h-14 w-auto max-w-[220px] overflow-hidden rounded border bg-black/40" style={{ borderColor: theme.toolbarBorder }}><img src={"data:image/jpeg;base64," + s.content} alt={s.title} className="h-full w-full object-contain" /></div>}
                        </div>
                      </div>
                    ))}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
