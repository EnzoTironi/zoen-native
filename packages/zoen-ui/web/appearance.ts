export type Appearance = 'system' | 'light' | 'dark';
/** The production preference and parity launch use the same appearance control. */
export function applyAppearance(value: Appearance) {
  if (value === 'system') delete document.documentElement.dataset.appearance;
  else document.documentElement.dataset.appearance = value;
  document.documentElement.style.colorScheme = value === 'system' ? 'light dark' : value;
}
