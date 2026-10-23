import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { build } from 'esbuild';
import {
  nativeMiniappCommand,
  nativeMiniappHtml,
  nativeMiniappMessage,
  nativeMiniappNavigation,
} from './host-transport.ts';
import { sandboxHtml } from './sandbox.ts';
import { findBrowserExecutable } from '../../../../tools/miniapp-build/browser-executable.ts';
import { annotateOwnedBrowserFailure } from '../../../../tools/miniapp-build/browser-startup.ts';
import { chromeEndpoint, type ChromeStartup } from './chrome-startup.fixtures.ts';

const nonce = 'a24582bd-0377-4fee-bb76-73d465c927c3';
const other = 'c34128af-3476-4ce2-ba34-37d459c934d6';
const message = { jsonrpc: '2.0', id: 1, method: 'ui/initialize', params: {} };

async function realBrowserResult(chrome: string, folder: string, path: string): Promise<string> {
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
      '--disable-default-apps',
      '--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE localhost',
      '--remote-debugging-address=127.0.0.1',
      '--remote-debugging-port=0',
      `--user-data-dir=${join(folder, 'profile')}`,
      'about:blank',
    ],
    { stdio: ['ignore', 'pipe', 'pipe'] },
  );
  // Register before awaiting anything: a signaled process can exit before cleanup,
  // and its stdio may still be open after 'exit'.
  const closed = new Promise<void>((resolve) => child.once('close', () => resolve()));
  let stderr = '';
  let stdout = '';
  child.stdout.on('data', (data: Buffer) => {
    stdout = (stdout + data.toString()).slice(-16_384);
  });
  const startup: ChromeStartup = { portFile: 'unobserved', checksStarted: 0, checksCompleted: 0 };
  child.stderr.on('data', (data: Buffer) => {
    stderr = (stderr + data.toString()).slice(-16_384);
  });
  const failure = (message: string, cause?: Error) =>
    new Error(
      `${message}: ${JSON.stringify({
        binary: chrome,
        pid: child.pid ?? null,
        exitCode: child.exitCode,
        signalCode: child.signalCode,
        ...(cause ? { spawnError: cause.message } : {}),
        stderr,
        stdout,
        startup,
      })}`,
    );
  let socket: WebSocket | undefined;
  let deadline: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      (async () => {
        const endpoint = await chromeEndpoint(child, join(folder, 'profile'), startup, failure);
        socket = new WebSocket(endpoint);
        const connection = socket;
        await new Promise<void>((resolve, reject) => {
          connection.addEventListener('open', () => resolve(), { once: true });
          connection.addEventListener(
            'error',
            () => reject(new Error('Owned Chrome connection failed')),
            { once: true },
          );
        });
        let nextId = 0;
        const pending = new Map<
          number,
          { resolve: (value: Record<string, unknown>) => void; reject: (error: Error) => void }
        >();
        const events = new Map<string, () => void>();
        connection.addEventListener('message', (event) => {
          const packet = JSON.parse(String(event.data)) as {
            id?: number;
            sessionId?: string;
            method?: string;
            result?: Record<string, unknown>;
            error?: { message: string };
          };
          if (packet.id !== undefined) {
            const request = pending.get(packet.id);
            pending.delete(packet.id);
            if (packet.error) request?.reject(new Error(packet.error.message));
            else request?.resolve(packet.result ?? {});
          } else if (packet.method) events.get(`${packet.sessionId}:${packet.method}`)?.();
        });
        const send = (method: string, params: Record<string, unknown> = {}, sessionId?: string) =>
          new Promise<Record<string, unknown>>((resolve, reject) => {
            const id = ++nextId;
            pending.set(id, { resolve, reject });
            connection.send(
              JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }),
            );
          });
        const target = await send('Target.createTarget', { url: 'about:blank' });
        const attached = await send('Target.attachToTarget', {
          targetId: target.targetId,
          flatten: true,
        });
        const session = String(attached.sessionId);
        await send('Page.enable', {}, session);
        const loaded = new Promise<void>((resolve) => {
          events.set(`${session}:Page.loadEventFired`, resolve);
        });
        await send('Page.navigate', { url: `file://${path}` }, session);
        await loaded;
        const evaluated = await send(
          'Runtime.evaluate',
          {
            expression: `new Promise(resolve=>{const read=()=>{const node=document.getElementById('transport-qa-result');if(node){observer.disconnect();resolve(node.textContent);}};const observer=new MutationObserver(read);observer.observe(document,{childList:true,subtree:true});read();})`,
            awaitPromise: true,
            returnByValue: true,
            timeout: 10_000,
          },
          session,
        );
        const result = evaluated.result as { value?: unknown } | undefined;
        assert.equal(typeof result?.value, 'string', JSON.stringify(evaluated));
        return result!.value as string;
      })(),
      new Promise<never>((_, reject) => {
        deadline = setTimeout(
          () => reject(failure('Owned Chrome transport check timed out')),
          20_000,
        );
      }),
    ]);
  } catch (error) {
    await annotateOwnedBrowserFailure(error, child, join(folder, 'profile'));
    throw error;
  } finally {
    clearTimeout(deadline);
    socket?.close();
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      if (child.exitCode === null && child.signalCode === null) {
        child.kill('SIGTERM');
        timer = setTimeout(() => {
          if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
        }, 2_000);
      }
      await closed;
    } finally {
      clearTimeout(timer);
    }
  }
}

