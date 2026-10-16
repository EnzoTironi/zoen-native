/** Read-only schedule content. Never instantiate a mini-app or expose internal resource identifiers. */
export function eventPreview(json: string): string {
  const lines: string[] = [];
  function visit(value: unknown, label = ''): void {
    if (value === null || value === undefined) return;
    if (Array.isArray(value)) { value.forEach((entry) => visit(entry)); return; }
    if (typeof value === 'object') {
      for (const [key, child] of Object.entries(value)) {
        if (key.startsWith('_') || /^(ids?|app|kind|version|schema|schema_version|identity|members|attendees|owner|author|device|signature|key|token)$/i.test(key) || /(?:_ids?|Ids?|ID)$/.test(key)) continue;
        const name = key.replace(/([a-z])([A-Z])/g, '$1 $2').replace(/_/g, ' ');
        if (typeof child === 'number' && /(?:at|start|end|time|date)(?: ms)?$/i.test(name)) { if (Number.isSafeInteger(child) && child >= 0 && child <= 8_640_000_000_000_000) lines.push(`${name}: ${new Date(child).toLocaleString()}`); }
        else visit(child, name);
      }
      return;
    }
    if (typeof value === 'string' && value.trim()) lines.push(label ? `${label}: ${value}` : value);
    else if (typeof value === 'number' || typeof value === 'boolean') lines.push(label ? `${label}: ${value}` : String(value));
  }
  try { visit(JSON.parse(json)); } catch { return 'Event details could not be loaded.'; }
  return lines.join('\n');
}
