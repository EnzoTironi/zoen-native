import { useLayoutEffect, useRef, type ReactNode } from 'react';

type LineRect = Pick<DOMRect, 'left' | 'right' | 'top' | 'bottom' | 'width' | 'height'>;
/** A reservation may have a smaller height than the text on the same baseline. */
export function nativeLineWidth(rects: readonly LineRect[]): number {
  const lines: { left: number; right: number; top: number; bottom: number }[] = [];
  for (const rect of rects) {
    if (rect.width <= 0 || rect.height <= 0) continue;
    const line = lines.find(line => rect.top < line.bottom && rect.bottom > line.top);
    if (line) { line.left = Math.min(line.left, rect.left); line.right = Math.max(line.right, rect.right); }
    else lines.push({ left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom });
  }
  return Math.max(0, ...lines.map(line => line.right - line.left));
}

/** SpaceView.BubbleText reserves indivisible inline room and overlays the receipt.
 * CSS fit-content alone retains the full proposed width after wrapping; SwiftUI
 * shrinks to the longest resulting line. Measure real glyphs, not message-specific
 * character counts, so both layouts preserve that intrinsic wrapping behavior.
 */
export function NativeBubbleText({ text, meta }: { text: string; meta: ReactNode }) {
  const textNode = useRef<HTMLSpanElement>(null);
  const reservation = useRef<HTMLSpanElement>(null);
  const metaNode = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    const textElement = textNode.current, reserved = reservation.current, metadata = metaNode.current;
    const bubble = textElement?.parentElement, row = bubble?.closest<HTMLElement>('.zm-row');
    if (!textElement || !reserved || !metadata || !bubble || !row || !document.createRange().getClientRects) return;
    let disposed = false, lastWidth = 0;
    const measure = () => {
      if (disposed) return;
      const rowStyle = getComputedStyle(row), bubbleStyle = getComputedStyle(bubble);
      const available = row.clientWidth - parseFloat(rowStyle.paddingLeft) - parseFloat(rowStyle.paddingRight);
      if (available <= 0) return;
      reserved.style.width = `${metadata.getBoundingClientRect().width + 6}px`;
      reserved.style.height = `${Math.max(1, metadata.getBoundingClientRect().height)}px`;
      bubble.style.width = `${available}px`;
      const range = document.createRange();
      range.selectNodeContents(textElement);
      const padding = parseFloat(bubbleStyle.paddingLeft) + parseFloat(bubbleStyle.paddingRight);
      const lineWidth = nativeLineWidth(Array.from(range.getClientRects()));
      const quoteWidth = bubble.querySelector<HTMLElement>('.zm-quote')?.scrollWidth ?? 0;
      if (lineWidth > 0) bubble.style.width = `${Math.min(available, Math.ceil(Math.max(lineWidth, quoteWidth) + padding))}px`;
      lastWidth = available;
    };
    measure();
    void document.fonts?.ready.then(measure);
    const resized = () => {
      const style = getComputedStyle(row);
      const width = row.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      if (Math.abs(width - lastWidth) > .5) measure();
    };
    const observer = typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(resized);
    observer?.observe(row);
    window.addEventListener('resize', resized);
    return () => { disposed = true; observer?.disconnect(); window.removeEventListener('resize', resized); };
  }, [text, meta]);
  return <><span className="zm-text" ref={textNode}>{text}{' '}<span ref={reservation} className="zm-meta-reservation" aria-hidden="true"/></span><span ref={metaNode} className="zm-meta zm-reserved-meta">{meta}</span></>;
}
