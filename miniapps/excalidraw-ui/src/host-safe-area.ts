import "./host-touch.css";
import { applyHostCloseButton } from "../../../packages/zoen-miniapp-sdk/src/host-chrome";
import { useLayoutEffect } from "react";
import type { App } from "@modelcontextprotocol/ext-apps";

const properties = [
  ["--sat", "top"],
  ["--sar", "right"],
  ["--sab", "bottom"],
  ["--sal", "left"],
] as const;
type Styles = readonly (readonly [string, string, string])[];
const owners = new WeakMap<HTMLElement, { previous: Styles }>();

// The SDK merges partial host-context notifications before calling listeners.
// Only floating Excalidraw UI uses these variables; the canvas stays full-bleed.
export function useHostSafeArea(app: App | null) {
  useLayoutEffect(() => {
    if (!app) return;
    const root = document.documentElement;
    const managed = [
      ...properties.map(([property]) => property),
      ...["top", "right", "width", "height"].map((edge) => `--zoen-host-close-${edge}`),
    ];
    const previous =
      owners.get(root)?.previous ??
      managed.map(
        (property) =>
          [
            property,
            root.style.getPropertyValue(property),
            root.style.getPropertyPriority(property),
          ] as const,
      );
    const owner = { previous };
    owners.set(root, owner);
    const apply = () => {
      if (owners.get(root) !== owner) return;
      const context = app.getHostContext();
      applyHostCloseButton(root.style, context);
      const insets = context?.displayMode === "fullscreen" ? context.safeAreaInsets : undefined;
      for (const [property, edge] of properties) {
        const value = insets?.[edge];
        if (typeof value === "number" && Number.isFinite(value) && value >= 0)
          root.style.setProperty(property, `${value}px`);
        else root.style.removeProperty(property);
      }
    };
    app.addEventListener("hostcontextchanged", apply);
    apply();
    return () => {
      app.removeEventListener("hostcontextchanged", apply);
      if (owners.get(root) !== owner) return;
      owners.delete(root);
      for (const [property, value, priority] of previous) {
        if (value) root.style.setProperty(property, value, priority);
        else root.style.removeProperty(property);
      }
    };
  }, [app]);
}
