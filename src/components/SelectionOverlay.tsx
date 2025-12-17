import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface Selection {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
}

interface PendingCapture {
  image_data: string;
  width: number;
  height: number;
}

export default function SelectionOverlay() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [isSelecting, setIsSelecting] = useState(false);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [capturedImage, setCapturedImage] = useState<string | null>(null);
  const [imageSize, setImageSize] = useState({ width: 0, height: 0 });
  // Store drawing layout for aspect-ratio preserving display
  const [drawLayout, setDrawLayout] = useState({
    offsetX: 0,
    offsetY: 0,
    drawWidth: 0,
    drawHeight: 0,
  });

  useEffect(() => {
    loadCapturedImage();

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        closeWindow();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  useEffect(() => {
    if (capturedImage && canvasRef.current) {
      drawCanvas();
    }
  }, [capturedImage, selection, isSelecting]);

  const loadCapturedImage = async () => {
    try {
      const pending = await invoke<PendingCapture | null>("get_pending_capture");
      if (pending) {
        setCapturedImage(pending.image_data);
        setImageSize({ width: pending.width, height: pending.height });
      }
    } catch (err) {
      console.error("Failed to load captured image:", err);
    }
  };

  const drawCanvas = () => {
    const canvas = canvasRef.current;
    if (!canvas || !capturedImage) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const img = new Image();
    img.onload = () => {
      // Set canvas size to window size
      canvas.width = window.innerWidth;
      canvas.height = window.innerHeight;

      // Calculate aspect-ratio preserving dimensions
      const imgAspect = img.width / img.height;
      const canvasAspect = canvas.width / canvas.height;

      let drawWidth, drawHeight, offsetX, offsetY;

      if (imgAspect > canvasAspect) {
        // Image is wider than canvas - fit to width
        drawWidth = canvas.width;
        drawHeight = canvas.width / imgAspect;
        offsetX = 0;
        offsetY = (canvas.height - drawHeight) / 2;
      } else {
        // Image is taller than canvas - fit to height
        drawHeight = canvas.height;
        drawWidth = canvas.height * imgAspect;
        offsetX = (canvas.width - drawWidth) / 2;
        offsetY = 0;
      }

      // Store layout for mouse calculations
      setDrawLayout({ offsetX, offsetY, drawWidth, drawHeight });

      // Fill background with black
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, canvas.width, canvas.height);

      // Draw the captured image with preserved aspect ratio
      ctx.drawImage(img, offsetX, offsetY, drawWidth, drawHeight);

      // Draw dark overlay
      ctx.fillStyle = "rgba(0, 0, 0, 0.5)";
      ctx.fillRect(0, 0, canvas.width, canvas.height);

      // If selecting, clear the selection area to show the image
      if (selection) {
        const x = Math.min(selection.startX, selection.endX);
        const y = Math.min(selection.startY, selection.endY);
        const width = Math.abs(selection.endX - selection.startX);
        const height = Math.abs(selection.endY - selection.startY);

        if (width > 0 && height > 0) {
          // Calculate scale based on actual drawn dimensions
          const scaleX = imageSize.width / drawWidth;
          const scaleY = imageSize.height / drawHeight;

          // Convert screen coordinates to image coordinates
          const imgX = (x - offsetX) * scaleX;
          const imgY = (y - offsetY) * scaleY;
          const imgW = width * scaleX;
          const imgH = height * scaleY;

          // Clear the selection area
          ctx.clearRect(x, y, width, height);
          // Redraw the image in that area
          ctx.drawImage(
            img,
            Math.max(0, imgX),
            Math.max(0, imgY),
            imgW,
            imgH,
            x,
            y,
            width,
            height
          );

          // Draw selection border
          ctx.strokeStyle = "#e94560";
          ctx.lineWidth = 2;
          ctx.setLineDash([5, 5]);
          ctx.strokeRect(x, y, width, height);

          // Draw dimensions (actual image pixels)
          ctx.setLineDash([]);
          ctx.fillStyle = "#e94560";
          ctx.font = "14px sans-serif";
          const dimText = `${Math.round(imgW)} × ${Math.round(imgH)}`;
          const textWidth = ctx.measureText(dimText).width;
          ctx.fillRect(x, y - 24, textWidth + 16, 22);
          ctx.fillStyle = "#fff";
          ctx.fillText(dimText, x + 8, y - 8);
        }
      }

      // Draw instructions
      ctx.fillStyle = "rgba(0, 0, 0, 0.7)";
      ctx.fillRect(canvas.width / 2 - 150, 20, 300, 30);
      ctx.fillStyle = "#fff";
      ctx.font = "14px sans-serif";
      ctx.textAlign = "center";
      ctx.fillText("Click and drag to select region • ESC to cancel", canvas.width / 2, 40);
      ctx.textAlign = "start";
    };
    img.src = `data:image/png;base64,${capturedImage}`;
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    setIsSelecting(true);
    setSelection({
      startX: e.clientX,
      startY: e.clientY,
      endX: e.clientX,
      endY: e.clientY,
    });
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!isSelecting || !selection) return;
    setSelection({
      ...selection,
      endX: e.clientX,
      endY: e.clientY,
    });
  };

  const handleMouseUp = async () => {
    if (!isSelecting || !selection || !capturedImage) return;
    setIsSelecting(false);

    const canvas = canvasRef.current;
    if (!canvas || drawLayout.drawWidth === 0) return;

    // Calculate the actual image coordinates using aspect-ratio preserving layout
    const scaleX = imageSize.width / drawLayout.drawWidth;
    const scaleY = imageSize.height / drawLayout.drawHeight;

    const screenX = Math.min(selection.startX, selection.endX);
    const screenY = Math.min(selection.startY, selection.endY);
    const screenWidth = Math.abs(selection.endX - selection.startX);
    const screenHeight = Math.abs(selection.endY - selection.startY);

    // Convert screen coordinates to image coordinates (accounting for offset)
    const x = Math.round(Math.max(0, (screenX - drawLayout.offsetX) * scaleX));
    const y = Math.round(Math.max(0, (screenY - drawLayout.offsetY) * scaleY));
    const width = Math.round(screenWidth * scaleX);
    const height = Math.round(screenHeight * scaleY);

    // Minimum selection size
    if (width < 10 || height < 10) {
      setSelection(null);
      return;
    }

    try {
      // Crop the region
      const result = await invoke<{ image_data: string; width: number; height: number }>(
        "capture_region",
        {
          x,
          y,
          width,
          height,
          sourceImage: capturedImage,
        }
      );

      // Open editor with cropped image
      await invoke("open_editor_with_image", {
        imageData: result.image_data,
        width: result.width,
        height: result.height,
      });

      // Close selection window
      await closeWindow();
    } catch (err) {
      console.error("Failed to crop region:", err);
    }
  };

  const closeWindow = async () => {
    try {
      const window = getCurrentWindow();
      await window.close();
    } catch (err) {
      console.error("Failed to close window:", err);
    }
  };

  // Show black screen while loading to prevent white flash
  if (!capturedImage) {
    return <div className="selection-overlay no-select" style={{ backgroundColor: "#000" }} />;
  }

  return (
    <div className="selection-overlay no-select" style={{ backgroundColor: "#000" }}>
      <canvas
        ref={canvasRef}
        className="w-full h-full"
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
      />
    </div>
  );
}
