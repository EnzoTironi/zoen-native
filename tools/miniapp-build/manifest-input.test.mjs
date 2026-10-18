import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm, access, symlink } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';
const { buildApp } = await import(
  process.env.ZOEN_MINIAPP_BUILDER_MODULE ?? new URL('./build.mjs', import.meta.url)
);

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'zoen-miniapp-manifests-'));
  const app = join(root, 'app');
  const out = join(root, 'out');
  await mkdir(join(app, 'src'), { recursive: true });
  await mkdir(join(app, 'manifests'));
  const manifest = async (id, entry, extra = {}) => {
    const path = `manifests/${id}.json`;
    await writeFile(
      join(app, path),
      JSON.stringify({ id, name: id, entry, capabilities: [], allowedDomains: [], ...extra }),
    );
    return { manifestPath: path };
  };
  return { root, app, out, manifest, close: () => rm(root, { recursive: true, force: true }) };
}

test('separate manifests build only their real graph, keeping explicit link authority local', async () => {
  const f = await fixture();
  try {
    await writeFile(join(f.app, 'src/plain.ts'), 'document.body.textContent="offline";');
    await writeFile(
      join(f.app, 'src/link.ts'),
      'document.body.textContent="https://wa.me/?text=synthetic";',
    );
    const plain = await f.manifest('plain', 'src/plain.ts');
    const link = await f.manifest('link', 'src/link.ts', { linkDomains: ['wa.me'] });
    const first = await buildApp(f.app, f.out, plain);
    assert.deepEqual(first.linkDomains, []);
    assert.doesNotMatch(await readFile(join(f.out, 'plain.html'), 'utf8'), /wa\.me/);
    const linked = await buildApp(f.app, f.out, link);
    assert.deepEqual(linked.linkDomains, ['wa.me']);
    assert.match(await readFile(join(f.out, 'link.html'), 'utf8'), /wa\.me/);
    await f.manifest('link', 'src/link.ts');
    await assert.rejects(buildApp(f.app, f.out, link), /undeclared host "wa.me"/);
  } finally {
    await f.close();
  }
});

test('undeclared URLs in a transitive selected source fail before any output is written', async () => {
  const f = await fixture();
  try {
    await writeFile(
      join(f.app, 'src/main.ts'),
      'import {url} from "./data";document.body.textContent=url;',
    );
    await writeFile(
      join(f.app, 'src/data.ts'),
      'export const url="https://outside.example/private";',
    );
    const options = await f.manifest('bounded', 'src/main.ts');
    await assert.rejects(buildApp(f.app, f.out, options), /undeclared host "outside.example"/);
    await assert.rejects(access(join(f.out, 'bounded.html')));
  } finally {
    await f.close();
  }
});

test('manifest, entry and prefix-sibling imports cannot escape the selected app root', async () => {
  const f = await fixture();
  try {
    await writeFile(join(f.root, 'outside.ts'), 'document.body.textContent="outside";');
    await assert.rejects(
      buildApp(f.app, f.out, { manifestPath: '../outside.json' }),
      /Manifest must stay/,
    );
    const outside = await f.manifest('bounded', '../outside.ts');
    await assert.rejects(buildApp(f.app, f.out, outside), /Entry must stay/);
    await mkdir(join(f.root, 'app-sibling'));
    await writeFile(join(f.root, 'app-sibling/payload.ts'), 'export const text="outside";');
    await writeFile(
      join(f.app, 'src/main.ts'),
      'import {text} from "../../app-sibling/payload";document.body.textContent=text;',
    );
    const sibling = await f.manifest('bounded', 'src/main.ts');
    await assert.rejects(buildApp(f.app, f.out, sibling), /Import escapes the app folder/);
  } finally {
    await f.close();
  }
});

test('a manifest symlink cannot import authority from outside the app', async () => {
  const f = await fixture();
  try {
    await writeFile(join(f.app, 'src/main.ts'), 'document.body.textContent="offline";');
    await writeFile(
      join(f.root, 'outside.json'),
      JSON.stringify({
        id: 'bounded',
        name: 'bounded',
        entry: 'src/main.ts',
        capabilities: [],
        allowedDomains: [],
        linkDomains: ['outside.example'],
      }),
    );
    await symlink(join(f.root, 'outside.json'), join(f.app, 'manifests/linked.json'));
    await assert.rejects(
      buildApp(f.app, f.out, { manifestPath: 'manifests/linked.json' }),
      /Manifest must stay inside the app folder/,
    );
    await assert.rejects(access(join(f.out, 'bounded.html')));
  } finally {
    await f.close();
  }
});

test('an entry symlink cannot bundle an outside source', async () => {
  const f = await fixture();
  try {
    await writeFile(join(f.root, 'outside.ts'), 'document.body.textContent="outside";');
    await symlink(join(f.root, 'outside.ts'), join(f.app, 'src/entry.ts'));
    const options = await f.manifest('bounded', 'src/entry.ts');
    await assert.rejects(buildApp(f.app, f.out, options), /Entry must stay inside the app folder/);
    await assert.rejects(access(join(f.out, 'bounded.html')));
  } finally {
    await f.close();
  }
});

test('an extensionless transitive symlink cannot bypass the source or URL boundary', async () => {
  const f = await fixture();
  try {
    await writeFile(
      join(f.app, 'src/main.ts'),
      'import {url} from "./alias";document.body.textContent=url;',
    );
    await writeFile(
      join(f.root, 'outside.ts'),
      'export const url="https://outside.example/private";',
    );
    await symlink(join(f.root, 'outside.ts'), join(f.app, 'src/alias.ts'));
    const options = await f.manifest('bounded', 'src/main.ts');
    await assert.rejects(
      buildApp(f.app, f.out, options),
      /Source escapes the approved mini-app roots/,
    );
    await assert.rejects(access(join(f.out, 'bounded.html')));
  } finally {
    await f.close();
  }
});

test('an outside source named node_modules is not an approved dependency', async () => {
  const f = await fixture();
  try {
    await mkdir(join(f.root, 'node_modules/react'), { recursive: true });
    const outside = join(f.root, 'node_modules/react/payload.ts');
    await writeFile(outside, 'export const url="https://outside.example/private";');
    await writeFile(
      join(f.app, 'src/main.ts'),
      'import {url} from "./alias";document.body.textContent=url;',
    );
    await symlink(outside, join(f.app, 'src/alias.ts'));
    const options = await f.manifest('bounded', 'src/main.ts');
    await assert.rejects(
      buildApp(f.app, f.out, options),
      /Source escapes the approved mini-app roots/,
    );
    await assert.rejects(access(join(f.out, 'bounded.html')));
  } finally {
    await f.close();
  }
});

test('an in-app source symlink still enforces its declared URL policy', async () => {
  const f = await fixture();
  try {
    await writeFile(
      join(f.app, 'src/main.ts'),
      'import {url} from "./alias";document.body.textContent=url;',
    );
    await writeFile(
      join(f.app, 'src/data.ts'),
      'export const url="https://outside.example/private";',
    );
    await symlink(join(f.app, 'src/data.ts'), join(f.app, 'src/alias.ts'));
    const options = await f.manifest('bounded', 'src/main.ts');
    await assert.rejects(buildApp(f.app, f.out, options), /undeclared host "outside.example"/);
    await f.manifest('bounded', 'src/main.ts', { linkDomains: ['outside.example'] });
    const built = await buildApp(f.app, f.out, options);
    assert.deepEqual(built.linkDomains, ['outside.example']);
    assert.match(await readFile(join(f.out, 'bounded.html'), 'utf8'), /outside\.example/);
  } finally {
    await f.close();
  }
});
