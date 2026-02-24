import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface SelectionRect {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
}

function CaptureOverlay() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [selection, setSelection] = useState<SelectionRect | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [screenshotLoaded, setScreenshotLoaded] = useState(false);
  const imageRef = useRef<HTMLImageElement | null>(null);

  // Load the screenshot as background
  useEffect(() => {
    async function loadScreenshot() {
      try {
        const b64: string = await invoke("get_capture_screenshot");
        const img = new Image();
        img.onload = () => {
          imageRef.current = img;
          setScreenshotLoaded(true);
        };
        img.src = `data:image/png;base64,${b64}`;
      } catch (e) {
        console.error("Failed to load screenshot:", e);
      }
    }
    loadScreenshot();
  }, []);

  // Draw the canvas whenever selection or screenshot changes
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    canvas.width = window.innerWidth;
    canvas.height = window.innerHeight;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // Draw screenshot as background
    if (imageRef.current && screenshotLoaded) {
      ctx.drawImage(imageRef.current, 0, 0, canvas.width, canvas.height);
    }

    // Dark overlay
    ctx.fillStyle = "rgba(0, 0, 0, 0.4)";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Clear the selected region to show the screenshot underneath
    if (selection) {
      const x = Math.min(selection.startX, selection.endX);
      const y = Math.min(selection.startY, selection.endY);
      const w = Math.abs(selection.endX - selection.startX);
      const h = Math.abs(selection.endY - selection.startY);

      if (w > 0 && h > 0) {
        // Redraw the screenshot portion in the selection area
        ctx.clearRect(x, y, w, h);
        if (imageRef.current) {
          const scaleX = imageRef.current.naturalWidth / canvas.width;
          const scaleY = imageRef.current.naturalHeight / canvas.height;
          ctx.drawImage(
            imageRef.current,
            x * scaleX, y * scaleY, w * scaleX, h * scaleY,
            x, y, w, h
          );
        }

        // Selection border
        ctx.strokeStyle = "#4fc3f7";
        ctx.lineWidth = 2;
        ctx.strokeRect(x, y, w, h);

        // Size label
        ctx.fillStyle = "#4fc3f7";
        ctx.font = "12px monospace";
        ctx.fillText(`${Math.round(w)} × ${Math.round(h)}`, x, y > 20 ? y - 6 : y + h + 16);
      }
    }
  }, [selection, screenshotLoaded]);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    setIsDragging(true);
    setSelection({
      startX: e.clientX,
      startY: e.clientY,
      endX: e.clientX,
      endY: e.clientY,
    });
  }, []);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isDragging || !selection) return;
    setSelection((prev) =>
      prev ? { ...prev, endX: e.clientX, endY: e.clientY } : null
    );
  }, [isDragging, selection]);

  const handleMouseUp = useCallback(async () => {
    if (!isDragging || !selection) return;
    setIsDragging(false);

    const x = Math.min(selection.startX, selection.endX);
    const y = Math.min(selection.startY, selection.endY);
    const w = Math.abs(selection.endX - selection.startX);
    const h = Math.abs(selection.endY - selection.startY);

    // Ignore tiny selections (accidental clicks)
    if (w < 10 || h < 10) {
      setSelection(null);
      return;
    }

    // Account for device pixel ratio for the actual capture region
    const dpr = window.devicePixelRatio || 1;
    const region = {
      x: x * dpr,
      y: y * dpr,
      width: w * dpr,
      height: h * dpr,
    };

    try {
      await invoke("process_capture", {
        region,
        aiCompletion: true, // default to enabled; could read from settings
      });
    } catch (e) {
      console.error("process_capture failed:", e);
    }
  }, [isDragging, selection]);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === "Escape") {
      // Close the overlay without capturing
      window.close();
    }
  }, []);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  return (
    <canvas
      ref={canvasRef}
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        width: "100vw",
        height: "100vh",
        cursor: "crosshair",
        zIndex: 9999,
      }}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
    />
  );
}

export default CaptureOverlay;
