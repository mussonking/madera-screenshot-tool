import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Pipette, Copy, X } from "lucide-react";
import type { ColorInfo } from "../stores/appStore";

interface MagnifierPixel {
  r: number;
  g: number;
  b: number;
}

interface PendingCapture {
  image_data: string;
  width: number;
  height: number;
  monitor_name: string;
}

export default function ColorPicker() {
  const [mousePos, setMousePos] = useState({ x: 0, y: 0 });
  const [currentColor, setCurrentColor] = useState<ColorInfo | null>(null);
  const [magnifierPixels, setMagnifierPixels] = useState<MagnifierPixel[][]>([]);
  const [copied, setCopied] = useState(false);
  const [isActive, setIsActive] = useState(true);
  const [screenCapture, setScreenCapture] = useState<PendingCapture | null>(null);
  const [loading, setLoading] = useState(true);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const screenCanvasRef = useRef<HTMLCanvasElement>(null);
  const magnifierSize = 11; // 11x11 grid

  // Load screen capture on mount (from pending capture set by Rust before window opened)
  useEffect(() => {
    const loadCapture = async () => {
      try {
        const capture = await invoke<PendingCapture | null>("get_pending_capture");
        if (capture) {
          setScreenCapture(capture);
        }
      } catch (err) {
        console.error("Failed to get pending capture:", err);
      }
      setLoading(false);
    };

    loadCapture();
  }, []);

  // Draw screen capture to canvas
  useEffect(() => {
    if (!screenCapture || !screenCanvasRef.current) return;

    const canvas = screenCanvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    canvas.width = screenCapture.width;
    canvas.height = screenCapture.height;

    const img = new Image();
    img.onload = () => {
      ctx.drawImage(img, 0, 0);
    };
    img.src = `data:image/png;base64,${screenCapture.image_data}`;
  }, [screenCapture]);

  // Get color from canvas at position
  const getColorFromCanvas = useCallback((clientX: number, clientY: number) => {
    if (!screenCanvasRef.current || !screenCapture) return null;

    const canvas = screenCanvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;

    // Get the actual pixel coordinates
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    const x = Math.floor(clientX * scaleX);
    const y = Math.floor(clientY * scaleY);

    if (x < 0 || y < 0 || x >= canvas.width || y >= canvas.height) return null;

    const pixel = ctx.getImageData(x, y, 1, 1).data;
    const r = pixel[0];
    const g = pixel[1];
    const b = pixel[2];

    // Calculate hex
    const hex = `#${r.toString(16).padStart(2, "0").toUpperCase()}${g.toString(16).padStart(2, "0").toUpperCase()}${b.toString(16).padStart(2, "0").toUpperCase()}`;
    const hexLower = hex.toLowerCase();

    // Calculate HSL
    const rNorm = r / 255;
    const gNorm = g / 255;
    const bNorm = b / 255;
    const max = Math.max(rNorm, gNorm, bNorm);
    const min = Math.min(rNorm, gNorm, bNorm);
    const l = (max + min) / 2;

    let h = 0;
    let s = 0;

    if (max !== min) {
      const d = max - min;
      s = l > 0.5 ? d / (2 - max - min) : d / (max + min);

      if (max === rNorm) {
        h = ((gNorm - bNorm) / d + (gNorm < bNorm ? 6 : 0)) / 6;
      } else if (max === gNorm) {
        h = ((bNorm - rNorm) / d + 2) / 6;
      } else {
        h = ((rNorm - gNorm) / d + 4) / 6;
      }
    }

    return {
      hex,
      hex_lower: hexLower,
      rgb: { r, g, b },
      hsl: {
        h: Math.round(h * 360),
        s: Math.round(s * 100),
        l: Math.round(l * 100),
      },
    } as ColorInfo;
  }, [screenCapture]);

  // Get magnifier pixels from canvas
  const getMagnifierFromCanvas = useCallback((clientX: number, clientY: number) => {
    if (!screenCanvasRef.current || !screenCapture) return [];

    const canvas = screenCanvasRef.current;
    const ctx = canvas.getContext("2d");
    if (!ctx) return [];

    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    const centerX = Math.floor(clientX * scaleX);
    const centerY = Math.floor(clientY * scaleY);

    const radius = Math.floor(magnifierSize / 2);
    const pixels: MagnifierPixel[][] = [];

    for (let dy = -radius; dy <= radius; dy++) {
      const row: MagnifierPixel[] = [];
      for (let dx = -radius; dx <= radius; dx++) {
        const x = centerX + dx;
        const y = centerY + dy;

        if (x < 0 || y < 0 || x >= canvas.width || y >= canvas.height) {
          row.push({ r: 0, g: 0, b: 0 });
        } else {
          const pixel = ctx.getImageData(x, y, 1, 1).data;
          row.push({ r: pixel[0], g: pixel[1], b: pixel[2] });
        }
      }
      pixels.push(row);
    }

    return pixels;
  }, [screenCapture, magnifierSize]);

  useEffect(() => {
    if (loading) return;

    const handleMouseMove = (e: MouseEvent) => {
      setMousePos({ x: e.clientX, y: e.clientY });

      const color = getColorFromCanvas(e.clientX, e.clientY);
      if (color) {
        setCurrentColor(color);
      }

      const pixels = getMagnifierFromCanvas(e.clientX, e.clientY);
      setMagnifierPixels(pixels);
    };

    const handleKeyDown = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        const win = getCurrentWindow();
        await win.close();
      }
    };

    const handleClick = async () => {
      if (!currentColor || !isActive) return;

      try {
        // Copy to clipboard
        await invoke("copy_text_to_clipboard", { text: currentColor.hex });

        // Save to history
        await invoke("save_color_pick", {
          hex: currentColor.hex,
          r: currentColor.rgb.r,
          g: currentColor.rgb.g,
          b: currentColor.rgb.b,
          h: currentColor.hsl.h,
          s: currentColor.hsl.s,
          l: currentColor.hsl.l,
          sourceApp: null,
        });

        // Show copied feedback
        setCopied(true);
        setIsActive(false);

        // Close window after a brief delay
        setTimeout(async () => {
          const win = getCurrentWindow();
          await win.close();
        }, 400);
      } catch (err) {
        console.error("Failed to save color:", err);
      }
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("click", handleClick);

    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("click", handleClick);
    };
  }, [currentColor, isActive, loading, getColorFromCanvas, getMagnifierFromCanvas]);

  // Draw magnifier on canvas
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || magnifierPixels.length === 0) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const pixelSize = 12;
    const size = magnifierSize * pixelSize;
    canvas.width = size;
    canvas.height = size;

    // Draw pixels
    magnifierPixels.forEach((row, y) => {
      row.forEach((pixel, x) => {
        ctx.fillStyle = `rgb(${pixel.r}, ${pixel.g}, ${pixel.b})`;
        ctx.fillRect(x * pixelSize, y * pixelSize, pixelSize, pixelSize);
      });
    });

    // Draw grid lines
    ctx.strokeStyle = "rgba(255, 255, 255, 0.3)";
    ctx.lineWidth = 1;

    for (let i = 0; i <= magnifierSize; i++) {
      ctx.beginPath();
      ctx.moveTo(i * pixelSize, 0);
      ctx.lineTo(i * pixelSize, size);
      ctx.stroke();

      ctx.beginPath();
      ctx.moveTo(0, i * pixelSize);
      ctx.lineTo(size, i * pixelSize);
      ctx.stroke();
    }

    // Highlight center pixel
    const center = Math.floor(magnifierSize / 2);
    ctx.strokeStyle = "#fff";
    ctx.lineWidth = 2;
    ctx.strokeRect(center * pixelSize, center * pixelSize, pixelSize, pixelSize);
  }, [magnifierPixels, magnifierSize]);

  // Calculate popup position (avoid edges)
  const popupWidth = 220;
  const popupHeight = 250;
  const offset = 20;

  let popupX = mousePos.x + offset;
  let popupY = mousePos.y + offset;

  // Adjust for screen bounds
  if (popupX + popupWidth > window.innerWidth) {
    popupX = mousePos.x - popupWidth - offset;
  }
  if (popupY + popupHeight > window.innerHeight) {
    popupY = mousePos.y - popupHeight - offset;
  }

  if (loading) {
    return (
      <div className="fixed inset-0 bg-black flex items-center justify-center">
        <div className="text-white text-lg">Capturing screen...</div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 cursor-crosshair overflow-hidden">
      {/* Screen capture as background */}
      <canvas
        ref={screenCanvasRef}
        className="absolute inset-0 w-full h-full object-cover"
        style={{ imageRendering: "auto" }}
      />

      {/* Semi-transparent overlay */}
      <div
        className="absolute inset-0 pointer-events-none"
        style={{ backgroundColor: "rgba(0, 0, 0, 0.05)" }}
      />

      {/* Floating Panel */}
      <div
        className="fixed bg-gray-900/95 backdrop-blur-sm rounded-lg shadow-2xl border border-gray-700 p-3 pointer-events-none z-50"
        style={{
          left: popupX,
          top: popupY,
          width: popupWidth,
        }}
      >
        {/* Header */}
        <div className="flex items-center gap-2 mb-3 text-white">
          <Pipette size={16} className="text-cyan-400" />
          <span className="text-sm font-medium">Color Picker</span>
          {copied && (
            <span className="ml-auto text-xs text-green-400 flex items-center gap-1">
              <Copy size={12} />
              Copied!
            </span>
          )}
        </div>

        {/* Magnifier Canvas */}
        <div className="flex justify-center mb-3">
          <canvas
            ref={canvasRef}
            className="rounded border border-gray-600"
            style={{ imageRendering: "pixelated" }}
          />
        </div>

        {/* Color Preview */}
        {currentColor && (
          <div className="space-y-2">
            {/* Color Swatch + Hex */}
            <div className="flex items-center gap-3">
              <div
                className="w-12 h-12 rounded-lg border-2 border-gray-600 shadow-inner"
                style={{ backgroundColor: currentColor.hex }}
              />
              <div className="flex-1">
                <div className="text-white font-mono text-lg font-bold">
                  {currentColor.hex}
                </div>
                <div className="text-gray-400 text-xs font-mono">
                  rgb({currentColor.rgb.r}, {currentColor.rgb.g}, {currentColor.rgb.b})
                </div>
              </div>
            </div>

            {/* HSL Info */}
            <div className="text-gray-500 text-xs font-mono">
              hsl({currentColor.hsl.h}, {currentColor.hsl.s}%, {currentColor.hsl.l}%)
            </div>
          </div>
        )}

        {/* Instructions */}
        <div className="mt-3 pt-2 border-t border-gray-700 text-center">
          <span className="text-gray-500 text-xs">
            Click to copy • ESC to cancel
          </span>
        </div>
      </div>

      {/* Center crosshair on cursor */}
      <div
        className="fixed pointer-events-none z-50"
        style={{
          left: mousePos.x - 12,
          top: mousePos.y - 12,
          width: 24,
          height: 24,
        }}
      >
        <svg viewBox="0 0 24 24" className="w-full h-full">
          {/* Outer white circle */}
          <circle
            cx="12"
            cy="12"
            r="10"
            fill="none"
            stroke="#fff"
            strokeWidth="2"
          />
          {/* Inner color circle */}
          <circle
            cx="12"
            cy="12"
            r="8"
            fill="none"
            stroke={currentColor?.hex || "#000"}
            strokeWidth="3"
          />
          {/* Center dot */}
          <circle
            cx="12"
            cy="12"
            r="2"
            fill={currentColor?.hex || "#fff"}
          />
        </svg>
      </div>

      {/* ESC hint in corner */}
      <div className="fixed top-4 right-4 bg-gray-900/90 text-white px-3 py-2 rounded-lg text-sm flex items-center gap-2 pointer-events-none z-50">
        <X size={16} />
        Press ESC to cancel
      </div>
    </div>
  );
}
