import assert from 'node:assert/strict';
import { test } from 'node:test';
import { nativeContactReader } from '../media/native-contact.ts';
import type { LocalMediaFile } from '../media/types';

const file: LocalMediaFile = {
  uri: 'file:///synthetic/authorized/contact.vcf',
  itemId: 'signed-item',
  version: 7,
  info: {
    name: 'contact.vcf',
    path: '',
    mime: 'text/vcard',
    bytes: '160',
    sha256: 'a'.repeat(64),
    ready: true,
    chunks: 1,
    chunks_here: 1,
    has_thumbnail: false,
  },
};

test('native contact read binds the held exact item/version/hash, projects only name/phones and never releases its lease', async () => {
  const requests: unknown[] = [];
  const read = nativeContactReader({
    readContactFile: async (request) => {
      requests.push(JSON.parse(request));
      return JSON.stringify({
        name: 'Ana',
        phones: [{ label: 'mobile', value: '+55 11 1234', private: 'discard' }],
        email: 'discard@example.invalid',
        photo: 'discard',
      });
    },
  });
  const contact = await read(file);
  assert.deepEqual(requests, [
    { uri: file.uri, itemId: file.itemId, version: 7, sha256: file.info.sha256 },
  ]);
  assert.deepEqual(contact, { name: 'Ana', phones: [{ label: 'mobile', value: '+55 11 1234' }] });
  assert.ok(
    Object.isFrozen(contact) &&
      Object.isFrozen(contact.phones) &&
      Object.isFrozen(contact.phones[0]),
  );
});

test('malformed or oversized native contact projections fail closed', async () => {
  for (const value of [
    null,
    {},
    { name: '', phones: [] },
    { name: 'é'.repeat(129), phones: [] },
    { name: 'Ana', phones: Array.from({ length: 11 }, () => ({ label: '', value: '1' })) },
    { name: 'Ana', phones: [{ label: 'x'.repeat(65), value: '1' }] },
    { name: 'Ana', phones: [{ label: '', value: 'x'.repeat(81) }] },
    { name: 'Ana', phones: [{ label: '', value: '+1\nSEND' }] },
    { name: 'Ana', phones: [{ label: '\0', value: '1' }] },
  ]) {
    await assert.rejects(
      nativeContactReader({ readContactFile: async () => JSON.stringify(value) })(file),
      /contact-invalid/,
    );
  }
});

test('native authorization/read failure remains a failure with no retry, import or fallback metadata', async () => {
  let calls = 0;
  const failure = new Error('revoked');
  await assert.rejects(
    nativeContactReader({
      readContactFile: async () => {
        calls++;
        throw failure;
      },
    })(file),
    (error) => error === failure,
  );
  assert.equal(calls, 1);
});
