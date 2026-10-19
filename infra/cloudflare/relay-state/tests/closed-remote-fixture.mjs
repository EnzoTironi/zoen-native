// Temporary authenticated preview only. The runner replaces this exact null marker
// with immutable synthetic frame/session/directory tuples from ONE native fixture.
const ALLOWED = null;

function refusal(status = 403) { return new Response('Refused', { status }); }
function rustError(error) {
  try {
    const value = JSON.parse(error.message ?? String(error));
    if (typeof value.code === 'string' && typeof value.reason === 'string' && typeof value.permanent === 'boolean')
      return { code: value.code, reason: value.reason, permanent: value.permanent };
  } catch {}
  return { code: 'remote_failure', reason: 'Private service invocation failed.', permanent: false };
}

export default {
  async fetch(request, env) {
    if (ALLOWED === null || request.method !== 'POST') return refusal(404);
    const path = new URL(request.url).pathname;
    if (!['/publish', '/snapshot', '/read', '/surface'].includes(path)) return refusal(404);
    const text = await request.text();
    if (text.length > 128) return refusal();
    let input;
    try { input = JSON.parse(text); } catch { return refusal(); }
    if (input === null || typeof input !== 'object' || Array.isArray(input)) return refusal();
    const keys = Object.keys(input);
    if (path === '/publish') {
      if (keys.length !== 1 || keys[0] !== 'index' || !Number.isSafeInteger(input.index)
        || input.index < 0 || input.index >= ALLOWED.frames.length) return refusal();
    } else if (keys.length !== 0) return refusal();

    try {
      let result;
      if (path === '/publish') {
        const tuple = ALLOWED.frames[input.index];
        result = await env.RELAY_CLOSED.publish({
          frame: Uint8Array.from(atob(tuple.frame_base64), char => char.charCodeAt(0)),
          session: tuple.session, target: tuple.target,
        });
      } else if (path === '/snapshot') {
        result = await env.RELAY_CLOSED.snapshot({ spaceId: ALLOWED.space_id });
      } else if (path === '/read') {
        result = await env.RELAY_CLOSED.read({ spaceId: ALLOWED.space_id, from: '0', limit: 8 });
      } else {
        result = [];
        for (const route of ['/', '/healthz', '/readyz', '/v1/sync', '/v1/auth/challenge', '/append', '/publish']) {
          const response = await env.RELAY_CLOSED.fetch(new Request(`https://private.test${route}`, { method: 'POST', body: '{}' }));
          result.push({ route, status: response.status });
        }
      }
      return Response.json({ ok: true, result });
    } catch (error) {
      return Response.json({ ok: false, error: rustError(error) }, { status: 400 });
    }
  },
};
