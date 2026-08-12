import { useEffect, useRef, useState } from "react";
import { FolderOpen, Plus } from "lucide-react";

export const InlinePrompt = ({ title, defaultValue, multiline, onSubmit, onCancel }: {
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

export const SnippetEditor = ({ snippet, onSave, onCancel }: {
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

export const CategoryPicker = ({ title, categories, current, onSelect, onCancel }: {
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
