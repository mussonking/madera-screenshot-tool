import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { load } from "@tauri-apps/plugin-store";
import { Shield, RotateCcw, X, Check, AlertTriangle } from "lucide-react";

interface SavedLayout {
  id: string;
  name: string;
  created_at: string;
  windows: Array<{
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
  }>;
  is_auto_save: boolean;
}

const STORE_PATH = "settings.json";

export default function WakePopup() {
  const [showPopup, setShowPopup] = useState(false);
  const [lastLayout, setLastLayout] = useState<SavedLayout | null>(null);
  const [restoring, setRestoring] = useState(false);
  const [restoreResults, setRestoreResults] = useState<Array<[string, string | null]>>([]);
  const [showResults, setShowResults] = useState(false);

  useEffect(() => {
    // Listen for wake from sleep event
    const unlistenWake = listen("system-wake-from-sleep", async () => {
      console.log("System woke from sleep!");

      // Load saved layouts from store
      try {
        const store = await load(STORE_PATH);
        const layouts = await store.get<SavedLayout[]>("desktop_layouts");

        if (layouts && layouts.length > 0) {
          // Find most recent auto-save or any layout
          const autoSave = layouts.find(l => l.is_auto_save);
          const layoutToRestore = autoSave || layouts[0];
          setLastLayout(layoutToRestore);
          setShowPopup(true);
        }
      } catch (err) {
        console.error("Failed to load layouts:", err);
      }
    });

    // Also listen for tray menu restore action
    const unlistenRestore = listen("restore-last-layout", async () => {
      try {
        const store = await load(STORE_PATH);
        const layouts = await store.get<SavedLayout[]>("desktop_layouts");

        if (layouts && layouts.length > 0) {
          const autoSave = layouts.find(l => l.is_auto_save);
          const layoutToRestore = autoSave || layouts[0];

          // Directly restore without popup
          const results = await invoke<Array<[string, string | null]>>("restore_window_layout", {
            layout: layoutToRestore,
          });

          setRestoreResults(results);
          setLastLayout(layoutToRestore);
          setShowResults(true);
          setShowPopup(true);
        }
      } catch (err) {
        console.error("Failed to restore layout:", err);
      }
    });

    return () => {
      unlistenWake.then(f => f());
      unlistenRestore.then(f => f());
    };
  }, []);

  const handleRestore = async () => {
    if (!lastLayout) return;

    setRestoring(true);
    try {
      const results = await invoke<Array<[string, string | null]>>("restore_window_layout", {
        layout: lastLayout,
      });
      setRestoreResults(results);
      setShowResults(true);
    } catch (err) {
      console.error("Failed to restore:", err);
    }
    setRestoring(false);
  };

  const handleClose = () => {
    setShowPopup(false);
    setShowResults(false);
    setRestoreResults([]);
  };

  if (!showPopup) return null;

  const successCount = restoreResults.filter(([_, err]) => !err).length;
  const totalCount = restoreResults.length;

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-[9999] p-4">
      <div
        className="max-w-md w-full p-6 rounded-xl shadow-2xl relative"
        style={{
          backgroundColor: "#16213e",
          border: "2px solid #0f3460",
        }}
      >
        <button
          onClick={handleClose}
          className="absolute top-4 right-4 text-gray-400 hover:text-white transition-colors"
        >
          <X size={20} />
        </button>

        {!showResults ? (
          <>
            {/* Wake notification */}
            <div className="flex items-center gap-4 mb-6">
              <div className="p-3 rounded-full bg-cyan-500/20">
                <Shield size={32} className="text-cyan-400" />
              </div>
              <div>
                <h2 className="text-xl font-bold text-white">Windows Restored?</h2>
                <p className="text-gray-400 text-sm">Your system just woke up from sleep</p>
              </div>
            </div>

            <div className="p-4 rounded-lg mb-6" style={{ backgroundColor: "#0f3460" }}>
              <p className="text-gray-300 text-sm mb-2">
                Last saved layout: <span className="text-cyan-400 font-medium">{lastLayout?.name}</span>
              </p>
              <p className="text-gray-400 text-xs">
                {lastLayout?.windows.length} windows captured
              </p>
            </div>

            <p className="text-gray-400 text-sm mb-6 text-center">
              Did Windows mess up your window positions?
            </p>

            <div className="flex gap-3">
              <button
                onClick={handleClose}
                className="flex-1 px-4 py-3 rounded-lg bg-gray-700 hover:bg-gray-600 text-white font-medium transition-colors"
              >
                No, I'm Good
              </button>
              <button
                onClick={handleRestore}
                disabled={restoring}
                className="flex-1 px-4 py-3 rounded-lg bg-cyan-500 hover:bg-cyan-600 text-white font-medium transition-colors flex items-center justify-center gap-2 disabled:opacity-50"
              >
                {restoring ? (
                  <>Restoring...</>
                ) : (
                  <>
                    <RotateCcw size={18} />
                    Restore Layout
                  </>
                )}
              </button>
            </div>
          </>
        ) : (
          <>
            {/* Restore results */}
            <div className="flex items-center gap-4 mb-6">
              <div className={`p-3 rounded-full ${successCount === totalCount ? 'bg-green-500/20' : 'bg-yellow-500/20'}`}>
                {successCount === totalCount ? (
                  <Check size={32} className="text-green-400" />
                ) : (
                  <AlertTriangle size={32} className="text-yellow-400" />
                )}
              </div>
              <div>
                <h2 className="text-xl font-bold text-white">
                  {successCount === totalCount ? 'Layout Restored!' : 'Partially Restored'}
                </h2>
                <p className="text-gray-400 text-sm">
                  {successCount} of {totalCount} windows restored
                </p>
              </div>
            </div>

            <div className="max-h-60 overflow-auto mb-6 space-y-2">
              {restoreResults.map(([name, error], idx) => (
                <div
                  key={idx}
                  className="flex items-center gap-2 p-2 rounded"
                  style={{ backgroundColor: "#0f3460" }}
                >
                  {error ? (
                    <AlertTriangle size={16} className="text-yellow-500 flex-shrink-0" />
                  ) : (
                    <Check size={16} className="text-green-500 flex-shrink-0" />
                  )}
                  <div className="flex-1 min-w-0">
                    <p className="text-white text-sm truncate">{name}</p>
                    {error && (
                      <p className="text-yellow-500 text-xs truncate">{error}</p>
                    )}
                  </div>
                </div>
              ))}
            </div>

            <button
              onClick={handleClose}
              className="w-full px-4 py-3 rounded-lg bg-cyan-500 hover:bg-cyan-600 text-white font-medium transition-colors"
            >
              Done
            </button>
          </>
        )}
      </div>
    </div>
  );
}
