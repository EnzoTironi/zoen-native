/** The sign-in document uses provider popups; message documents use the MLS SAB bridge. */
function webIsolationHeaders(requestUrl) {
  let pathname = '/';
  try {
    pathname = new URL(requestUrl, 'http://localhost').pathname;
  } catch {}
  const signIn = ['/sign-in', '/sign-in/', '/sign-in.html'].includes(pathname);
  return {
    'Cross-Origin-Opener-Policy': signIn ? 'same-origin-allow-popups' : 'same-origin',
    'Cross-Origin-Embedder-Policy': signIn ? 'unsafe-none' : 'require-corp',
  };
}

module.exports = { webIsolationHeaders };
