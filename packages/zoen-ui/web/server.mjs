import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { dirname, extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const directory = resolve(dirname(fileURLToPath(import.meta.url)), 'dist');
const port = Number(process.env.ZOEN_WEB_PORT ?? 4174);
const types = new Map([['.html', 'text/html; charset=utf-8'], ['.js', 'text/javascript; charset=utf-8'],
  ['.css', 'text/css; charset=utf-8'], ['.wasm', 'application/wasm'], ['.json', 'application/json; charset=utf-8']]);
const headers = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
  'Cross-Origin-Resource-Policy': 'same-origin',
  'Content-Security-Policy': "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self' blob:; style-src 'self'; img-src 'self' data: blob:; connect-src 'self' https: wss: http://localhost:* http://127.0.0.1:* ws://localhost:* ws://127.0.0.1:*; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
  'X-Content-Type-Options': 'nosniff',
  'Referrer-Policy': 'no-referrer',
  'Cache-Control': 'no-store',
};
const server = createServer(async (request, response) => {
  try {
    if (request.method !== 'GET' && request.method !== 'HEAD') { response.writeHead(405, headers); response.end(); return; }
    const path = decodeURIComponent(new URL(request.url ?? '/', 'http://localhost').pathname);
    const route = path === '/' || /^\/join\/[0-9A-HJKMNP-TV-Z]{10}$/i.test(path) ? '/index.html' : path;
    const file = resolve(directory, `.${route}`);
    if (!file.startsWith(directory + sep) || !(await stat(file)).isFile()) throw new Error('Not found');
    const body = await readFile(file);
    response.writeHead(200, { ...headers, 'Content-Type': types.get(extname(file)) ?? 'application/octet-stream' });
    response.end(request.method === 'HEAD' ? undefined : body);
  } catch { response.writeHead(404, headers); response.end('Not found'); }
});
server.listen(port, '127.0.0.1', () => console.info(`Authenticated Zoen web: http://127.0.0.1:${port}`));
const stop = () => server.close(() => process.exit(0));
process.on('SIGINT', stop);
process.on('SIGTERM', stop);
