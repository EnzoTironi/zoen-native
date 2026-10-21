/** Fixed presentation categories only. Never forward an exception, URL or document text. */
const reasons = [
  'document_status_failed',
  'autosave_busy',
  'autosave_failed',
  'flush_busy',
  'flush_failed',
  'layout_failed',
  'export_failed',
  'host_ui_failed',
] as const;
export type DeckcraftFeedbackReason = (typeof reasons)[number];
export function deckcraftFeedbackReason(value: unknown): DeckcraftFeedbackReason | null {
  return typeof value === 'string' && (reasons as readonly string[]).includes(value)
    ? (value as DeckcraftFeedbackReason)
    : null;
}

/** The pinned engine rejects its own Promise with this exact one-field JSON value.
 * Similar prose, Error messages and extra fields do not relax any failure handling. */
export function deckcraftInteractionRefusal(value: unknown): boolean {
  if (typeof value !== 'string' || value.length > 128) return false;
  try {
    const parsed: unknown = JSON.parse(value);
    return (
      !!parsed &&
      typeof parsed === 'object' &&
      !Array.isArray(parsed) &&
      Object.keys(parsed).length === 1 &&
      (parsed as { code?: unknown }).code === 'human_interaction'
    );
  } catch {
    return false;
  }
}
