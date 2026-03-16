import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import { load } from "@tauri-apps/plugin-store";
import * as fabric from "fabric";
import {
  Pencil,
  Highlighter,
  ArrowRight,
  Square,
  Circle,
  Type,
  Hash,
  Grid3X3,
  Undo2,
  Redo2,
  Copy,
  Save,
  X,
  Trash2,
  LucideIcon,
  Sparkles,
  ZoomIn,
  ZoomOut,
  Maximize2,
  LayoutGrid,
  Upload,
} from "lucide-react";

type Tool =
  | "select"
  | "pen"
  | "highlighter"
  | "arrow"
  | "rectangle"
  | "circle"
  | "text"
  | "number"
  | "blur"
  | "magnifier"
  | "crop";

interface PendingCapture {
  image_data: string;
  width: number;
  height: number;
}

interface Toast {
  message: string;
  type: 'success' | 'error' | 'info';
  visible: boolean;
}

// Theme definitions
type ThemeName = "default" | "cyberpunk" | "retro" | "candy" | "sketch" | "neon";

interface Theme {
  name: string;
  colors: string[];
  toolbar: string;
  toolbarBorder: string;
  buttonBg: string;
  buttonActive: string;
  buttonHover: string;
  canvasBg: string;
  textColor: string;
  accentColor: string;
  // Visual style
  fontFamily: string;
  borderRadius: string;
  borderStyle: string;
  buttonStyle: string;
  glowEffect?: string;
  scanlines?: boolean;
}

const THEMES: Record<ThemeName, Theme> = {
  default: {
    name: "Default",
    colors: ["#e94560", "#ff6b35", "#f7c948", "#4ade80", "#22d3ee", "#3b82f6", "#a855f7", "#ffffff", "#000000"],
    toolbar: "#16213e",
    toolbarBorder: "#0f3460",
    buttonBg: "#0f3460",
    buttonActive: "#e94560",
    buttonHover: "rgba(233, 69, 96, 0.5)",
    canvasBg: "#1a1a2e",
    textColor: "#e5e5e5",
    accentColor: "#e94560",
    fontFamily: "'Segoe UI', system-ui, sans-serif",
    borderRadius: "8px",
    borderStyle: "solid",
    buttonStyle: "normal",
  },
  cyberpunk: {
    name: "⚡ Cyberpunk 2077",
    colors: ["#fcee0a", "#00f0ff", "#ff003c", "#ff00ff", "#00ff9f", "#ff6b00", "#bd00ff", "#ffffff", "#000000"],
    toolbar: "#0c0c0c",
    toolbarBorder: "#fcee0a",
    buttonBg: "#1a1a1a",
    buttonActive: "#fcee0a",
    buttonHover: "rgba(252, 238, 10, 0.3)",
    canvasBg: "#0a0a0a",
    textColor: "#fcee0a",
    accentColor: "#00f0ff",
    fontFamily: "'Orbitron', 'Rajdhani', 'Share Tech Mono', monospace",
    borderRadius: "0px",
    borderStyle: "solid",
    buttonStyle: "cyber",
    glowEffect: "0 0 10px #fcee0a, 0 0 20px #fcee0a40",
  },
  retro: {
    name: "▸ Terminal",
    colors: ["#33bb33", "#22aa22", "#44cc44", "#aaaa33", "#aa8833", "#aa3333", "#888888", "#33bb33", "#0a200a"],
    toolbar: "#0a140a",
    toolbarBorder: "#1a3a1a",
    buttonBg: "#0d1f0d",
    buttonActive: "#33bb33",
    buttonHover: "rgba(51, 187, 51, 0.2)",
    canvasBg: "#050d05",
    textColor: "#33bb33",
    accentColor: "#22aa22",
    fontFamily: "'VT323', 'Courier New', monospace",
    borderRadius: "0px",
    borderStyle: "solid",
    buttonStyle: "terminal",
    glowEffect: "0 0 5px #33bb3380",
    scanlines: true,
  },
  candy: {
    name: "🍬 Candy Pop",
    colors: ["#ff6b9d", "#c44569", "#f8b500", "#7ed6df", "#be2edd", "#ff9ff3", "#feca57", "#ffffff", "#2d3436"],
    toolbar: "#fff0f5",
    toolbarBorder: "#ffb6c1",
    buttonBg: "#ffe4ec",
    buttonActive: "#ff6b9d",
    buttonHover: "rgba(255, 107, 157, 0.3)",
    canvasBg: "#fff5f8",
    textColor: "#c44569",
    accentColor: "#ff6b9d",
    fontFamily: "'Comic Sans MS', 'Bubblegum Sans', cursive",
    borderRadius: "20px",
    borderStyle: "solid",
    buttonStyle: "rounded",
  },
  sketch: {
    name: "✏️ Sketch",
    colors: ["#2d3436", "#636e72", "#b2bec3", "#0984e3", "#e17055", "#00b894", "#fdcb6e", "#dfe6e9", "#000000"],
    toolbar: "#fefefe",
    toolbarBorder: "#ccc",
    buttonBg: "#f0f0f0",
    buttonActive: "#2d3436",
    buttonHover: "rgba(45, 52, 54, 0.15)",
    canvasBg: "#f8f8f8",
    textColor: "#2d3436",
    accentColor: "#0984e3",
    fontFamily: "'Segoe Print', 'Patrick Hand', cursive",
    borderRadius: "4px",
    borderStyle: "dashed",
    buttonStyle: "sketch",
  },
  neon: {
    name: "🌈 Neon Glow",
    colors: ["#ff00ff", "#00ffff", "#ffff00", "#ff0080", "#00ff80", "#8000ff", "#ff8000", "#ffffff", "#000000"],
    toolbar: "#0a0015",
    toolbarBorder: "#ff00ff",
    buttonBg: "#150025",
    buttonActive: "#ff00ff",
    buttonHover: "rgba(255, 0, 255, 0.3)",
    canvasBg: "#050010",
    textColor: "#ff88ff",
    accentColor: "#00ffff",
    fontFamily: "'Audiowide', 'Orbitron', sans-serif",
    borderRadius: "12px",
    borderStyle: "solid",
    buttonStyle: "neon",
    glowEffect: "0 0 10px currentColor, 0 0 20px currentColor, 0 0 30px currentColor",
  },
};

