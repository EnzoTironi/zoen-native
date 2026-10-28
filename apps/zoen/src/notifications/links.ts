export type ZoenLink =
  | { kind: 'item' | 'chat'; id: string }
  | { kind: 'message'; space: string; message: string }
  | { kind: 'invite'; code: string }
  | { kind: 'agent'; target: string };

const id = /^[A-Za-z0-9_-]{1,128}$/;
const invite = /^[0-9A-HJKMNP-TV-Z]{10}$/i;
const agent = /^@?[A-Za-z0-9_.-]{1,128}$/;

/** Addresses carry IDs only, never text, credentials or permission to read. */
export function messageAddress(space: string, message: string): string | null {
  return id.test(space) && id.test(message) ? `zoen://message/${space}/${message}` : null;
}

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
    let arguments_: string[];
    if (url.protocol === 'zoen:') {
      route = url.hostname;
      arguments_ = path;
    } else if (
      url.protocol === 'https:' &&
      ['tryzoen.com', 'www.tryzoen.com'].includes(url.hostname)
    ) {
      [route, ...arguments_] = path;
    } else return null;
    if (route === 'message') {
      const [space, message] = arguments_;
      return arguments_.length === 2 &&
        space &&
        message &&
        messageAddress(space, message) &&
        !url.pathname.includes('//') &&
        !url.pathname.endsWith('/')
        ? { kind: 'message', space, message }
        : null;
    }
    if (arguments_.length !== 1) return null;
    target = arguments_[0];
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

/** Expo CLI and the development client may redeliver their startup envelopes.
 * Discard the whole envelope; never open, fetch or authenticate with its inner address. */
function expoBootstrapLink(path: string): boolean {
  if (
    path.length > 2048 ||
    !/^(?:(?:zoen|exp\+zoen):\/\/\/?|exp\+zoen:\/\/expo-development-client\/?)\?/.test(path) ||
    Array.from(path).some(
      (character) => character.charCodeAt(0) <= 32 || character.charCodeAt(0) === 127,
    )
  )
    return false;
  try {
    const launch = new URL(path);
    const developmentClient =
      launch.protocol === 'exp+zoen:' && launch.host === 'expo-development-client';
    if (
      !['zoen:', 'exp+zoen:'].includes(launch.protocol) ||
      (!developmentClient && launch.host) ||
      launch.username ||
      launch.password ||
      (launch.pathname !== '' && launch.pathname !== '/') ||
      launch.hash
    )
      return false;
    const keys = [...launch.searchParams.keys()].sort().join(',');
    if (
      developmentClient
        ? keys !== 'url'
        : keys !== '__expo_url' &&
          keys !==
            '__expo_disable_auto_launch,__expo_disable_fab,__expo_disable_onboarding,__expo_url'
    )
      return false;
    for (const key of [
      '__expo_disable_auto_launch',
      '__expo_disable_fab',
      '__expo_disable_onboarding',
    ]) {
      if (launch.searchParams.has(key) && launch.searchParams.get(key) !== '1') return false;
    }
    const transport = new URL(
      launch.searchParams.get(developmentClient ? 'url' : '__expo_url') ?? '',
    );
    return (
      ['http:', 'https:'].includes(transport.protocol) &&
      !!transport.hostname &&
      !transport.username &&
      !transport.password
    );
  } catch {
    return false;
  }
}

export function nativeLinkPath(path: string): string {
  if (path.startsWith('/') && !path.startsWith('//')) return path;
  // Expo Router uses createURL('/') when a native launch has no incoming deep link.
  if (/^(?:zoen|exp\+zoen):\/\/\/?$/.test(path) || expoBootstrapLink(path)) return '/';
  const link = parseZoenLink(path);
  if (!link) return '/open-link?invalid=1';
  switch (link.kind) {
    case 'item':
      return `/item/${encodeURIComponent(link.id)}`;
    case 'chat':
      return `/chat/${encodeURIComponent(link.id)}`;
    case 'message':
      return `/message/${encodeURIComponent(link.space)}/${encodeURIComponent(link.message)}`;
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
