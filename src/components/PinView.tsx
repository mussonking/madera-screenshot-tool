import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { X } from "lucide-react";

export default function PinView() {
  const [imageData, setImageData] = useState<string | null>(null);
  const [opacity, setOpacity] = useState(1.0);
  const [isDragging, setIsDragging] = useState(false);
  const [showControls, setShowControls] = useState(false);

  useEffect(() => {
    invoke<{ image_data: string; width: number; height: number } | null>("get_pending_capture")
      .then((data) => {
        if (data) {
          setImageData(data.image_data);
        }
      });
  }, []);

  const handleClose = async () => {
    const win = getCurrentWindow();
    await win.close();
  };

  // Allow dragging the pin window
  const handleMouseDown = async (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("button")) return;
    setIsDragging(true);
    const win = getCurrentWindow();
    await win.startDragging();
    setIsDragging(false);
  };

  if (!imageData) {
    return <div className="w-full h-full bg-black flex items-center justify-center text-white text-sm">Loading...</div>;
  }

  return (
    <div
      className="w-full h-full relative cursor-move select-none"
      style={{ opacity }}
      onMouseDown={handleMouseDown}
      onMouseEnter={() => setShowControls(true)}
      onMouseLeave={() => { if (!isDragging) setShowControls(false); }}
    >
      <img
        src={`data:image/png;base64,${imageData}`}
        alt="Pinned screenshot"
        className="w-full h-full object-contain"
        draggable={false}
      />
      {/* Controls overlay */}
      {showControls && (
        <div className="absolute top-0 right-0 flex items-center gap-1 p-1 bg-black/60 rounded-bl-lg">
          {/* Opacity slider */}
          <input
            type="range"
            min="0.2"
            max="1"
            step="0.1"
            value={opacity}
            onChange={(e) => setOpacity(parseFloat(e.target.value))}
            onMouseDown={(e) => e.stopPropagation()}
            className="w-16 h-1 accent-white"
            title={`Opacity: ${Math.round(opacity * 100)}%`}
          />
          <button
            onClick={handleClose}
            className="p-1 text-white hover:text-red-400 transition-colors"
            title="Close pin"
          >
            <X size={14} />
          </button>
        </div>
      )}
    </div>
  );
}
