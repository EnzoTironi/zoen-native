import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

const compiled = await build({
  entryPoints: [new URL('./markdown-files.ts', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'native-file-ports',
      setup(builder) {
        builder.onResolve({ filter: /^(expo-|\.\.\/i18n\/)/ }, ({ path }) => ({
          path,
          external: true,
        }));
      },
    },
  ],
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function fixture() {
  const availability = deferred<boolean>(),
    write = deferred<void>();
  const effects: string[] = [];
  let active = true;
  class Directory {
    exists = true;
    create() {
      effects.push('directory.create');
    }
    delete() {
      effects.push('directory.delete');
      this.exists = false;
    }
  }
  class File {
    uri = 'owned-temporary-export';
    create() {
      effects.push('file.create');
    }
    write() {
      effects.push('file.write');
      return write.promise;
    }
  }
  const ports: Record<string, unknown> = {
    'expo-document-picker': {},
    'expo-file-system': { Directory, File, Paths: { cache: 'cache' } },
    'expo-sharing': {
      isAvailableAsync: () => {
        effects.push('available');
        return availability.promise;
      },
      shareAsync: async () => {
        effects.push('share');
      },
    },
    '../i18n/index.ts': { t: (key: string) => key },
    '../i18n/money.ts': { formatMoneyInput: () => '' },
  };
  const module = {
    exports: {} as {
      exportMarkdown(title: string, markdown: string, current: () => boolean): Promise<void>;
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  return {
    effects,
    availability,
    write,
    stop: () => {
      active = false;
    },
    run: () => module.exports.exportMarkdown('Selected v2', '# Exact', () => active),
  };
}

test('native Markdown export rechecks authority after both awaited preparation boundaries and deletes its cache', async () => {
  for (const boundary of ['before', 'availability', 'write', 'success']) {
    const f = fixture();
    if (boundary === 'before') f.stop();
    const running = f.run();
    if (boundary === 'availability') f.stop();
    f.availability.resolve(true);
    await new Promise<void>((done) => setImmediate(done));
    if (boundary === 'write') f.stop();
    f.write.resolve();
    await running;
    assert.equal(f.effects.includes('share'), boundary === 'success', boundary);
    assert.equal(
      f.effects.includes('directory.delete'),
      ['write', 'success'].includes(boundary),
      boundary,
    );
    assert.equal(
      f.effects.includes('directory.create'),
      ['write', 'success'].includes(boundary),
      boundary,
    );
  }
});
