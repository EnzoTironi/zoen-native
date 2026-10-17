export type ZoenLink =
  | { kind: 'item' | 'chat'; id: string }
  | { kind: 'invite'; code: string }
  | { kind: 'agent'; target: string };

const id = /^[A-Za-z0-9_-]{1,128}$/;
const invite = /^[0-9A-HJKMNP-TV-Z]{10}$/i;
const agent = /^@?[A-Za-z0-9_.-]{1,128}$/;

/** Only Zoen's own documented link shapes may enter application navigation. */
export function parseZoenLink(value: string): ZoenLink | null {
  if (
    !value ||
    value.length > 2048 ||
    Array.from(value).some(
      (character) => character.charCodeAt(0) <= 32 || character.charCodeAt(0) === 127,
    )
  )
    return null;
  try {
    if (
      value
        .split(/[?#]/, 1)[0]!
        .split('/')
        .some((part) => ['.', '..'].includes(decodeURIComponent(part)))
    )
      return null;
    const url = new URL(value);
    if (url.username || url.password || url.port || url.hash || url.search) return null;
    const path = url.pathname.split('/').filter(Boolean).map(decodeURIComponent);
    let route: string;
    let target: string | undefined;
    if (url.protocol === 'zoen:') {
      if (path.length !== 1) return null;
      route = url.hostname;
      target = path[0];
    } else if (
      url.protocol === 'https:' &&
      ['tryzoen.com', 'www.tryzoen.com'].includes(url.hostname)
    ) {
      if (path.length !== 2) return null;
      [route, target] = path;
    } else return null;
    if (!target) return null;
    if (['app', 'item'].includes(route) && id.test(target)) return { kind: 'item', id: target };
    if (route === 'chat' && id.test(target)) return { kind: 'chat', id: target };
    if (['join', 'j'].includes(route) && invite.test(target))
      return { kind: 'invite', code: target.toUpperCase() };
    if (['agent', 'a'].includes(route) && agent.test(target) && !['.', '..'].includes(target))
      return { kind: 'agent', target };
    return null;
  } catch {
    return null;
  }
}

export function nativeLinkPath(path: string, development = false): string {
  if (path.startsWith('/') && !path.startsWith('//')) return path;
  if (development) {
    try {
      const launch = new URL(path);
      const metro = new URL(launch.searchParams.get('__expo_url') ?? '');
      if (
        ['zoen:', 'exp+zoen:'].includes(launch.protocol) &&
        !launch.host &&
        ['', '/'].includes(launch.pathname) &&
        !launch.hash &&
        ['http:', 'https:'].includes(metro.protocol) &&
        ['localhost', '127.0.0.1', '[::1]'].includes(metro.hostname) &&
        !metro.username &&
        !metro.password
      )
        return '/';
    } catch {
      // Ordinary application links continue through the strict parser below.
    }
  }
  const link = parseZoenLink(path);
  if (!link) return '/open-link?invalid=1';
  switch (link.kind) {
    case 'item':
      return `/item/${encodeURIComponent(link.id)}`;
    case 'chat':
      return `/chat/${encodeURIComponent(link.id)}`;
    case 'invite':
      return `/open-link?kind=invite&target=${encodeURIComponent(link.code)}`;
    case 'agent':
      return `/open-link?kind=agent&target=${encodeURIComponent(link.target)}`;
  }
}

export function validLocalNotification(
  data: unknown,
  deviceId: string,
): { spaceId: string } | null {
  if (typeof data !== 'object' || data === null || Array.isArray(data)) return null;
  const value = data as Record<string, unknown>;
  if (
    Object.keys(value).sort().join(',') !== 'device_id,event_seq,space_id,zoen_local' ||
    value.zoen_local !== 1 ||
    value.device_id !== deviceId ||
    typeof value.space_id !== 'string' ||
    !id.test(value.space_id) ||
    typeof value.event_seq !== 'string' ||
    !/^[1-9][0-9]{0,19}$/.test(value.event_seq) ||
    BigInt(value.event_seq) > 18446744073709551615n
  )
    return null;
  return { spaceId: value.space_id };
}