test('owned browser startup failures retain executable, process and bounded stderr diagnostics', async () => {
  const folder = await mkdtemp(join(tmpdir(), 'zoen-browser-startup-'));
  try {
    await assert.rejects(realBrowserResult(process.execPath, folder, ''), (error: Error) => {
      assert.match(error.message, /Owned Chrome exited before connection/);
      const detail = JSON.parse(error.message.slice(error.message.indexOf(': ') + 2));
      assert.equal(detail.binary, process.execPath);
      assert.equal(typeof detail.pid, 'number');
      assert.notEqual(detail.exitCode, 0);
      assert.match(detail.stderr, /bad option|unknown option/);
      assert.ok(detail.stderr.length <= 16_384);
      return true;
    });
    if (process.platform !== 'win32') {
      const noisy = join(folder, 'synthetic-chrome');
      await writeFile(
        noisy,
        `#!${process.execPath}\nprocess.stdout.write('x'.repeat(20_000)+'OWNED_STDOUT_END');process.stderr.write('owned synthetic stderr');process.exit(7);\n`,
        { mode: 0o700 },
      );
      await assert.rejects(realBrowserResult(noisy, folder, ''), (error: Error) => {
        assert.match(error.message, /Owned Chrome exited before connection/);
        const detail = JSON.parse(error.message.slice(error.message.indexOf(': ') + 2));
        assert.equal(detail.exitCode, 7);
        assert.equal(detail.stdout.length, 16_384);
        assert.ok(detail.stdout.endsWith('OWNED_STDOUT_END'));
        assert.equal(detail.stderr, 'owned synthetic stderr');
        return true;
      });
    }
    const missing = join(folder, 'missing-chrome');
    await assert.rejects(realBrowserResult(missing, folder, ''), (error: Error) => {
      assert.match(error.message, /Owned Chrome could not spawn/);
      const detail = JSON.parse(error.message.slice(error.message.indexOf(': ') + 2));
      assert.equal(detail.binary, missing);
      assert.equal(detail.pid, null);
      assert.match(detail.spawnError, /ENOENT/);
      return true;
    });
  } finally {
    await rm(folder, { recursive: true, force: true });
  }
});

test('native messages require the current document envelope and bounded JSON-RPC', () => {
  const encoded = JSON.stringify(message);
  assert.equal(nativeMiniappMessage(JSON.stringify({ nonce, message: encoded }), nonce), encoded);
  for (const value of [
    encoded,
    JSON.stringify({ nonce: other, message: encoded }),
    JSON.stringify({ nonce, message: encoded, bypass: true }),
    JSON.stringify({ nonce, message: '[]' }),
    JSON.stringify({ nonce, message: '{' }),
    JSON.stringify({ nonce, message: '{"jsonrpc":"1.0"}' }),
    JSON.stringify({ nonce, message: 'x'.repeat(1_500_001) }),
  ])
    assert.equal(nativeMiniappMessage(value, nonce), null);
  // JSON encoding the native envelope escapes the already serialized payload.
  const quoted = JSON.stringify({ ...message, params: { text: '"'.repeat(300_000) } });
  assert.equal(nativeMiniappMessage(JSON.stringify({ nonce, message: quoted }), nonce), quoted);
});

