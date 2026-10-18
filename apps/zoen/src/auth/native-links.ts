/** Navigation only: credentials are handled exclusively by the native provider SDK. */
export function nativeProviderReturnPath(
  value: string,
  googleIosClientId: string | undefined,
): string | null {
  if (
    !googleIosClientId ||
    !/^\d+-[a-z0-9]+\.apps\.googleusercontent\.com$/.test(googleIosClientId)
  )
    return null;
  try {
    const url = new URL(value);
    const suffix = '.apps.googleusercontent.com';
    const scheme = `com.googleusercontent.apps.${googleIosClientId.slice(0, -suffix.length)}:`;
    return url.protocol === scheme && !url.host && url.pathname === '/oauth2callback' ? '/' : null;
  } catch {
    return null;
  }
}
