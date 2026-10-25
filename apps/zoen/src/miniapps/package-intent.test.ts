import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Driver, deferred } from '../core/store.fixtures.ts';
import type { ItemDetail } from '../core/types.ts';
import { AppPackageIntent, isAppPackage } from './package-intent.ts';
import { AppPackageRelease } from './package-release.ts';

const source = () => ({
  expected_identity_id: 'owner',
  expected_device_id: 'device',
  space_id: 'space',
  package_item_id: 'file',
  package_version: 1,
  package_sha256: 'a'.repeat(64),
  title: 'Drawing',
});
const uri = `ui://zoen/package/file/1/${'a'.repeat(64)}`;
const app = (resource = uri) =>
  ({ id: 'app', space_id: 'space', app: { resource_uri: resource } }) as ItemDetail;
const doc = {
  item_id: 'app',
  version: 3,
  current_version: 3,
  sha256: 'b'.repeat(64),
  scene_json: '{"elements":[]}',
};

// Core storage/receipts are explicit ports here; no test claims real persistence.
test('package double tap shares one allocation/lookup/install and captures exact source before awaits', async () => {
  const driver = new Driver(),
    gate = deferred<string>();
  const inputs: unknown[] = [];
  let allocations = 0,
    lookups = 0,
    installs = 0;
  driver.operation = async (command) => {
    if (command.op === 'new_mutation_id') {
      allocations++;
      return gate.promise;
    }
    if (command.op === 'app_resource_find') {
      lookups++;
      inputs.push(command.input);
      return null;
    }
    if (command.op === 'app_resource_install') {
      installs++;
      inputs.push(command.input);
      return app();
    }
    throw Error('Unexpected command');
  };
  const input = source(),
    expected = { ...input, mutation_id: 'intent' };
  const action = new AppPackageIntent(
    input,
    (command) => driver.request(command),
    () => true,
  );
  const first = action.open(),
    second = action.open();
  assert.equal(first, second);
  input.package_version = 99;
  input.package_sha256 = 'f'.repeat(64);
  gate.resolve('intent');
  assert.equal((await first).id, 'app');
  assert.deepEqual({ allocations, lookups, installs }, { allocations: 1, lookups: 1, installs: 1 });
  assert.deepEqual(inputs, [expected, expected]);
});

test('lost install response resolves from canonical lookup, including after a cold presentation lifetime', async () => {
  const driver = new Driver();
  let accepted: ItemDetail | null = null,
    installs = 0,
    ids = 0;
  driver.operation = async (command) => {
    if (command.op === 'new_mutation_id') return `intent-${++ids}`;
    if (command.op === 'app_resource_find') return accepted;
    if (command.op === 'app_resource_install') {
      installs++;
      accepted = app();
      throw Error('lost response');
    }
    throw Error('Unexpected command');
  };
  const original = new AppPackageIntent(
    source(),
    (command) => driver.request(command),
    () => true,
  );
  await assert.rejects(original.open(), /lost response/);
  assert.equal((await original.open()).id, 'app');
  original.close();
  const reopened = new AppPackageIntent(
    source(),
    (command) => driver.request(command),
    () => true,
  );
  assert.equal((await reopened.open()).id, 'app');
  assert.equal(installs, 1);
});

test('failed or ambiguous canonical lookup never falls back to creating another App', async () => {
  const driver = new Driver();
  let installs = 0;
  driver.operation = async (command) => {
    if (command.op === 'new_mutation_id') return 'intent';
    if (command.op === 'app_resource_find') throw Error('ambiguous');
    if (command.op === 'app_resource_install') installs++;
    throw Error('Unexpected command');
  };
  const action = new AppPackageIntent(
    source(),
    (command) => driver.request(command),
    () => true,
  );
  await assert.rejects(action.open(), /ambiguous/);
  await assert.rejects(action.open(), /ambiguous/);
  assert.equal(installs, 0);
});

test('retirement during allocation prevents install; late accepted install cannot navigate into a new owner', async () => {
  for (const stage of ['allocation', 'install'] as const) {
    const driver = new Driver(),
      gate = deferred<unknown>(),
      entered = deferred<void>();
    let installs = 0;
    driver.operation = async (command) => {
      if (command.op === 'new_mutation_id') {
        if (stage === 'allocation') {
          entered.resolve();
          return gate.promise;
        }
        return 'intent';
      }
      if (command.op === 'app_resource_find') return null;
      if (command.op === 'app_resource_install') {
        installs++;
        entered.resolve();
        return gate.promise;
      }
      throw Error('Unexpected command');
    };
    const action = new AppPackageIntent(
      source(),
      (command) => driver.request(command),
      () => true,
    );
    const pending = action.open();
    await entered.promise;
    action.close();
    gate.resolve(stage === 'allocation' ? 'late-id' : app());
    await assert.rejects(pending);
    assert.equal(installs, stage === 'allocation' ? 0 : 1);
  }
});

test('retry of an accepted creation may return its same Item with a newer canonical release', async () => {
  const driver = new Driver();
  driver.operation = async (command) => {
    if (command.op === 'new_mutation_id') return 'original';
    if (command.op === 'app_resource_find') return null;
    if (command.op === 'app_resource_install')
      return app(`ui://zoen/package/new-file/4/${'c'.repeat(64)}`);
    throw Error('Unexpected command');
  };
  const action = new AppPackageIntent(
    source(),
    (command) => driver.request(command),
    () => true,
  );
  assert.equal((await action.open()).id, 'app');
});

test('release retry keeps exact Core document base and mutation after lost ACK; it never changes the scene', async () => {
  const driver = new Driver(),
    commands: unknown[] = [];
  let reads = 0,
    ids = 0;
  driver.operation = async (command) => {
    if (command.op === 'app_document_read_pinned') {
      reads++;
      return doc;
    }
    if (command.op === 'new_mutation_id') {
      ids++;
      return 'update-intent';
    }
    if (command.op === 'app_resource_update') {
      commands.push(command.input);
      if (commands.length === 1) throw Error('lost response');
      return { ...doc, version: 4, current_version: 5 };
    }
    throw Error('Unexpected command');
  };
  const prior = `ui://zoen/package/prior/1/${'d'.repeat(64)}`;
  const action = new AppPackageRelease(
    source(),
    'app',
    prior,
    (command) => driver.request(command),
    () => true,
  );
  await assert.rejects(action.update(), /lost response/);
  const accepted = await action.update();
  assert.equal(accepted.scene_json, doc.scene_json);
  assert.equal(reads, 1);
  assert.equal(ids, 1);
  assert.deepEqual(commands[0], commands[1]);
  assert.deepEqual(commands[0], {
    admission: { ...source(), mutation_id: 'update-intent' },
    item_id: 'app',
    expected_version: 3,
    expected_sha256: doc.sha256,
    expected_resource_uri: prior,
  });
});

test('already installed release is checked with Core and produces no update or mutation', async () => {
  const driver = new Driver();
  driver.operation = async (command) => {
    assert.equal(command.op, 'app_document_read_pinned');
    return doc;
  };
  const action = new AppPackageRelease(
    source(),
    'app',
    uri,
    (command) => driver.request(command),
    () => true,
  );
  assert.equal((await action.update()).version, 3);
});

test('ordinary HTML filename or MIME never opts into running an App', () => {
  assert.equal(
    isAppPackage({ kind_id: 'file', file: { mime: 'text/html', name: 'app.html' } } as ItemDetail),
    false,
  );
  assert.equal(
    isAppPackage({ kind_id: 'file', file: { mime: 'text/html;profile=mcp-app' } } as ItemDetail),
    true,
  );
});