test('injected commands preserve JSON data and bind send/dispose to their document', () => {
  const calls: unknown[][] = [];
  const payload: unknown = JSON.parse(
    '{"jsonrpc":"2.0","id":2,"result":{"__proto__":{"x":1},"text":"</script>🌤️"}}',
  );
  const context = {
    window: {
      __zoenHostTransport: {
        send: (...args: unknown[]) => calls.push(args),
        dispose: (...args: unknown[]) => calls.push(args),
      },
    },
  };
  runInNewContext(nativeMiniappCommand(nonce, 'send', payload), context);
  runInNewContext(nativeMiniappCommand(other, 'dispose'), context);
  assert.equal(calls.length, 2);
  assert.equal(calls[0]?.[0], nonce);
  assert.equal(JSON.stringify(calls[0]?.[1]), JSON.stringify(payload));
  assert.equal(
    Object.hasOwn((calls[0]![1] as typeof message & { result: object }).result, '__proto__'),
    true,
  );
  assert.deepEqual(calls[1], [other]);
  assert.throws(() => nativeMiniappHtml('', '', 'not-a-document-nonce'));
  assert.throws(() => nativeMiniappHtml('x'.repeat(9 * 1024 * 1024 + 1), '', nonce));
});

test('native navigation allows only the outer blank document and its sandboxed srcdoc', () => {
  assert.equal(nativeMiniappNavigation({ url: 'about:blank', isTopFrame: true }), true);
  assert.equal(nativeMiniappNavigation({ url: 'about:srcdoc', isTopFrame: false }), true);
  assert.equal(nativeMiniappNavigation({ url: 'about:srcdoc#selection', isTopFrame: false }), true);
  for (const url of [
    'https://example.com',
    'file:///private/fixture',
    'data:text/html,hi',
    'javascript:alert(1)',
    'about:srcdoc',
  ])
    assert.equal(nativeMiniappNavigation({ url, isTopFrame: true }), false);
  assert.equal(nativeMiniappNavigation({ url: 'https://example.com', isTopFrame: false }), false);
});

