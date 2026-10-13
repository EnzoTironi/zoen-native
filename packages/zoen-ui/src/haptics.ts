export type ZoenHaptic =
  | "selection"
  | "send"
  | "record"
  | "lock"
  | "success"
  | "warning"
  | "deny"
  | "error";

const reducedMotion = (): boolean =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export function prepareZoenHaptic(_kind: ZoenHaptic): void {}

export function playZoenHaptic(kind: ZoenHaptic): void {
  if (typeof navigator.vibrate !== "function") return;
  if (kind === "send") {
    if (reducedMotion()) return;
    navigator.vibrate(10);
    return;
  }
  if (kind === "error") navigator.vibrate([20, 40, 20]);
}
