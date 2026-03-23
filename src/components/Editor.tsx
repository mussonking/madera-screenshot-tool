import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";
import * as fabric from "fabric";
import { THEMES, ThemeName, loadThemeFromStore } from "../utils/theme";
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

// Drawing color palette (theme-independent)
const DRAWING_COLORS = ["#e94560", "#ff6b35", "#f7c948", "#4ade80", "#22d3ee", "#3b82f6", "#a855f7", "#ffffff", "#000000"];

const STROKE_WIDTHS = [1, 2, 4, 6, 8, 12];

const CANVAS_PADDING = 0;

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
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [imageSize, setImageSize] = useState({ width: 0, height: 0 });
  const [baseCanvasSize, setBaseCanvasSize] = useState({ width: 0, height: 0 });
  const [zoom, setZoom] = useState(1);
  const [isUploading, setIsUploading] = useState(false);
  const [toast, setToast] = useState<Toast>({ message: '', type: 'info', visible: false });

  const showNotification = (message: string, type: 'success' | 'error' | 'info' = 'info') => {
    setToast({ message, type, visible: true });
    // Errors stay longer (8s) than success/info (3s) for better readability
    const duration = type === 'error' ? 8000 : 3000;
    setTimeout(() => setToast(prev => ({ ...prev, visible: false })), duration);
  };

  const theme = THEMES[currentTheme];
  const COLORS = DRAWING_COLORS;

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

  // Initialize canvas - wait for theme to load first so layout is stable
  useEffect(() => {
    if (!themeLoaded) return;
    loadImage();

    return () => {
      if (fabricRef.current) {
        fabricRef.current.dispose();
      }
    };
  }, [themeLoaded]);

  // Re-fit canvas when container resizes
  useEffect(() => {
    if (!containerRef.current) return;
    const observer = new ResizeObserver(() => {
      if (!fabricRef.current || !containerRef.current || imageSize.width === 0) return;
      const cw = containerRef.current.clientWidth;
      const ch = containerRef.current.clientHeight;
      if (cw < 100 || ch < 100) return;

      const imgW = imageSize.width;
      const imgH = imageSize.height;
      let newW = imgW;
      let newH = imgH;

      if (imgW > cw || imgH > ch) {
        const ratio = Math.min(cw / imgW, ch / imgH);
        newW = Math.floor(imgW * ratio);
        newH = Math.floor(imgH * ratio);
      }

      setBaseCanvasSize({ width: newW, height: newH });
      const canvas = fabricRef.current;
      canvas.setDimensions({ width: newW, height: newH });

      // Re-scale background image
      const bg = canvas.backgroundImage;
      if (bg) {
        bg.set({
          left: 0,
          top: 0,
          scaleX: newW / imgW,
          scaleY: newH / imgH,
        });
      }
      canvas.renderAll();
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, [imageSize]);

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

      // Wait for DOM layout to be ready (requestAnimationFrame ensures paint)
      const waitForLayout = () => {
        const container = containerRef.current;
        if (container && container.clientWidth > 100 && container.clientHeight > 100) {
          initCanvas(pending.image_data, pending.width, pending.height);
        } else {
          requestAnimationFrame(waitForLayout);
        }
      };
      requestAnimationFrame(waitForLayout);
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

    const maxWidth = cw;
    const maxHeight = ch;

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
      const errorMsg = String(err);
      console.error("Upload failed:", errorMsg);

      // Parse and display SSH error messages clearly
      let displayMsg = errorMsg;
      if (errorMsg.includes("SSH upload not enabled")) {
        displayMsg = "⚠️ Upload SSH désactivé. Activez-le dans Paramètres > SSH Upload";
      } else if (errorMsg.includes("SSH host not configured")) {
        displayMsg = "⚠️ Serveur SSH non configuré. Entrez votre hôte SSH dans Paramètres";
      } else if (errorMsg.includes("SSH remote path not configured")) {
        displayMsg = "⚠️ Chemin distant SSH non configuré. Entrez le chemin dans Paramètres";
      } else if (errorMsg.includes("Authentication failed") || errorMsg.includes("AuthFailed")) {
        displayMsg = "❌ Authentification SSH échouée. Vérifiez vos clés SSH et la configuration.";
      } else if (errorMsg.includes("Cannot connect to") || errorMsg.includes("ConnectionFailed")) {
        displayMsg = `❌ Impossible de se connecter au serveur. Vérifiez l'adresse du serveur et votre connexion réseau.`;
      } else if (errorMsg.includes("scp not found")) {
        displayMsg = "❌ scp n'est pas installé. Installez OpenSSH sur votre système.";
      } else if (errorMsg.includes("SSH upload failed")) {
        displayMsg = `❌ Erreur SSH: ${errorMsg}`;
      } else if (errorMsg.includes("Base64 decode failed")) {
        displayMsg = "❌ Erreur interne: impossible de traiter l'image";
      }

      showNotification(displayMsg, 'error');
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
      }}
      className="p-2 transition-all hover:opacity-80"
      title={`${toolName} (${shortcut})`}
    >
      <Icon size={20} />
    </button>
  );


  // Show loading state while theme loads
  if (!themeLoaded) {
    return <div className="h-full" style={{ backgroundColor: "#1a1a2e" }} />;
  }

  return (
    <div
      className="h-screen w-screen flex flex-col overflow-hidden"
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

      </div>

      {/* Canvas Container */}
      <div ref={containerRef} className="flex-1 flex items-center justify-center overflow-hidden">
        <canvas ref={canvasRef} className="shadow-2xl" />
      </div>

      {/* Bottom Action Bar */}
      <div
        className="p-2 flex items-center justify-center gap-4"
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
          }}
          className="px-6 py-2 text-white hover:opacity-80 transition-colors flex items-center gap-2 font-medium"
        >
          <Copy size={18} />
          Copy to Clipboard
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
          Save
        </button>
        {/* SSH Upload Button */}
        <button
          onClick={uploadToCLI}
          disabled={isUploading}
          style={{
            backgroundColor: "#10b981",
            borderRadius: theme.borderRadius,
            opacity: isUploading ? 0.5 : 1,
          }}
          className="px-6 py-2 text-white hover:opacity-80 transition-colors flex items-center gap-2 font-medium"
          title="Upload to SSH server and copy path"
        >
          <Upload size={18} />
          {isUploading ? "Uploading..." : "SSH"}
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
          History
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
          Cancel
        </button>
      </div>
      {/* Toast Notification */}
      <div
        className={`fixed bottom-8 left-1/2 transform -translate-x-1/2 px-6 py-4 rounded-xl shadow-2xl flex items-start gap-3 transition-all duration-300 z-50 max-w-md ${toast.visible ? 'opacity-100 translate-y-0' : 'opacity-0 translate-y-4 pointer-events-none'
          } ${toast.type === 'success' ? 'bg-green-500 text-white' :
            toast.type === 'error' ? 'bg-red-600 text-white border-2 border-red-700' :
              'bg-slate-800 text-white'
          }`}
      >
        {toast.type === 'success' && <Sparkles size={20} className="flex-shrink-0 mt-0.5" />}
        {toast.type === 'error' && <X size={20} className="flex-shrink-0 mt-0.5" />}
        <span className="font-medium leading-snug text-sm">{toast.message}</span>
      </div>
    </div>
  );
}