const STROKE_WIDTHS = [1, 2, 4, 6, 8, 12];

// Padding around the image for annotations outside bounds
const CANVAS_PADDING = 50;

const STORE_PATH = "settings.json";

// Helper to load theme from Tauri store
const loadThemeFromStore = async (): Promise<ThemeName> => {
  try {
    const store = await load(STORE_PATH);
    const saved = await store.get<string>("theme");
    if (saved && saved in THEMES) {
      return saved as ThemeName;
    }
  } catch {
    // Store not available
  }
  return "default";
};

// Helper to save theme to Tauri store
const saveThemeToStore = async (themeName: ThemeName) => {
  try {
    const store = await load(STORE_PATH);
    await store.set("theme", themeName);
    await store.save();
  } catch {
    // Store not available
  }
};

export default function Editor() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const fabricRef = useRef<fabric.Canvas | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const [tool, setTool] = useState<Tool>("pen"); // Default to pen
  const [currentTheme, setCurrentTheme] = useState<ThemeName>("default");
  const [color, setColor] = useState("#ff0000"); // Always bright red default
  const [themeLoaded, setThemeLoaded] = useState(false);
  const [strokeWidth, setStrokeWidth] = useState(2);
  const [showColorPicker, setShowColorPicker] = useState(false);
  const [showThemePicker, setShowThemePicker] = useState(false);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [imageSize, setImageSize] = useState({ width: 0, height: 0 });
  const [baseCanvasSize, setBaseCanvasSize] = useState({ width: 0, height: 0 });
  const [zoom, setZoom] = useState(1);
  const [isUploading, setIsUploading] = useState(false);
  const [toast, setToast] = useState<Toast>({ message: '', type: 'info', visible: false });

  const showNotification = (message: string, type: 'success' | 'error' | 'info' = 'info') => {
    setToast({ message, type, visible: true });
    setTimeout(() => setToast(prev => ({ ...prev, visible: false })), 3000);
  };

  // Get current theme and colors
  const theme = THEMES[currentTheme];
  const COLORS = theme.colors;

  // Use refs for history to avoid stale closure issues
  const historyRef = useRef<string[]>([]);
  const historyIndexRef = useRef(-1);

  // Ref for copyToClipboard to be accessible in global event listener
  const copyToClipboardRef = useRef<() => void>(() => { });

  // Load theme from store on mount
  useEffect(() => {
    loadThemeFromStore().then((savedTheme) => {
      setCurrentTheme(savedTheme);
      // Keep bright red as default - don't change color based on theme
      setThemeLoaded(true);
    });
  }, []);

  // Listen for global copy event (Ctrl+P from anywhere)
  useEffect(() => {
    const unlisten = listen("global-copy", () => {
      copyToClipboardRef.current();
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Initialize canvas - only runs once
  useEffect(() => {
    loadImage();

    return () => {
      if (fabricRef.current) {
        fabricRef.current.dispose();
      }
    };
  }, []);

  // Keyboard and wheel event handlers
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        closeWindow();
      } else if (e.key === "Delete") {
        e.preventDefault();
        deleteSelected();
      } else if (e.ctrlKey && e.key === "z") {
        e.preventDefault();
        undo();
      } else if (e.ctrlKey && e.key === "y") {
        e.preventDefault();
        redo();
      } else if (e.ctrlKey && e.key === "c") {
        // Always copy the full image to clipboard (Ctrl+C)
        e.preventDefault();
        copyToClipboardRef.current();
      } else if (e.ctrlKey && e.key === "p") {
        e.preventDefault();
        copyToClipboardRef.current();
      } else if (e.ctrlKey && e.key === "s") {
        e.preventDefault();
        saveToFile();
      } else if (e.ctrlKey && (e.key === "=" || e.key === "+")) {
        e.preventDefault();
        zoomIn();
      } else if (e.ctrlKey && e.key === "-") {
        e.preventDefault();
        zoomOut();
      } else if (e.ctrlKey && e.key === "0") {
        e.preventDefault();
        resetZoom();
      } else {
        // Tool shortcuts
        const shortcuts: Record<string, Tool> = {
          p: "pen",
          h: "highlighter",
          a: "arrow",
          r: "rectangle",
          c: "circle",
          t: "text",
          n: "number",
          b: "blur",
          m: "magnifier",
          x: "crop",
          v: "select",
        };
        if (shortcuts[e.key.toLowerCase()] && !e.ctrlKey && !e.altKey) {
          setTool(shortcuts[e.key.toLowerCase()]);
        }
        // Number keys for colors
        const num = parseInt(e.key);
        if (num >= 1 && num <= 9 && !e.ctrlKey) {
          setColor(COLORS[num - 1]);
        }
      }
    };

    const handleWheel = (e: WheelEvent) => {
      if (e.ctrlKey) {
        e.preventDefault();
        const canvas = fabricRef.current;
        if (!canvas || baseCanvasSize.width === 0) return;

        const delta = e.deltaY > 0 ? 0.9 : 1.1;
        const newZoom = Math.min(Math.max(zoom * delta, 0.25), 5);
        setZoom(newZoom);
        canvas.setZoom(newZoom);
        canvas.setDimensions({
          width: baseCanvasSize.width * newZoom,
          height: baseCanvasSize.height * newZoom,
        });
        canvas.renderAll();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("wheel", handleWheel, { passive: false });
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("wheel", handleWheel);
    };
  }, [zoom, baseCanvasSize]);

  // Update canvas mode when tool changes
  useEffect(() => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    canvas.isDrawingMode = tool === "pen" || tool === "highlighter";

    if (canvas.isDrawingMode) {
      const brush = new fabric.PencilBrush(canvas);
      brush.color = tool === "highlighter" ? `${color}80` : color;
      brush.width = tool === "highlighter" ? strokeWidth * 3 : strokeWidth;
      canvas.freeDrawingBrush = brush;
    }

    canvas.selection = tool === "select";
    canvas.forEachObject((obj) => {
      obj.selectable = tool === "select";
      // Enable rotation control
      obj.setControlsVisibility({
        mtr: true, // rotation control
      });
    });
  }, [tool, color, strokeWidth]);

  // Update selected object's color when color changes
  useEffect(() => {
    const canvas = fabricRef.current;
    if (!canvas || tool !== "select") return;

    const activeObjects = canvas.getActiveObjects();
    if (activeObjects.length > 0) {
      activeObjects.forEach((obj) => {
        if (obj.type === "path") {
          // For drawn paths
          obj.set("stroke", color);
        } else if (obj.type === "rect" || obj.type === "circle") {
          // For shapes
          obj.set("stroke", color);
        } else if (obj.type === "line") {
          obj.set("stroke", color);
        } else if (obj.type === "polygon") {
          obj.set("fill", color);
        } else if (obj.type === "i-text" || obj.type === "text") {
          obj.set("fill", color);
        } else if (obj.type === "group") {
          // For groups (like arrows), update child objects
          const group = obj as fabric.Group;
          group.getObjects().forEach((child) => {
            if (child.type === "line") {
              child.set("stroke", color);
            } else if (child.type === "polygon") {
              child.set("fill", color);
            } else if (child.type === "circle") {
              child.set("fill", color);
            }
          });
        }
      });
      canvas.renderAll();
    }
  }, [color, tool]);

  const loadImage = async () => {
    try {
      console.log('Fetching pending capture...');
      const pending = await invoke<PendingCapture | null>("get_pending_capture");
      if (!pending) {
        console.error("No pending capture returned from backend");
        return;
      }
      setImageSize({ width: pending.width, height: pending.height });

      // Wait for DOM to be ready
      setTimeout(() => {
        console.log('Initializing canvas...');
        initCanvas(pending.image_data, pending.width, pending.height);
      }, 100);
    } catch (err) {
      console.error("Failed to load image:", err);
    }
  };

  const initCanvas = (imageData: string, width: number, height: number) => {
    if (!canvasRef.current || !containerRef.current) return;

    // Clean up existing canvas
    if (fabricRef.current) {
      fabricRef.current.dispose();
    }

    // Calculate canvas size to fit in container
    // Calculate canvas size to fit in container
    const container = containerRef.current;

    // Fallback to window size if container is not ready (prevents 0/negative values)
    const cw = container.clientWidth || window.innerWidth;
    const ch = container.clientHeight || window.innerHeight;

    const maxWidth = cw - 40;
    const maxHeight = ch - 40;

    // Calculate scaled image dimensions
    let imageWidth = width;
    let imageHeight = height;

    if (width > maxWidth - CANVAS_PADDING * 2 || height > maxHeight - CANVAS_PADDING * 2) {
      const ratio = Math.min((maxWidth - CANVAS_PADDING * 2) / width, (maxHeight - CANVAS_PADDING * 2) / height);
      imageWidth = Math.floor(width * ratio);
      imageHeight = Math.floor(height * ratio);
    }

    // Canvas size = image size + padding on all sides
    const canvasWidth = imageWidth + CANVAS_PADDING * 2;
    const canvasHeight = imageHeight + CANVAS_PADDING * 2;

    // Store base canvas size for zoom calculations
    setBaseCanvasSize({ width: canvasWidth, height: canvasHeight });

    // Create fabric canvas
    const canvas = new fabric.Canvas(canvasRef.current, {
      width: canvasWidth,
      height: canvasHeight,
      backgroundColor: "#2a2a3e",
    });

    fabricRef.current = canvas;

    // Load background image
    const img = new Image();

    img.onload = () => {
      const fabricImage = new fabric.FabricImage(img, {
        left: CANVAS_PADDING,
        top: CANVAS_PADDING,
        scaleX: imageWidth / width,
        scaleY: imageHeight / height,
        selectable: false,
        evented: false,
      });
      canvas.backgroundImage = fabricImage;
      canvas.renderAll();
      saveState();
    };

    img.onerror = (err) => {
      console.error("FAILED TO LOAD IMAGE FROM BASE64", err);
      showNotification("Error: Failed to load image data. The screenshot data might be corrupted.", "error");
    };

    img.src = `data:image/png;base64,${imageData}`;

    // Setup event handlers
    canvas.on("mouse:down", handleMouseDown);
    canvas.on("mouse:up", handleMouseUp);
    canvas.on("path:created", saveState);
    canvas.on("object:modified", saveState);

    // Initialize drawing mode (default is pen)
    canvas.isDrawingMode = true;
    const brush = new fabric.PencilBrush(canvas);
    brush.color = color;
    brush.width = strokeWidth;
    canvas.freeDrawingBrush = brush;
  };

  const handleMouseDown = (opt: fabric.TPointerEventInfo) => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const pointer = canvas.getScenePoint(opt.e);

    if (tool === "text") {
      const text = new fabric.IText("Click to edit", {
        left: pointer.x,
        top: pointer.y,
        fontSize: 20,
        fill: color,
        fontFamily: "Arial",
      });
      canvas.add(text);
      canvas.setActiveObject(text);
      text.enterEditing();
      saveState();
    } else if (tool === "number") {
      const nextNumber = getNextAvailableNumber();
      const circle = new fabric.Circle({
        radius: 15,
        fill: color,
        left: pointer.x - 15,
        top: pointer.y - 15,
      });
      const text = new fabric.FabricText(nextNumber.toString(), {
        left: pointer.x,
        top: pointer.y,
        fontSize: 18,
        fill: "#ffffff",
        fontFamily: "Arial",
        fontWeight: "bold",
        originX: "center",
        originY: "center",
      });
      const group = new fabric.Group([circle, text], {
        left: pointer.x - 15,
        top: pointer.y - 15,
      });
      canvas.add(group);
      saveState();
    }
  };

  const handleMouseUp = () => {
    // Handle shape completion if needed
  };

  // Shape drawing functions
  const drawArrow = () => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const centerX = baseCanvasSize.width / 2;
    const centerY = baseCanvasSize.height / 2;

    const line = new fabric.Line([centerX - 50, centerY, centerX + 50, centerY], {
      stroke: color,
      strokeWidth: strokeWidth,
      strokeUniform: true, // Stroke stays constant when resizing
    });

    // Arrow head
    const headLength = 15;
    const angle = 0;
    const head = new fabric.Polygon(
      [
        { x: 0, y: 0 },
        { x: -headLength, y: headLength / 2 },
        { x: -headLength, y: -headLength / 2 },
      ],
      {
        fill: color,
        left: centerX + 50,
        top: centerY,
        angle: angle,
        originX: "center",
        originY: "center",
      }
    );

    const group = new fabric.Group([line, head], {
      left: centerX - 50,
      top: centerY - headLength / 2,
    });

    canvas.add(group);
    canvas.setActiveObject(group);
    saveState();
    setTool("select"); // Auto-switch to select after placing shape
  };

  const drawRectangle = () => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const rect = new fabric.Rect({
      left: baseCanvasSize.width / 2 - 50,
      top: baseCanvasSize.height / 2 - 30,
      width: 100,
      height: 60,
      fill: "transparent",
      stroke: color,
      strokeWidth: strokeWidth,
      strokeUniform: true, // Stroke stays constant when resizing
    });

    canvas.add(rect);
    canvas.setActiveObject(rect);
    saveState();
    setTool("select"); // Auto-switch to select after placing shape
  };

  const drawCircle = () => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const circle = new fabric.Circle({
      left: baseCanvasSize.width / 2 - 40,
      top: baseCanvasSize.height / 2 - 40,
      radius: 40,
      fill: "transparent",
      stroke: color,
      strokeWidth: strokeWidth,
      strokeUniform: true, // Stroke stays constant when resizing
    });

    canvas.add(circle);
    canvas.setActiveObject(circle);
    saveState();
    setTool("select"); // Auto-switch to select after placing shape
  };

  const applyBlur = () => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    // Add a blur rectangle
    const rect = new fabric.Rect({
      left: baseCanvasSize.width / 2 - 50,
      top: baseCanvasSize.height / 2 - 30,
      width: 100,
      height: 60,
      fill: "#888888",
      opacity: 0.9,
    });

    canvas.add(rect);
    canvas.setActiveObject(rect);
    saveState();
    setTool("select"); // Auto-switch to select after placing shape
  };

  // History management using refs to avoid stale closure issues
  const saveState = useCallback(() => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const json = JSON.stringify(canvas.toJSON());
    const currentIndex = historyIndexRef.current;
    const newHistory = historyRef.current.slice(0, currentIndex + 1);
    newHistory.push(json);
    historyRef.current = newHistory;
    historyIndexRef.current = newHistory.length - 1;
    setCanUndo(historyIndexRef.current > 0);
    setCanRedo(false);
  }, []);

  const undo = useCallback(() => {
    const canvas = fabricRef.current;
    if (!canvas || historyIndexRef.current <= 0) return;

    const newIndex = historyIndexRef.current - 1;
    const stateToLoad = historyRef.current[newIndex];

    canvas.loadFromJSON(stateToLoad).then(() => {
      canvas.renderAll();
      historyIndexRef.current = newIndex;
      setCanUndo(newIndex > 0);
      setCanRedo(true);
    });
  }, []);

  const redo = useCallback(() => {
    const canvas = fabricRef.current;
    if (!canvas || historyIndexRef.current >= historyRef.current.length - 1) return;

    const newIndex = historyIndexRef.current + 1;
    const stateToLoad = historyRef.current[newIndex];

    canvas.loadFromJSON(stateToLoad).then(() => {
      canvas.renderAll();
      historyIndexRef.current = newIndex;
      setCanUndo(true);
      setCanRedo(newIndex < historyRef.current.length - 1);
    });
  }, []);

  const clearAnnotations = () => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    // Remove all objects except background
    canvas.getObjects().forEach((obj) => {
      canvas.remove(obj);
    });
    canvas.renderAll();
    saveState();
  };

  const deleteSelected = () => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const activeObjects = canvas.getActiveObjects();
    activeObjects.forEach((obj) => {
      canvas.remove(obj);
    });
    canvas.discardActiveObject();
    canvas.renderAll();
    saveState();
  };

  // Get the next available number by finding gaps in existing numbered annotations
  const getNextAvailableNumber = (): number => {
    const canvas = fabricRef.current;
    if (!canvas) return 1;

    // Find all numbers currently used in numbered annotations (Groups with circle + text)
    const usedNumbers: number[] = [];

    canvas.getObjects().forEach((obj) => {
      // Check if it's a group (numbered annotation)
      if (obj.type === "group") {
        const group = obj as fabric.Group;
        const objects = group.getObjects();

        // Look for the text element inside the group
        for (const child of objects) {
          if (child.type === "text") {
            const textObj = child as fabric.FabricText;
            const num = parseInt(textObj.text || "", 10);
            if (!isNaN(num) && num > 0) {
              usedNumbers.push(num);
            }
          }
        }
      }
    });

    // If no numbers used, return 1
    if (usedNumbers.length === 0) {
      return 1;
    }

    // Sort and find the first gap
    usedNumbers.sort((a, b) => a - b);

    for (let i = 1; i <= usedNumbers.length + 1; i++) {
      if (!usedNumbers.includes(i)) {
        return i; // First missing number
      }
    }

    // Fallback (should not reach here)
    return Math.max(...usedNumbers) + 1;
  };

  // Export functions
  const getExportedImage = async (): Promise<string> => {
    const canvas = fabricRef.current;
    if (!canvas) {
      showNotification("Error: Canvas not found", "error");
      return "";
    }

    try {
      // Deselect any active objects so selection borders don't get exported
      canvas.discardActiveObject();
      canvas.renderAll();

      // Simplest possible export: exactly what is on the canvas right now
      const finalDataUrl = canvas.toDataURL({
        format: "png",
        multiplier: 1,
        quality: 1.0
      });

      if (!finalDataUrl || finalDataUrl.length < 100) {
         showNotification("Error: Generated image is empty", "error");
         return "";
      }

      // Return base64 without the data URL prefix
      return finalDataUrl.replace(/^data:image\/png;base64,/, "");
    } catch (e) {
      console.error("Exception in getExportedImage:", e);
      showNotification(`Export exception: ${e}`, "error");
      return "";
    }
  };

  const copyToClipboard = useCallback(async () => {
    try {
      console.log("copyToClipboard invoked!");
      const imageData = await getExportedImage();
      console.log("getExportedImage returned image data of length:", imageData?.length);
      if (!imageData) {
        showNotification("Error: exported image is empty!", "error");
        return;
      }

      console.log("Invoking copy_to_clipboard...");
      await invoke("copy_to_clipboard", { imageData });
      console.log("copy_to_clipboard success!");
      showNotification(`Copied to clipboard! (Data len: ${Math.round(imageData.length / 1024)}KB)`, "success");

      // Save to history
      console.log("Invoking save_to_history...");
      await invoke("save_to_history", {
        imageData,
        width: imageSize.width,
        height: imageSize.height,
      });

      // Don't close window - user may want to continue editing
    } catch (err) {
      console.error("Failed to copy:", err);
      showNotification(`Copy Failed: ${err}`, "error");
    }
  }, [imageSize]);

  // Update ref when copyToClipboard changes
  useEffect(() => {
    copyToClipboardRef.current = copyToClipboard;
  }, [copyToClipboard]);

  const saveToFile = async () => {
    try {
      // Get default save path from backend (Pictures/sstool/)
      const defaultPath = await invoke<string>("get_default_save_path");

      const path = await save({
        filters: [{ name: "PNG Image", extensions: ["png"] }],
        defaultPath: defaultPath,
      });

      if (path) {
        const imageData = await getExportedImage();
        await invoke("save_image_to_file", { imageData, path });

        // Also save to history
        await invoke("save_to_history", {
          imageData,
          width: imageSize.width,
          height: imageSize.height,
        });
      }
    } catch (err) {
      console.error("Failed to save:", err);
    }
  };

  const uploadToCLI = async () => {
    try {
      setIsUploading(true);
      const imageData = await getExportedImage();
      const remotePath = await invoke<string>('upload_to_dev_server', { imageData });

      // Show success notification
      console.log(`✅ Uploaded! Path copied: ${remotePath}`);
      showNotification(`Upload réussi ! Lien copié.`, 'success');

      // Optional: close window after upload
      // await closeWindow();
    } catch (err) {
      console.error("Upload failed:", err);
      showNotification(`Erreur: ${err}`, 'error');
    } finally {
      setIsUploading(false);
    }
  };

  const closeWindow = async () => {
    try {
      const window = getCurrentWindow();
      await window.close();
    } catch (err) {
      console.error("Failed to close:", err);
    }
  };

  // Zoom functions
  const zoomIn = () => {
    const canvas = fabricRef.current;
    if (!canvas || baseCanvasSize.width === 0) return;
    const newZoom = Math.min(zoom * 1.2, 5); // Max 5x zoom
    setZoom(newZoom);
    canvas.setZoom(newZoom);
    canvas.setDimensions({
      width: baseCanvasSize.width * newZoom,
      height: baseCanvasSize.height * newZoom,
    });
    canvas.renderAll();
  };

  const zoomOut = () => {
    const canvas = fabricRef.current;
    if (!canvas || baseCanvasSize.width === 0) return;
    const newZoom = Math.max(zoom / 1.2, 0.25); // Min 0.25x zoom
    setZoom(newZoom);
    canvas.setZoom(newZoom);
    canvas.setDimensions({
      width: baseCanvasSize.width * newZoom,
      height: baseCanvasSize.height * newZoom,
    });
    canvas.renderAll();
  };

  const resetZoom = () => {
    const canvas = fabricRef.current;
    if (!canvas || baseCanvasSize.width === 0) return;
    setZoom(1);
    canvas.setZoom(1);
    canvas.setDimensions({
      width: baseCanvasSize.width,
      height: baseCanvasSize.height,
    });
    canvas.renderAll();
  };

  const addText = () => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const text = new fabric.IText("Click to edit", {
      left: baseCanvasSize.width / 2 - 50,
      top: baseCanvasSize.height / 2 - 10,
      fontSize: 20,
      fill: color,
      fontFamily: "Arial",
    });
    canvas.add(text);
    canvas.setActiveObject(text);
    text.enterEditing();
    saveState();
    setTool("select");
  };

  const addNumber = () => {
    const canvas = fabricRef.current;
    if (!canvas) return;

    const centerX = baseCanvasSize.width / 2;
    const centerY = baseCanvasSize.height / 2;
    const nextNumber = getNextAvailableNumber();

    const circle = new fabric.Circle({
      radius: 15,
      fill: color,
      left: centerX - 15,
      top: centerY - 15,
    });
    const text = new fabric.FabricText(nextNumber.toString(), {
      left: centerX,
      top: centerY,
      fontSize: 18,
      fill: "#ffffff",
      fontFamily: "Arial",
      fontWeight: "bold",
      originX: "center",
      originY: "center",
    });
    const group = new fabric.Group([circle, text], {
      left: centerX - 15,
      top: centerY - 15,
    });
    canvas.add(group);
    canvas.setActiveObject(group);
    saveState();
    setTool("select");
  };

  const ToolButton = ({
    toolName,
    icon: Icon,
    shortcut,
  }: {
    toolName: Tool;
    icon: LucideIcon | React.ComponentType<{ size?: number }>;
    shortcut: string;
  }) => (
    <button
      onClick={() => {
        if (toolName === "arrow") drawArrow();
        else if (toolName === "rectangle") drawRectangle();
        else if (toolName === "circle") drawCircle();
        else if (toolName === "blur") applyBlur();
        else if (toolName === "text") addText();
        else if (toolName === "number") addNumber();
        else setTool(toolName);
      }}
      style={{
        backgroundColor: tool === toolName ? theme.buttonActive : theme.buttonBg,
        color: tool === toolName ? "#fff" : theme.textColor,
        borderRadius: theme.borderRadius,
        boxShadow: tool === toolName && theme.glowEffect ? theme.glowEffect : undefined,
      }}
      className="p-2 transition-all hover:opacity-80"
      title={`${toolName} (${shortcut})`}
    >
      <Icon size={20} />
    </button>
  );

  const handleThemeChange = (newTheme: ThemeName) => {
    setCurrentTheme(newTheme);
    setShowThemePicker(false);
    // Keep current color - don't change based on theme
    // Save to Tauri store
    saveThemeToStore(newTheme);
  };

  // Get theme-specific classes
  const getThemeClasses = () => {
    switch (currentTheme) {
      case "retro":
        return "scanlines crt-glow";
      case "cyberpunk":
        return "cyber-clip";
      case "neon":
        return "neon-glow";
      default:
        return "";
    }
  };

  // Show loading state while theme loads
  if (!themeLoaded) {
    return <div className="h-full" style={{ backgroundColor: "#1a1a2e" }} />;
  }

  return (
    <div
      className={`h-screen w-screen flex flex-col overflow-hidden ${getThemeClasses()}`}
      style={{
        backgroundColor: theme.canvasBg,
        fontFamily: theme.fontFamily,
      }}
    >
      {/* Toolbar - All tools grouped together on the left */}
      <div
        className="p-2 flex items-center gap-2 shadow-md"
        style={{
          backgroundColor: theme.toolbar,
          borderBottom: `2px ${theme.borderStyle} ${theme.toolbarBorder} `,
          boxShadow: theme.glowEffect ? `${theme.glowEffect} ` : undefined,
        }}
      >
        {/* Drawing Tools */}
        <div
          className="flex gap-1 p-1"
          style={{
            backgroundColor: `${theme.buttonBg} 80`,
            borderRadius: theme.borderRadius,
            border: `1px ${theme.borderStyle} ${theme.toolbarBorder} `,
          }}
        >
          <ToolButton toolName="select" icon={() => <span className="text-sm font-bold">V</span>} shortcut="V" />
          <ToolButton toolName="pen" icon={Pencil} shortcut="P" />
          <ToolButton toolName="highlighter" icon={Highlighter} shortcut="H" />
        </div>

        {/* Shape Tools */}
        <div
          className="flex gap-1 p-1"
          style={{
            backgroundColor: `${theme.buttonBg} 80`,
            borderRadius: theme.borderRadius,
            border: `1px ${theme.borderStyle} ${theme.toolbarBorder} `,
          }}
        >
          <ToolButton toolName="arrow" icon={ArrowRight} shortcut="A" />
          <ToolButton toolName="rectangle" icon={Square} shortcut="R" />
          <ToolButton toolName="circle" icon={Circle} shortcut="C" />
          <ToolButton toolName="blur" icon={Grid3X3} shortcut="B" />
        </div>

        {/* Text Tools */}
        <div
          className="flex gap-1 p-1"
          style={{
            backgroundColor: `${theme.buttonBg} 80`,
            borderRadius: theme.borderRadius,
            border: `1px ${theme.borderStyle} ${theme.toolbarBorder} `,
          }}
        >
          <ToolButton toolName="text" icon={Type} shortcut="T" />
          <ToolButton toolName="number" icon={Hash} shortcut="N" />
        </div>

        {/* History & Delete */}
        <div
          className="flex gap-1 p-1"
          style={{
            backgroundColor: `${theme.buttonBg} 80`,
            borderRadius: theme.borderRadius,
            border: `1px ${theme.borderStyle} ${theme.toolbarBorder} `,
          }}
        >
          <button
            onClick={undo}
            disabled={!canUndo}
            style={{
              backgroundColor: theme.buttonBg,
              color: theme.textColor,
              borderRadius: theme.borderRadius,
            }}
            className="p-2 hover:opacity-80 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            title="Undo (Ctrl+Z)"
          >
            <Undo2 size={20} />
          </button>
          <button
            onClick={redo}
            disabled={!canRedo}
            style={{
              backgroundColor: theme.buttonBg,
              color: theme.textColor,
              borderRadius: theme.borderRadius,
            }}
            className="p-2 hover:opacity-80 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            title="Redo (Ctrl+Y)"
          >
            <Redo2 size={20} />
          </button>
          <button
            onClick={deleteSelected}
            style={{
              backgroundColor: theme.buttonBg,
              color: theme.textColor,
              borderRadius: theme.borderRadius,
            }}
            className="p-2 hover:bg-red-500/50 transition-colors"
            title="Delete selected (Del)"
          >
            <Trash2 size={20} />
          </button>
          <button
            onClick={clearAnnotations}
            style={{
              backgroundColor: theme.buttonBg,
              color: theme.textColor,
              borderRadius: theme.borderRadius,
            }}
            className="p-2 hover:bg-red-500/50 transition-colors"
            title="Clear all annotations"
          >
            <X size={20} />
          </button>
        </div>

        {/* Zoom Controls */}
        <div
          className="flex gap-1 p-1 items-center"
          style={{
            backgroundColor: `${theme.buttonBg} 80`,
            borderRadius: theme.borderRadius,
            border: `1px ${theme.borderStyle} ${theme.toolbarBorder} `,
          }}
        >
          <button
            onClick={zoomOut}
            style={{
              backgroundColor: theme.buttonBg,
              color: theme.textColor,
              borderRadius: theme.borderRadius,
            }}
            className="p-2 hover:opacity-80 transition-colors"
            title="Zoom out (Ctrl+-)"
          >
            <ZoomOut size={20} />
          </button>
          <span
            className="px-2 text-xs min-w-[48px] text-center"
            style={{ color: theme.textColor }}
          >
            {Math.round(zoom * 100)}%
          </span>
          <button
            onClick={zoomIn}
            style={{
              backgroundColor: theme.buttonBg,
              color: theme.textColor,
              borderRadius: theme.borderRadius,
            }}
            className="p-2 hover:opacity-80 transition-colors"
            title="Zoom in (Ctrl++)"
          >
            <ZoomIn size={20} />
          </button>
          <button
            onClick={resetZoom}
            style={{
              backgroundColor: theme.buttonBg,
              color: theme.textColor,
              borderRadius: theme.borderRadius,
            }}
            className="p-2 hover:opacity-80 transition-colors"
            title="Reset zoom (Ctrl+0)"
          >
            <Maximize2 size={20} />
          </button>
        </div>

        {/* Color Picker */}
        <div className="relative">
          <button
            onClick={() => setShowColorPicker(!showColorPicker)}
            style={{
              backgroundColor: theme.buttonBg,
              border: `2px solid ${color} `,
              borderRadius: theme.borderRadius,
            }}
            className="p-1.5 hover:opacity-80 transition-colors"
            title="Color"
          >
            <div className="w-6 h-6" style={{ backgroundColor: color, borderRadius: theme.borderRadius }} />
          </button>
          {showColorPicker && (
            <div
              className="absolute top-full mt-2 left-0 p-3 shadow-xl z-50"
              style={{
                backgroundColor: theme.toolbar,
                border: `2px ${theme.borderStyle} ${theme.accentColor} `,
                borderRadius: theme.borderRadius,
                boxShadow: theme.glowEffect,
              }}
            >
              {/* Theme colors */}
              <div className="flex gap-2 mb-3">
                {COLORS.map((c, i) => (
                  <button
                    key={c}
                    onClick={() => {
                      setColor(c);
                      setShowColorPicker(false);
                    }}
                    className={`w - 7 h - 7 ${color === c ? "ring-2 ring-white scale-110" : "hover:scale-110"} transition - all shadow - md`}
                    style={{ backgroundColor: c, borderRadius: theme.borderRadius }}
                    title={`Color ${i + 1} `}
                  />
                ))}
              </div>
              {/* Extended palette */}
              <div className="grid grid-cols-10 gap-1 mb-3">
                {[
                  "#ff0000", "#ff4500", "#ff8c00", "#ffd700", "#ffff00",
                  "#9acd32", "#32cd32", "#00fa9a", "#00ced1", "#00bfff",
                  "#1e90ff", "#4169e1", "#8a2be2", "#9400d3", "#ff1493",
                  "#dc143c", "#b22222", "#8b4513", "#a0522d", "#d2691e",
                  "#f4a460", "#daa520", "#bdb76b", "#808000", "#6b8e23",
                  "#228b22", "#2e8b57", "#20b2aa", "#008b8b", "#5f9ea0",
                  "#4682b4", "#6495ed", "#7b68ee", "#ba55d3", "#db7093",
                ].map((c) => (
                  <button
                    key={c}
                    onClick={() => {
                      setColor(c);
                      setShowColorPicker(false);
                    }}
                    className={`w - 5 h - 5 ${color === c ? "ring-1 ring-white" : "hover:scale-125"} transition - all`}
                    style={{ backgroundColor: c, borderRadius: "2px" }}
                  />
                ))}
              </div>
              {/* Custom color input */}
              <div className="flex items-center gap-2">
                <input
                  type="color"
                  value={color}
                  onChange={(e) => setColor(e.target.value)}
                  className="w-8 h-8 cursor-pointer border-0 p-0"
                  style={{ borderRadius: theme.borderRadius }}
                />
                <input
                  type="text"
                  value={color}
                  onChange={(e) => {
                    if (/^#[0-9a-fA-F]{6}$/.test(e.target.value)) {
                      setColor(e.target.value);
                    }
                  }}
                  placeholder="#ffffff"
                  className="flex-1 px-2 py-1 text-xs font-mono"
                  style={{
                    backgroundColor: theme.buttonBg,
                    color: theme.textColor,
                    borderRadius: theme.borderRadius,
                    border: `1px solid ${theme.toolbarBorder} `,
                  }}
                />
              </div>
            </div>
          )}
        </div>

        {/* Stroke Width */}
        <div
          className="flex gap-1 p-1"
          style={{
            backgroundColor: `${theme.buttonBg} 80`,
            borderRadius: theme.borderRadius,
            border: `1px ${theme.borderStyle} ${theme.toolbarBorder} `,
          }}
        >
          {STROKE_WIDTHS.map((w) => (
            <button
              key={w}
              onClick={() => setStrokeWidth(w)}
              style={{
                backgroundColor: strokeWidth === w ? theme.buttonActive : theme.buttonBg,
                color: strokeWidth === w ? "#fff" : theme.textColor,
                borderRadius: theme.borderRadius,
              }}
              className="w-8 h-8 flex items-center justify-center transition-all hover:opacity-80"
              title={`Stroke width ${w} px`}
            >
              <div
                className="rounded-full bg-current"
                style={{ width: w + 2, height: w + 2 }}
              />
            </button>
          ))}
        </div>

        {/* Spacer to push theme picker to the right */}
        <div className="flex-1" />

        {/* Theme Picker */}
        <div className="relative">
          <button
            onClick={() => setShowThemePicker(!showThemePicker)}
            style={{
              backgroundColor: theme.buttonBg,
              color: theme.accentColor,
              borderRadius: theme.borderRadius,
              border: `1px ${theme.borderStyle} ${theme.toolbarBorder} `,
            }}
            className="p-2 hover:opacity-80 transition-colors"
            title="Change Theme"
          >
            <Sparkles size={20} />
          </button>
          {showThemePicker && (
            <div
              className="absolute top-full mt-2 right-0 p-2 shadow-xl z-50 min-w-[180px]"
              style={{
                backgroundColor: theme.toolbar,
                border: `2px ${theme.borderStyle} ${theme.accentColor} `,
                borderRadius: theme.borderRadius,
                boxShadow: theme.glowEffect,
              }}
            >
              <div className="flex flex-col gap-1">
                {(Object.keys(THEMES) as ThemeName[]).map((themeName) => (
                  <button
                    key={themeName}
                    onClick={() => handleThemeChange(themeName)}
                    className="px-3 py-2 text-left text-sm transition-all hover:opacity-80"
                    style={{
                      backgroundColor: currentTheme === themeName ? THEMES[themeName].buttonActive : theme.buttonBg,
                      color: currentTheme === themeName ? "#fff" : theme.textColor,
                      borderRadius: theme.borderRadius,
                      fontFamily: THEMES[themeName].fontFamily,
                    }}
                  >
                    {currentTheme === themeName ? "▶ " : "  "}
                    {THEMES[themeName].name}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Canvas Container */}
      <div ref={containerRef} className="flex-1 flex items-center justify-center p-4 overflow-auto">
        <canvas ref={canvasRef} className="shadow-2xl" />
      </div>

      {/* Bottom Action Bar */}
      <div
        className="p-3 flex items-center justify-center gap-4"
        style={{
          backgroundColor: theme.toolbar,
          borderTop: `2px ${theme.borderStyle} ${theme.toolbarBorder} `,
        }}
      >
        <button
          onClick={copyToClipboard}
          style={{
            backgroundColor: theme.accentColor,
            borderRadius: theme.borderRadius,
            boxShadow: theme.glowEffect,
          }}
          className="px-6 py-2 text-white hover:opacity-80 transition-colors flex items-center gap-2 font-medium"
        >
          <Copy size={18} />
          {currentTheme === "retro" ? "> COPY" : currentTheme === "cyberpunk" ? "COPY_" : "Copy to Clipboard"}
        </button>
        <button
          onClick={saveToFile}
          style={{
            backgroundColor: theme.buttonBg,
            color: theme.textColor,
            borderRadius: theme.borderRadius,
            border: `1px ${theme.borderStyle} ${theme.toolbarBorder} `,
          }}
          className="px-6 py-2 hover:opacity-80 transition-colors flex items-center gap-2"
        >
          <Save size={18} />
          {currentTheme === "retro" ? "> SAVE" : currentTheme === "cyberpunk" ? "SAVE_" : "Save"}
        </button>
        {/* CLI Upload Button */}
        <button
          onClick={uploadToCLI}
          disabled={isUploading}
          style={{
            backgroundColor: "#10b981",
            borderRadius: theme.borderRadius,
            opacity: isUploading ? 0.5 : 1,
          }}
          className="px-6 py-2 text-white hover:opacity-80 transition-colors flex items-center gap-2 font-medium"
          title="Upload to DEV Server and copy path"
        >
          <Upload size={18} />
          {isUploading ? "Uploading..." : (currentTheme === "retro" ? "> CLI" : currentTheme === "cyberpunk" ? "CLI_" : "📤 CLI")}
        </button>
        <button
          onClick={async () => {
            await invoke("open_history_panel");
          }}
          style={{
            backgroundColor: theme.buttonBg,
            color: theme.textColor,
            borderRadius: theme.borderRadius,
            border: `1px ${theme.borderStyle} ${theme.toolbarBorder} `,
          }}
          className="px-6 py-2 hover:opacity-80 transition-colors flex items-center gap-2"
        >
          <LayoutGrid size={18} />
          {currentTheme === "retro" ? "> HISTORY" : currentTheme === "cyberpunk" ? "HIST_" : "History"}
        </button>
        <button
          onClick={closeWindow}
          style={{
            backgroundColor: "#4a4a4a",
            borderRadius: theme.borderRadius,
          }}
          className="px-6 py-2 text-white hover:bg-gray-500 transition-colors flex items-center gap-2"
        >
          <X size={18} />
          {currentTheme === "retro" ? "> EXIT" : currentTheme === "cyberpunk" ? "EXIT_" : "Cancel"}
        </button>
      </div>
      {/* Toast Notification */}
      <div
        className={`fixed bottom-8 left-1/2 transform -translate-x-1/2 px-6 py-3 rounded-xl shadow-2xl flex items-center gap-3 transition-all duration-300 z-50 ${toast.visible ? 'opacity-100 translate-y-0' : 'opacity-0 translate-y-4 pointer-events-none'
          } ${toast.type === 'success' ? 'bg-green-500 text-white' :
            toast.type === 'error' ? 'bg-red-500 text-white' :
              'bg-slate-800 text-white'
          }`}
      >
        {toast.type === 'success' && <Sparkles size={20} />}
        {toast.type === 'error' && <X size={20} />}
        <span className="font-medium">{toast.message}</span>
      </div>
    </div>
  );
}
