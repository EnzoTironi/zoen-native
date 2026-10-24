import { useCallback, useEffect, useRef, useState } from "react";
import { exportToSvg } from "@excalidraw/excalidraw";
import morphdom from "morphdom";
import { initPencilAudio, playStroke } from "./pencil-audio";
import { captureInitialElements } from "./edit-context";
import { parsePartialElements, excludeIncompleteLastItem, convertRawElements, fixViewBox4x3, extractViewportAndElements, computeSceneBounds, sceneToSvgViewBox, EXPORT_PADDING, type ViewportRect } from "./mcp-diagram-scene";

const LERP_SPEED = 0.03; // 0–1, higher = faster snap
export function DiagramView({ toolInput, isFinal, displayMode, onElements, editedElements, onViewport, loadCheckpoint }: { toolInput: any; isFinal: boolean; displayMode: string; onElements?: (els: any[]) => void; editedElements?: any[]; onViewport?: (vp: ViewportRect) => void; loadCheckpoint?: (id: string) => Promise<{ elements: any[] } | null> }) {
  const svgRef = useRef<HTMLDivElement | null>(null);
  const latestRef = useRef<any[]>([]);
  const restoredRef = useRef<{ id: string; elements: any[] } | null>(null);
  const [, setCount] = useState(0);

  // Init pencil audio on first mount
  useEffect(() => { initPencilAudio(); }, []);

  // Set container height: 4:3 in inline, full viewport in fullscreen
  useEffect(() => {
    if (!svgRef.current) return;
    if (displayMode === "fullscreen") {
      svgRef.current.style.height = "100%";
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      const w = entry.contentRect.width;
      if (w > 0 && svgRef.current) {
        svgRef.current.style.height = `${Math.round(w * 3 / 4)}px`;
      }
    });
    observer.observe(svgRef.current);
    return () => observer.disconnect();
  }, [displayMode]);

  // Font preloading — ensure Virgil is loaded before first export
  const fontsReady = useRef<Promise<void> | null>(null);
  const ensureFontsLoaded = useCallback(() => {
    if (!fontsReady.current) {
      fontsReady.current = document.fonts.load('20px Excalifont').then(() => {});
    }
    return fontsReady.current;
  }, []);

  // Animated viewport in SCENE coordinates (stable across re-exports)
  const animatedVP = useRef<ViewportRect | null>(null);
  const targetVP = useRef<ViewportRect | null>(null);
  const sceneBoundsRef = useRef<{ minX: number; minY: number }>({ minX: 0, minY: 0 });
  const animFrameRef = useRef<number>(0);

  // User-controlled zoom during streaming (scale + pan offset in viewBox units)
  const zoomRef = useRef({ scale: 1, panX: 0, panY: 0 });
  const baseViewBoxRef = useRef<{ x: number; y: number; w: number; h: number } | null>(null);

  /** Apply user zoom on top of the stored base viewBox. */
  const applyZoom = useCallback(() => {
    if (!svgRef.current || !baseViewBoxRef.current) return;
    const svg = svgRef.current.querySelector("svg");
    if (!svg) return;
    const { x, y, w, h } = baseViewBoxRef.current;
    const { scale, panX, panY } = zoomRef.current;
    const zw = w / scale;
    const zh = h / scale;
    svg.setAttribute("viewBox", `${x + (w - zw) / 2 + panX} ${y + (h - zh) / 2 + panY} ${zw} ${zh}`);
  }, []);

  /** Apply current animated scene-space viewport to the SVG, then user zoom. */
  const applyViewBox = useCallback(() => {
    if (!animatedVP.current || !svgRef.current) return;
    const svg = svgRef.current.querySelector("svg");
    if (!svg) return;
    const { minX, minY } = sceneBoundsRef.current;
    const { x, y, width: w, height: h } = animatedVP.current;
    const ratio = w / h;
    const vp4x3: ViewportRect = Math.abs(ratio - 4 / 3) < 0.01 ? animatedVP.current
      : ratio > 4 / 3 ? { x, y, width: w, height: Math.round(w * 3 / 4) }
      : { x, y, width: Math.round(h * 4 / 3), height: h };
    const vb = sceneToSvgViewBox(vp4x3, minX, minY);
    baseViewBoxRef.current = { x: vb.x, y: vb.y, w: vb.w, h: vb.h };
    applyZoom();
  }, [applyZoom]);

  /** Lerp scene-space viewport toward target each frame. */
  const animateViewBox = useCallback(() => {
    if (!animatedVP.current || !targetVP.current) return;
    const a = animatedVP.current;
    const t = targetVP.current;
    a.x += (t.x - a.x) * LERP_SPEED;
    a.y += (t.y - a.y) * LERP_SPEED;
    a.width += (t.width - a.width) * LERP_SPEED;
    a.height += (t.height - a.height) * LERP_SPEED;
    applyViewBox();
    const delta = Math.abs(t.x - a.x) + Math.abs(t.y - a.y)
      + Math.abs(t.width - a.width) + Math.abs(t.height - a.height);
    if (delta > 0.5) {
      animFrameRef.current = requestAnimationFrame(animateViewBox);
    }
  }, [applyViewBox]);

  // Cleanup animation on unmount
  useEffect(() => {
    return () => { if (animFrameRef.current) cancelAnimationFrame(animFrameRef.current); };
  }, []);

  const renderSvgPreview = useCallback(async (els: any[], viewport: ViewportRect | null, baseElements?: any[]) => {
    if ((els.length === 0 && !baseElements?.length) || !svgRef.current) return;
    try {
      // Wait for Virgil font to load before computing text metrics
      await ensureFontsLoaded();

      // Convert new elements (raw → Excalidraw format)
      const convertedNew = convertRawElements(els);
      const baseReal = baseElements?.filter((el: any) => el.type !== "cameraUpdate") ?? [];
      const excalidrawEls = [...baseReal, ...convertedNew];

      // Update scene bounds from all elements
      sceneBoundsRef.current = computeSceneBounds(excalidrawEls);

      const svg = await exportToSvg({
        elements: excalidrawEls as any,
        appState: { viewBackgroundColor: "transparent", exportBackground: false } as any,
        files: null,
        exportPadding: EXPORT_PADDING,
        skipInliningFonts: true,
      });
      if (!svgRef.current) return;

      let wrapper = svgRef.current.querySelector(".svg-wrapper") as HTMLDivElement | null;
      if (!wrapper) {
        wrapper = document.createElement("div");
        wrapper.className = "svg-wrapper";
        svgRef.current.appendChild(wrapper);
      }

      // Fill the container (height set by ResizeObserver to maintain 4:3)
      svg.style.width = "100%";
      svg.style.height = "100%";
      svg.removeAttribute("width");
      svg.removeAttribute("height");

      const existing = wrapper.querySelector("svg");
      if (existing) {
        morphdom(existing, svg, { childrenOnly: false });
      } else {
        wrapper.appendChild(svg);
      }

      // Always fix SVG viewBox to 4:3, then store as base for user zoom
      const renderedSvg = wrapper.querySelector("svg");
      if (renderedSvg) {
        fixViewBox4x3(renderedSvg as SVGSVGElement);
        const vbAttr = (renderedSvg as SVGSVGElement).getAttribute("viewBox")?.split(" ").map(Number);
        if (vbAttr && vbAttr.length === 4) {
          baseViewBoxRef.current = { x: vbAttr[0], y: vbAttr[1], w: vbAttr[2], h: vbAttr[3] };
        }
      }

      // Animate viewport in scene space, convert to SVG space at apply time
      if (viewport) {
        targetVP.current = { ...viewport };
        onViewport?.(viewport);
        if (!animatedVP.current) {
          // First viewport — snap immediately
          animatedVP.current = { ...viewport };
        }
        // Re-apply immediately after morphdom to prevent flicker
        applyViewBox();
        // Start/restart animation toward new target
        if (animFrameRef.current) cancelAnimationFrame(animFrameRef.current);
        animFrameRef.current = requestAnimationFrame(animateViewBox);
      } else {
        // No explicit viewport — use default
        const defaultVP: ViewportRect = { x: 0, y: 0, width: 1024, height: 768 };
        onViewport?.(defaultVP);
        targetVP.current = defaultVP;
        if (!animatedVP.current) {
          animatedVP.current = { ...defaultVP };
        }
        applyViewBox();
        if (animFrameRef.current) cancelAnimationFrame(animFrameRef.current);
        animFrameRef.current = requestAnimationFrame(animateViewBox);
        targetVP.current = null;
        if (animFrameRef.current) cancelAnimationFrame(animFrameRef.current);
        // Apply user zoom on top of the fixed viewBox
        applyZoom();
      }
    } catch {
      // export can fail on partial/malformed elements
    }
  }, [applyViewBox, animateViewBox, applyZoom]);

  useEffect(() => {
    if (!toolInput) return;
    const raw = toolInput.elements;
    if (!raw) return;

    // Parse elements from string or array
    const str = typeof raw === "string" ? raw : JSON.stringify(raw);

    if (isFinal) {
      // Final input — parse complete JSON, render ALL elements
      const parsed = parsePartialElements(str);
      let { viewport, drawElements, restoreId, deleteIds } = extractViewportAndElements(parsed);

      // Load checkpoint base if restoring (async — from server)
      let base: any[] | undefined;
      const doFinal = async () => {
        if (restoreId && loadCheckpoint) {
          const saved = await loadCheckpoint(restoreId);
          if (saved) {
            base = saved.elements;
            // Extract camera from base as fallback
            if (!viewport) {
              const cam = base.find((el: any) => el.type === "cameraUpdate");
              if (cam) viewport = { x: cam.x, y: cam.y, width: cam.width, height: cam.height };
            }
            // Convert base with convertRawElements (handles both raw and already-converted)
            base = convertRawElements(base);
          }
          if (base && deleteIds.size > 0) {
            base = base.filter((el: any) => !deleteIds.has(el.id) && !deleteIds.has(el.containerId));
          }
        }

        latestRef.current = drawElements;
        // Convert new elements for fullscreen editor
        const convertedNew = convertRawElements(drawElements);

        // Merge base (converted) + new converted
        const allConverted = base ? [...base, ...convertedNew] : convertedNew;
        captureInitialElements(allConverted);
        // Only set elements if user hasn't edited yet (editedElements means user edits exist)
        if (!editedElements) onElements?.(allConverted);
        if (!editedElements) renderSvgPreview(drawElements, viewport, base);
      };
      doFinal();
      return;
    }

    // Partial input — drop last (potentially incomplete) element
    const parsed = parsePartialElements(str);

    // Extract restoreCheckpoint and delete before dropping last (they're small, won't be incomplete)
    let streamRestoreId: string | null = null;
    const streamDeleteIds = new Set<string>();
    for (const el of parsed) {
      if (el.type === "restoreCheckpoint") streamRestoreId = el.id;
      else if (el.type === "delete") {
        for (const id of String(el.ids ?? el.id).split(",")) streamDeleteIds.add(id.trim());
      }
    }

    const safe = excludeIncompleteLastItem(parsed);
    let { viewport, drawElements } = extractViewportAndElements(safe);

    const doStream = async () => {
      // Load checkpoint base (once per restoreId) — from server via callServerTool
      let base: any[] | undefined;
      if (streamRestoreId) {
        if (!restoredRef.current || restoredRef.current.id !== streamRestoreId) {
          if (loadCheckpoint) {
            const saved = await loadCheckpoint(streamRestoreId);
            if (saved) {
              const converted = convertRawElements(saved.elements);
              restoredRef.current = { id: streamRestoreId, elements: converted };
            }
          }
        }
        base = restoredRef.current?.elements;
        // Extract camera from base as fallback
        if (!viewport && base) {
          const cam = base.find((el: any) => el.type === "cameraUpdate");
          if (cam) viewport = { x: cam.x, y: cam.y, width: cam.width, height: cam.height };
        }
        if (base && streamDeleteIds.size > 0) {
          base = base.filter((el: any) => !streamDeleteIds.has(el.id) && !streamDeleteIds.has(el.containerId));
        }
      }

      if (drawElements.length > 0 && drawElements.length !== latestRef.current.length) {
        // Play pencil sound for each new element
        const prevCount = latestRef.current.length;
        for (let i = prevCount; i < drawElements.length; i++) {
          playStroke(drawElements[i].type ?? "rectangle");
        }
        latestRef.current = drawElements;
        setCount(drawElements.length);
        const jittered = drawElements.map((el: any) => ({ ...el, seed: Math.floor(Math.random() * 1e9) }));
        renderSvgPreview(jittered, viewport, base);
      } else if (base && base.length > 0 && latestRef.current.length === 0) {
        // First render: show restored base before new elements stream in
        renderSvgPreview([], viewport, base);
      }
    };
    doStream();
  }, [toolInput, isFinal, renderSvgPreview]);

  // Render already-converted elements directly (skip convertToExcalidrawElements)
  useEffect(() => {
    if (!editedElements || editedElements.length === 0 || !svgRef.current) return;
    (async () => {
      try {
        await ensureFontsLoaded();
        const svg = await exportToSvg({
          elements: editedElements as any,
          appState: { viewBackgroundColor: "transparent", exportBackground: false } as any,
          files: null,
          exportPadding: EXPORT_PADDING,
          skipInliningFonts: true,
        });
        if (!svgRef.current) return;
        let wrapper = svgRef.current.querySelector(".svg-wrapper") as HTMLDivElement | null;
        if (!wrapper) {
          wrapper = document.createElement("div");
          wrapper.className = "svg-wrapper";
          svgRef.current.appendChild(wrapper);
        }
        svg.style.width = "100%";
        svg.style.height = "100%";
        svg.removeAttribute("width");
        svg.removeAttribute("height");
        const existing = wrapper.querySelector("svg");
        if (existing) {
          morphdom(existing, svg, { childrenOnly: false });
        } else {
          wrapper.appendChild(svg);
        }
        const final = wrapper.querySelector("svg");
        if (final) {
          fixViewBox4x3(final as SVGSVGElement);
          const vbAttr = (final as SVGSVGElement).getAttribute("viewBox")?.split(" ").map(Number);
          if (vbAttr && vbAttr.length === 4) {
            baseViewBoxRef.current = { x: vbAttr[0], y: vbAttr[1], w: vbAttr[2], h: vbAttr[3] };
            applyZoom();
          }
        }
      } catch {}
    })();
  }, [editedElements, applyZoom]);

  // Zoom: pinch-to-zoom / Ctrl+scroll, pan when zoomed, double-click to reset
  useEffect(() => {
    const container = svgRef.current;
    if (!container) return;

    const handleWheel = (e: WheelEvent) => {
      const isZoomGesture = e.ctrlKey || e.metaKey;
      const isZoomedIn = Math.abs(zoomRef.current.scale - 1) > 0.01;

      if (!isZoomGesture && !isZoomedIn) return;
      e.preventDefault();

      const zoom = zoomRef.current;
      if (isZoomGesture) {
        const factor = e.deltaY > 0 ? 0.97 : 1.03;
        const newScale = Math.max(0.25, Math.min(8, zoom.scale * factor));
        if (baseViewBoxRef.current) {
          const rect = container.getBoundingClientRect();
          const mx = (e.clientX - rect.left) / rect.width;
          const my = (e.clientY - rect.top) / rect.height;
          const { w, h } = baseViewBoxRef.current;
          zoom.panX += w * (1 / newScale - 1 / zoom.scale) * (0.5 - mx);
          zoom.panY += h * (1 / newScale - 1 / zoom.scale) * (0.5 - my);
        }
        zoom.scale = newScale;
      } else if (baseViewBoxRef.current) {
        const { w, h } = baseViewBoxRef.current;
        zoom.panX += (e.deltaX / container.clientWidth) * (w / zoom.scale);
        zoom.panY += (e.deltaY / container.clientHeight) * (h / zoom.scale);
      }
      applyZoom();
    };

    const handleDblClick = () => {
      zoomRef.current = { scale: 1, panX: 0, panY: 0 };
      applyZoom();
    };

    container.addEventListener("wheel", handleWheel, { passive: false });
    container.addEventListener("dblclick", handleDblClick);
    return () => {
      container.removeEventListener("wheel", handleWheel);
      container.removeEventListener("dblclick", handleDblClick);
    };
  }, [applyZoom]);

  return (
    <div
      ref={svgRef}
      className="excalidraw-container"
      style={{ display: "flex", alignItems: "center", justifyContent: "center" }}
    />
  );
}

