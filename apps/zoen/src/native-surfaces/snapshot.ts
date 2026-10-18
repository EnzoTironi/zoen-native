import { t } from '../i18n/index.ts';
import type { NativeSurfaceItemDto } from '../core/generated/NativeSurfaceItemDto';

export type SurfaceOwner = Readonly<{ identityId: string; deviceId: string }>;
export type SnapshotItem = Omit<NativeSurfaceItemDto, 'title' | 'app'> & {
  app: NativeSurfaceItemDto['app'] | null;
};
export type MiniappSnapshot = Readonly<{
  id: string;
  appId: string;
  template: 'stat' | 'progress' | 'countdown' | 'list' | 'caption' | 'ticket' | 'photo';
  title: string;
  eyebrow?: string;
  value?: string;
  detail?: string;
  bars: ReadonlyArray<Readonly<{ label: string; value: number }>>;
  rows: ReadonlyArray<Readonly<{ text: string; done: boolean }>>;
  actions: ReadonlyArray<Readonly<{ tool: string; label: string }>>;
  targetMs?: number;
  accentHex: string;
  symbol: string;
  art?: string;
  photo?: string;
  codes: readonly string[];
  places: readonly string[];
  times: readonly string[];
  sensitive: boolean;
  deepLink: string;
}>;
export type NativeWidgetContent = Readonly<{
  redacted: boolean;
  title: string;
  eyebrow?: string;
  value?: string;
  detail?: string;
  bars: MiniappSnapshot['bars'];
  rows: MiniappSnapshot['rows'];
  targetMs?: number;
  accentHex: string;
  symbol: string;
  deepLink?: string;
}>;
const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
export const surfaceId = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
export function validSurfaceOwner(value: SurfaceOwner): boolean {
  return [value.identityId, value.deviceId].every(
    (id) => typeof id === 'string' && /^[A-Za-z0-9:_-]{1,256}$/.test(id),
  );
}
export function sameSurfaceOwner(a: SurfaceOwner | null, b: SurfaceOwner): boolean {
  return !!a && a.identityId === b.identityId && a.deviceId === b.deviceId;
}
function text(value: unknown, maximum = 48): string | undefined {
  if (
    typeof value !== 'string' ||
    /(?:[a-z][a-z0-9+.-]*:\/\/|(?:https?|data|javascript|file|content|blob):|www\.)/i.test(value)
  )
    return undefined;
  const clean = [...value]
    .filter((char) => {
      const code = char.charCodeAt(0);
      return code >= 32 && code !== 127;
    })
    .slice(0, maximum)
    .join('')
    .trim();
  return clean || undefined;
}
function strings(value: unknown, maximum: number): readonly string[] {
  return Array.isArray(value)
    ? Object.freeze(value.slice(0, 2).flatMap((entry) => text(entry, maximum) ?? []))
    : Object.freeze([]);
}

/** Decode a content snapshot, never a command. IDs are checked against the actual signed Item. */
export function miniappSnapshot(item: SnapshotItem): MiniappSnapshot | null {
  const json = item.app?.snapshot_json;
  if (!json || json.length > 16_384 || !surfaceId(item.id) || !surfaceId(item.space_id))
    return null;
  if (!Number.isSafeInteger(item.version) || item.version < 0) return null;
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch {
    return null;
  }
  if (!record(value)) return null;
  const title = text(value.title);
  if (
    value.id !== item.id ||
    value.appId !== item.app?.app_id ||
    value.deepLink !== `zoen://app/${item.id}` ||
    !['stat', 'progress', 'countdown', 'list', 'caption', 'ticket', 'photo'].includes(
      String(value.template),
    ) ||
    !title ||
    typeof value.accentHex !== 'string' ||
    !/^#[0-9a-f]{6}$/i.test(value.accentHex) ||
    typeof value.symbol !== 'string' ||
    !/^[a-z0-9.]{1,40}$/.test(value.symbol)
  )
    return null;
  const bars = Array.isArray(value.bars)
    ? value.bars.slice(0, 3).flatMap((bar) => {
        if (!record(bar)) return [];
        const label = text(bar.label, 16);
        return label && typeof bar.value === 'number' && Number.isFinite(bar.value)
          ? [Object.freeze({ label, value: Math.max(0, Math.min(1, bar.value)) })]
          : [];
      })
    : [];
  const rows = Array.isArray(value.rows)
    ? value.rows.slice(0, 4).flatMap((row) => {
        if (!record(row)) return [];
        const label = text(row.text);
        return label && typeof row.done === 'boolean'
          ? [Object.freeze({ text: label, done: row.done })]
          : [];
      })
    : [];
  const actions = Array.isArray(value.actions)
    ? value.actions.slice(0, 2).flatMap((action) => {
        if (!record(action)) return [];
        const label = text(action.label, 14);
        return label && typeof action.tool === 'string' && /^[a-z_]{1,32}$/.test(action.tool)
          ? [Object.freeze({ tool: action.tool, label })]
          : [];
      })
    : [];
  const codes = strings(value.codes, 5);
  const photo = ['hike-tomales', 'hike-steep', 'hike-lands'].includes(String(value.photo))
    ? String(value.photo)
    : undefined;
  if ((value.template === 'ticket' && codes.length !== 2) || (value.template === 'photo' && !photo))
    return null;
  const art = [
    'pet',
    'pet.asleep',
    'pet.gone',
    'globe',
    'pot',
    'ballot',
    'notepad',
    'trip',
    'hike',
  ].includes(String(value.art))
    ? String(value.art)
    : undefined;
  const targetMs =
    typeof value.targetMs === 'number' &&
    Number.isSafeInteger(value.targetMs) &&
    value.targetMs >= 0 &&
    value.targetMs <= 8_640_000_000_000_000
      ? value.targetMs
      : undefined;
  if (value.template === 'countdown' && targetMs === undefined) return null;
  return Object.freeze({
    id: item.id,
    appId: item.app!.app_id,
    template: value.template as MiniappSnapshot['template'],
    title,
    eyebrow: text(value.eyebrow),
    value: text(value.value),
    detail: text(value.detail),
    bars: Object.freeze(bars),
    rows: Object.freeze(rows),
    actions: Object.freeze(actions),
    targetMs,
    accentHex: value.accentHex,
    symbol: value.symbol,
    art,
    photo,
    codes,
    places: strings(value.places, 20),
    times: strings(value.times, 8),
    sensitive: value.sensitive !== false,
    deepLink: value.deepLink,
  });
}

/** Only this compact projection may cross into the shared widget container. No app state/keys. */
export function widgetProjection(
  item: SnapshotItem,
  owner: SurfaceOwner,
  showContent = false,
): NativeWidgetContent | null {
  if (!validSurfaceOwner(owner)) return null;
  const snapshot = miniappSnapshot(item);
  if (!snapshot) return null;
  const content: NativeWidgetContent = showContent
    ? {
        redacted: false,
        title: snapshot.title,
        eyebrow: snapshot.eyebrow,
        value: snapshot.value,
        detail: snapshot.detail,
        bars: snapshot.bars,
        rows: snapshot.rows,
        targetMs: snapshot.targetMs,
        accentHex: snapshot.accentHex,
        symbol: snapshot.symbol,
        deepLink: snapshot.deepLink,
      }
    : {
        redacted: true,
        title: 'Zoen',
        detail: t('nativeSurfaces.openToSeeYourMiniapp'),
        bars: Object.freeze([]),
        rows: Object.freeze([]),
        accentHex: '#5B8DEF',
        symbol: 'lock.fill',
        deepLink: snapshot.deepLink,
      };
  return Object.freeze(content);
}