test(
  'official App uses a real parent/source through the native shell and retires after disposal',
  { timeout: 30_000 },
  async (t) => {
    const chrome = await findBrowserExecutable();
    if (!chrome) {
      t.skip('Real Chrome is unavailable; codec tests do not claim browser source semantics.');
      return;
    }
    const folder = await mkdtemp(join(tmpdir(), 'zoen-mcp-app-transport-'));
    try {
      const appBundle = await build({
        stdin: {
          resolveDir: new URL('../..', import.meta.url).pathname,
          contents: `
        import { App } from '@modelcontextprotocol/ext-apps';
        const app = new App({ name: 'Synthetic transport oracle', version: '1.0.0' }, {}, { autoResize: false, strict: true });
        let parentSource = false;
        window.addEventListener('message', event => { if(event.source===window.parent&&event.data?.method==='qa/synthetic') window.dispatchEvent(new MessageEvent('message',{data:event.data.params.reply})); });
        window.addEventListener('message', event => { if(event.data?.jsonrpc==='2.0' && event.source===window.parent) parentSource=true; });
        (async()=>{
          await app.connect();
          const result=await app.callServerTool({name:'synthetic_echo',arguments:{text:'QA only'}});
          const sent=await app.sendMessage({role:'user',content:[{type:'text',text:'Synthetic transport only'}]});
          let opaque=false; try { void window.parent.document.body; } catch { opaque=true; }
          window.parent.postMessage({jsonrpc:'2.0',method:'qa/done',params:{
            realParent:window.parent!==window,parentSource,opaque,syntheticRejected:app.getHostVersion()?.name==='Synthetic host',
            echo:result.structuredContent?.text==='QA only',message:sent.isError!==true
          }},'*');
        })().catch(error=>window.parent.postMessage({jsonrpc:'2.0',method:'qa/failure',params:{message:String(error)}},'*'));
      `,
        },
        bundle: true,
        format: 'iife',
        platform: 'browser',
        write: false,
        minify: true,
        logLevel: 'silent',
      });
      const helpers = await build({
        entryPoints: [new URL('./host-transport.ts', import.meta.url).pathname],
        bundle: true,
        format: 'iife',
        globalName: 'TransportHelpers',
        platform: 'browser',
        write: false,
        minify: true,
        logLevel: 'silent',
      });
      const child = sandboxHtml(
        `<html><head><title>Synthetic App</title></head><body><script>${appBundle.outputFiles[0]!.text.replace(/<\/script/gi, '<\\/script')}</script></body></html>`,
        [],
      );
      const bootstrap = `<script>${helpers.outputFiles[0]!.text}
      const nonce=${JSON.stringify(nonce)}, other=${JSON.stringify(other)};
      const methods=[];
      function report(value){const node=document.createElement('pre');node.id='transport-qa-result';node.textContent=btoa(JSON.stringify(value));document.body.appendChild(node);}
      window.addEventListener('error',event=>report({success:false,error:String(event.message)}));
      window.addEventListener('unhandledrejection',event=>report({success:false,error:String(event.reason)}));
      window.ReactNativeWebView={postMessage(encoded){
        const decoded=TransportHelpers.nativeMiniappMessage(encoded,nonce);
        if(decoded===null){report({success:false,error:'invalid native envelope'});return;}
        const request=JSON.parse(decoded);methods.push(request.method);
        let result;
        if(request.method==='ui/initialize')result={protocolVersion:request.params.protocolVersion,hostInfo:{name:'Synthetic host',version:'1.0.0'},hostCapabilities:{serverTools:{},message:{text:{}}},hostContext:{theme:'light',displayMode:'inline'}};
        else if(request.method==='tools/call')result={content:[{type:'text',text:'Synthetic result'}],structuredContent:{text:request.arguments?.text??request.params?.arguments?.text}};
        else if(request.method==='ui/message'){if(!Array.isArray(request.params.content))throw Error('Message blocks were not standard');result={};}
        else if(request.method==='qa/failure'){report({success:false,error:request.params.message});return;}
        else if(request.method==='qa/done'){
          const frame=document.getElementById('miniapp'), source=frame.contentWindow, transport=window.__zoenHostTransport;
          const before=methods.length;
          const spoof={jsonrpc:'2.0',method:'qa/forged'};
          window.dispatchEvent(new MessageEvent('message',{data:spoof,source:window,origin:'null'}));
          window.dispatchEvent(new MessageEvent('message',{data:spoof,source,origin:'https://unrelated.example'}));
          window.dispatchEvent(new MessageEvent('message',{data:{...spoof,params:{text:'x'.repeat(1500001)}},source,origin:'null'}));
          const rejected=methods.length===before;
          const oldSend=transport.send(other,{jsonrpc:'2.0',id:999,result:{}})===false;
          const oldDispose=transport.dispose(other)===false;
          const labelStable=transport.title(nonce,'Renamed synthetic app')===true&&frame.title==='Renamed synthetic app'&&frame.contentWindow===source;
          const disposed=transport.dispose(nonce)===true;
          window.dispatchEvent(new MessageEvent('message',{data:spoof,source,origin:'null'}));
          const stopped=transport.send(nonce,{jsonrpc:'2.0',id:998,result:{}})===false&&methods.length===before;
          report({success:true,...request.params,rejected,oldSend,oldDispose,labelStable,disposed,stopped,removed:document.getElementById('miniapp')===null,methods});return;
        }
        if(request.id!==undefined&&result!==undefined){
          if(request.method==='ui/initialize')window.__zoenHostTransport.send(nonce,{jsonrpc:'2.0',method:'qa/synthetic',params:{reply:{jsonrpc:'2.0',id:request.id,result:{...result,hostInfo:{name:'Synthetic event must be rejected',version:'1.0.0'}}}}});
          eval(TransportHelpers.nativeMiniappCommand(nonce,'send',{jsonrpc:'2.0',id:request.id,result}));
        }
      }};
    </script>`;
      const html = nativeMiniappHtml(child, 'Synthetic </script> title', nonce).replace(
        '<script>',
        `${bootstrap}<script>`,
      );
      const path = join(folder, 'fixture.html');
      await writeFile(path, html, { mode: 0o600 });
      const encoded = await realBrowserResult(chrome, folder, path);
      const result = JSON.parse(Buffer.from(encoded, 'base64').toString('utf8')) as Record<
        string,
        unknown
      >;
      assert.equal(result.success, true, JSON.stringify(result));
      for (const key of [
        'realParent',
        'parentSource',
        'syntheticRejected',
        'opaque',
        'echo',
        'message',
        'rejected',
        'oldSend',
        'oldDispose',
        'labelStable',
        'disposed',
        'stopped',
        'removed',
      ])
        assert.equal(result[key], true, key);
      assert.deepEqual(result.methods, [
        'ui/initialize',
        'ui/notifications/initialized',
        'tools/call',
        'ui/message',
        'qa/done',
      ]);
    } finally {
      // Chrome descendants may finish writing their owned profile after close.
      await rm(folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    }
  },
);
