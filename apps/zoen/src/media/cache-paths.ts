/** SDK-returned copies may be removed only inside this app's cache root. */
export function isAppCacheCopy(uri: string, cacheRoot: string): boolean {
  try {
    const file = new URL(uri);
    const root = new URL(cacheRoot);
    if (file.protocol !== 'file:' || root.protocol !== 'file:' || file.host !== root.host)
      return false;
    if (file.search || file.hash) return false;
    const path = decodeURIComponent(file.pathname);
    const prefix = `${decodeURIComponent(root.pathname).replace(/\/+$/, '')}/`;
    return (
      path.startsWith(prefix) &&
      path.length > prefix.length &&
      !path.split('/').some((component) => component === '.' || component === '..')
    );
  } catch {
    return false;
  }
}
