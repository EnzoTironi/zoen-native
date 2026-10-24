import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const patch = readFileSync(
  new URL('../../patches/@expo+ui+58.0.23.patch', import.meta.url),
  'utf8',
);
function section(name: string): string | undefined {
  return patch
    .split('diff --git ')
    .find((part) => part.startsWith(`a/node_modules/@expo/ui/ios/${name} `));
}
function added(name: string): string {
  const part = section(name);
  assert.ok(part, `actual production patch source ${name} exists`);
  return (
    part
      .split('\n')
      .filter((line) => line.startsWith('+') && !line.startsWith('+++'))
      .map((line) => line.slice(1))
      .join('\n') + '\n'
  );
}
function nativeSource(directory: string): string {
  const name = 'RNHostView.swift';
  const original = readFileSync(
    new URL(`../../node_modules/@expo/ui/ios/${name}`, import.meta.url),
    'utf8',
  );
  // postinstall may already have applied our patch. Never modify installed dependencies.
  if (original.includes('.onChange(of: props.layoutRoot)')) return original;
  const delta = section(name);
  if (!delta) return original;
  const target = join(directory, 'node_modules/@expo/ui/ios');
  mkdirSync(target, { recursive: true });
  writeFileSync(join(target, name), original);
  execFileSync('git', ['apply', '-'], {
    cwd: directory,
    input: `diff --git ${delta}`,
    timeout: 10000,
  });
  return readFileSync(join(target, name), 'utf8');
}

test('real RNHost source reconciles layout-root prop changes with a stable child', () => {
  const directory = mkdtempSync(join(tmpdir(), 'zoen-rnhost-source-'));
  try {
    const source = nativeSource(directory);
    assert.match(
      source,
      /\.onChange\(of: props\.layoutRoot\)\s*\{\s*_ in\s*updateTouchHandler\(\)\s*\}/,
    );
    assert.match(
      source,
      /touchHandler\.update\(layoutRoot: props\.layoutRoot, view: firstChildUIView\)/,
    );
    assert.match(source, /\.onDisappear\s*\{\s*touchHandler\.detach\(\)/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test(
  'actual patched touch binding reconciles stable-child prop order and retires old ownership',
  { skip: process.platform !== 'darwin', timeout: 60000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-rnhost-binding-'));
    try {
      const binding = join(directory, 'RNHostTouchBinding.swift');
      writeFileSync(binding, added('RNHostTouchBinding.swift'));
      const binary = join(directory, 'binding-tests');
      execFileSync(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-j',
          '1',
          '-module-cache-path',
          join(directory, 'cache'),
          binding,
          fileURLToPath(
            new URL('../../modules/zoen-core/tests/RNHostTouchBindingTests.swift', import.meta.url),
          ),
          '-o',
          binary,
        ],
        { timeout: 40000 },
      );
      assert.match(
        execFileSync(binary, [], { encoding: 'utf8', timeout: 10000 }),
        /PASS stable child, late root prop, retirement, replacement, weak ownership and attach refusal/,
      );
      const adapter = join(directory, 'RNHostView.swift');
      writeFileSync(adapter, nativeSource(directory));
      execFileSync('xcrun', ['swiftc', '-frontend', '-parse', adapter], { timeout: 10000 });
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
