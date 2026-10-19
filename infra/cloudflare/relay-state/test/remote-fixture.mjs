// Temporary authenticated Wrangler preview, never a production route. Its client
// listens on loopback and reaches the deployed private Worker by service binding.
export default {
  async fetch(request, env) {
    if (request.method !== 'POST') return new Response('Not found', { status: 404 });
    const input = await request.json();
    if (!input.spaceId?.startsWith('qa-cloudflare-state-')) return new Response('Refused', { status: 403 });
    if (input.event) input.event.bytes = new Uint8Array(input.event.bytes);
    if (input.admissionState) input.admissionState = new Uint8Array(input.admissionState);
    const operation = { '/append': 'appendAdmitted', '/read': 'read', '/snapshot': 'snapshot' }[new URL(request.url).pathname];
    if (!operation) return new Response('Not found', { status: 404 });
    const result = await env.RELAY_STATE[operation](input);
    return Response.json(result);
  },
};
