import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { rm } from 'node:fs/promises';
import {
  observeOwnedChild,
  withCleanup,
} from '../../../../miniapps/excalidraw-ui/test/browser-lifecycle.mjs';
export async function runBrowser(chrome, folder, url) {
  const child = spawn(
    chrome,
    [
      '--headless',
      '--disable-gpu',
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-background-networking',
      '--disable-component-update',
      '--disable-sync',
      '--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE localhost',
      '--remote-debugging-address=127.0.0.1',
      '--remote-debugging-port=0',
      `--user-data-dir=${join(folder, 'profile')}`,
      'about:blank',
    ],
    { stdio: ['ignore', 'ignore', 'pipe'], detached: process.platform !== 'win32' },
  );
  const owned = observeOwnedChild(child, { processGroup: process.platform !== 'win32' });
  let socket, closeBrowser, startup, hardwall;
  return withCleanup(
    () =>
      Promise.race([
        (async () => {
          const endpoint = await Promise.race([
            new Promise((resolve, reject) => {
              let stderr = '';
              child.once('error', reject);
              child.once('exit', () => reject(Error('Owned Chrome exited before DevTools')));
              child.stderr.on('data', (chunk) => {
                stderr = (stderr + chunk).slice(-16_384);
                const match = stderr.match(/DevTools listening on (ws:\/\/127\.0\.0\.1:[^\s]+)/);
                if (match) resolve(match[1]);
              });
            }),
            new Promise((_, reject) => {
              startup = setTimeout(() => reject(Error('Owned Chrome startup exceeded30s')), 30_000);
            }),
          ]);
          clearTimeout(startup);
          socket = new WebSocket(endpoint);
          await new Promise((resolve, reject) => {
            socket.addEventListener('open', resolve, { once: true });
            socket.addEventListener('error', reject, { once: true });
          });
          let sequence = 0;
          const pending = new Map();
          socket.addEventListener('message', (event) => {
            const packet = JSON.parse(String(event.data));
            if (packet.id === undefined) return;
            const call = pending.get(packet.id);
            pending.delete(packet.id);
            if (packet.error) call?.reject(Error('Owned Chrome protocol error'));
            else call?.resolve(packet.result ?? {});
          });
          const send = (method, params = {}, sessionId) =>
            new Promise((resolve, reject) => {
              const id = ++sequence;
              pending.set(id, { resolve, reject });
              socket.send(
                JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }),
              );
            });
          closeBrowser = () => {
            if (socket.readyState === WebSocket.OPEN)
              socket.send(JSON.stringify({ id: ++sequence, method: 'Browser.close' }));
          };
          const target = await send('Target.createTarget', { url: 'about:blank' });
          const { sessionId } = await send('Target.attachToTarget', {
            targetId: target.targetId,
            flatten: true,
          });
          await send('Page.enable', {}, sessionId);
          await send('Page.navigate', { url }, sessionId);
          const deadline = performance.now() + 100_000;
          while (performance.now() < deadline) {
            const result = await send(
              'Runtime.evaluate',
              { expression: 'window.cursorResult??null', returnByValue: true },
              sessionId,
            );
            if (result.result?.value)
              return { result: result.result.value, lifecycle: owned.state };
            await new Promise((resolve) => setTimeout(resolve, 100));
          }
          throw Error('Owned real browser cursor proof exceeded100s');
        })(),
        new Promise((_, reject) => {
          hardwall = setTimeout(
            () => reject(Error('Owned browser hard wall exceeded130s')),
            130_000,
          );
        }),
      ]),
    async () => {
      clearTimeout(hardwall);
      clearTimeout(startup);
      try {
        await owned.stop(closeBrowser);
      } finally {
        socket?.close();
        await rm(join(folder, 'profile'), {
          recursive: true,
          force: true,
          maxRetries: 5,
          retryDelay: 100,
        });
      }
    },
  );
}
