import { t } from '../i18n/index.ts';
export const MAX_PHOTO_DATA_URL = 150_000;
export const MAX_PHOTO_SOURCE_BYTES = 32 * 1024 * 1024;
const DAY = 86_400_000;

export type PhotoPreview = {
  token: string;
  dataUrl: string;
  width: number;
  height: number;
  locationStripped: true;
};
export type BusyBlock = { startMs: number; endMs: number };
export type CapabilityLease = { item: string; signal: AbortSignal; check(): void };

/** Private, process-only previews. Authority to share stays in the host/core. */
export class PhotoVault {
  private readonly sessions = new Map<string, AbortController>();
  private readonly photos = new Map<string, { item: string; dataUrl: string; expires: number }>();
  private readonly now: () => number;

  constructor(now: () => number = Date.now) {
    this.now = now;
  }

  begin(item: string): CapabilityLease {
    if (!item || item.length > 200) throw new Error(t('miniapps.invalidMiniapp'));
    let controller = this.sessions.get(item);
    if (!controller) {
      controller = new AbortController();
      this.sessions.set(item, controller);
    }
    const current = controller;
    return {
      item,
      signal: current.signal,
      check: () => {
        if (current.signal.aborted || this.sessions.get(item) !== current)
          throw new Error(t('miniapps.thisMiniappHasClosed'));
      },
    };
  }

  put(lease: CapabilityLease, nonce: string, photo: Omit<PhotoPreview, 'token'>): PhotoPreview {
    lease.check();
    if (!/^[a-zA-Z0-9-]{24,80}$/.test(nonce)) throw new Error(t('miniapps.invalidPhotoIdentifier'));
    if (
      !photo.dataUrl.startsWith('data:image/jpeg;base64,') ||
      photo.dataUrl.length > MAX_PHOTO_DATA_URL ||
      !Number.isSafeInteger(photo.width) ||
      !Number.isSafeInteger(photo.height) ||
      photo.width < 1 ||
      photo.height < 1 ||
      photo.width > 1600 ||
      photo.height > 1600
    )
      throw new Error(t('miniapps.aPreviewOfThisPhotoCouldNotBe'));
    this.prune();
    const token = `zoen-photo:${nonce}`;
    if (this.photos.has(token)) throw new Error(t('miniapps.duplicatePhotoIdentifier'));
    // Bound memory without retaining original files or a disk photo history.
    while (this.photos.size >= 24) this.photos.delete(this.photos.keys().next().value!);
    const owned = [...this.photos].filter(([, entry]) => entry.item === lease.item);
    while (owned.length >= 12) this.photos.delete(owned.shift()![0]);
    this.photos.set(token, {
      item: lease.item,
      dataUrl: photo.dataUrl,
      expires: this.now() + 15 * 60_000,
    });
    return { ...photo, token };
  }

  resolve(token: string, item: string): string | null {
    this.prune();
    const photo = this.photos.get(token);
    return photo?.item === item ? photo.dataUrl : null;
  }

  release(item: string): void {
    this.sessions.get(item)?.abort();
    this.sessions.delete(item);
    for (const [token, photo] of this.photos) if (photo.item === item) this.photos.delete(token);
  }

  private prune(): void {
    for (const [token, photo] of this.photos)
      if (photo.expires <= this.now()) this.photos.delete(token);
  }
}

export function photoOptions(params: Record<string, unknown>): { max: number; maxSide: number } {
  const bounded = (value: unknown, fallback: number, min: number, max: number) => {
    if (value === undefined) return fallback;
    if (typeof value !== 'number' || !Number.isSafeInteger(value))
      throw new Error(t('miniapps.invalidPhotoOptions'));
    return Math.min(max, Math.max(min, value));
  };
  return { max: bounded(params.max, 4, 1, 6), maxSide: bounded(params.maxSide, 1024, 320, 1600) };
}

export function timeWindow(params: Record<string, unknown>): BusyBlock {
  const startMs = params.startMs;
  const endMs = params.endMs;
  if (
    typeof startMs !== 'number' ||
    typeof endMs !== 'number' ||
    !Number.isSafeInteger(startMs) ||
    !Number.isSafeInteger(endMs) ||
    startMs < 0 ||
    endMs <= startMs ||
    !Number.isFinite(new Date(endMs).getTime())
  )
    throw new Error(t('miniapps.chooseAValidDateRange'));
  return { startMs, endMs: Math.min(endMs, startMs + 31 * DAY) };
}

export function eventDraft(params: Record<string, unknown>) {
  const text = (key: string, max: number) => {
    const value = params[key];
    if (value === undefined) return undefined;
    if (
      typeof value !== 'string' ||
      [...value].some((char) => char.charCodeAt(0) < 32 && !['\t', '\n', '\r'].includes(char))
    )
      throw new Error(t('miniapps.theEventDataIsInvalid'));
    return [...value.trim()].slice(0, max).join('');
  };
  const title = text('title', 120);
  if (!title) throw new Error(t('miniapps.enterATitleForTheEvent'));
  const window = timeWindow(params);
  if (window.endMs !== params.endMs) throw new Error(t('miniapps.theEventCanLastUpTo31Days'));
  return {
    title,
    startDate: new Date(window.startMs),
    endDate: new Date(window.endMs),
    location: text('location', 160),
    notes: text('notes', 1000),
  };
}

/** Copies only busy intervals; no event names, calendar names, IDs or participants. */
export function busyBlocks(
  events: { startMs: number; endMs: number; allDay?: boolean; free?: boolean }[],
  range: BusyBlock,
): BusyBlock[] {
  const blocks = events
    .filter(
      (event) =>
        !event.allDay &&
        !event.free &&
        Number.isFinite(event.startMs) &&
        Number.isFinite(event.endMs),
    )
    .map((event) => ({
      startMs: Math.max(range.startMs, event.startMs),
      endMs: Math.min(range.endMs, event.endMs),
    }))
    .filter((event) => event.endMs > event.startMs)
    .sort((a, b) => a.startMs - b.startMs);
  const merged: BusyBlock[] = [];
  for (const block of blocks) {
    const last = merged.at(-1);
    if (last && block.startMs <= last.endMs) last.endMs = Math.max(last.endMs, block.endMs);
    else merged.push({ ...block });
  }
  return merged;
}

export function place(lat: number, lon: number, accuracyM: number | null, approximate: boolean) {
  if (!Number.isFinite(lat) || !Number.isFinite(lon) || Math.abs(lat) > 90 || Math.abs(lon) > 180)
    throw new Error(t('miniapps.yourLocationCouldNotBeObtained'));
  const accuracy =
    typeof accuracyM === 'number' && Number.isFinite(accuracyM) && accuracyM >= 0
      ? accuracyM
      : 3000;
  if (approximate)
    return {
      lat: Math.round(lat / 0.02) * 0.02,
      lon: Math.round(lon / 0.02) * 0.02,
      accuracyM: Math.max(3000, accuracy),
      approximate: true,
    };
  return { lat, lon, accuracyM: accuracy, approximate: false };
}

export function pickedContact(input: string): { name: string; initials: string }[] {
  const name = [...input]
    .map((char) => (char.charCodeAt(0) < 32 ? ' ' : char))
    .join('')
    .trim()
    .slice(0, 200);
  return name
    ? [
        {
          name,
          initials: name
            .split(/\s+/)
            .slice(0, 2)
            .map((part) => [...part][0])
            .join(''),
        },
      ]
    : [];
}
