/** A host-owned floating close hit area, in CSS pixels from the frame's top/right. */
export type HostCloseButton = Readonly<{
  top: number;
  right: number;
  width: number;
  height: number;
}>;
const edges = ["top", "right", "width", "height"] as const;

export function hostCloseButton(context: any): HostCloseButton | null {
  if (context?.displayMode !== "fullscreen") return null;
  const value = context.zoen?.chrome?.closeButton;
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  for (const edge of edges) {
    const number = value[edge];
    if (typeof number !== "number" || !Number.isFinite(number) || number < 0) return null;
    if (edge === "width" || edge === "height") {
      if (number === 0 || number > 128) return null;
    } else if (number > 2048) return null;
  }
  const dimensions = context.containerDimensions;
  if (typeof dimensions?.width === "number" && value.right + value.width > dimensions.width)
    return null;
  if (typeof dimensions?.height === "number" && value.top + value.height > dimensions.height)
    return null;
  return { top: value.top, right: value.right, width: value.width, height: value.height };
}

/** Only component chrome consumes these variables; never inset the canvas or body with them. */
export function applyHostCloseButton(style: CSSStyleDeclaration, context: unknown) {
  const button = hostCloseButton(context);
  for (const edge of edges) {
    const key = `--zoen-host-close-${edge}`;
    if (button) style.setProperty(key, `${button[edge]}px`);
    else style.removeProperty(key);
  }
}
