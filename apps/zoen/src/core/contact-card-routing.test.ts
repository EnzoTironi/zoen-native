import assert from 'node:assert/strict';
import test from 'node:test';
import { elements, fixture, tick } from './inline-image-frame.fixtures.ts';

test('actual FileAttachment routes authorized vCard metadata to a contact card without reading bytes on render', async () => {
  const f = fixture(false, {
    info: { mime: 'text/vcard', name: 'contact.vcf' },
    contactCapability: true,
  });
  f.render();
  await tick();
  const card = elements(f.render()).find((node) => node.type === 'ContactAttachment');
  assert.ok(card, 'native contact capability must select the actual contact consumer');
  assert.equal((card.props.source as { expectedHash: string }).expectedHash, 'a'.repeat(64));
  assert.deepEqual(f.materialized, [], 'metadata alone must not decrypt/materialize contacts');
});

test('non-contact MIME and platforms without a native parser keep the existing file renderer', async () => {
  for (const [mime, capability] of [
    ['text/vcard', false],
    ['text/plain', true],
  ] as const) {
    const f = fixture(false, {
      info: { mime, name: 'contact.vcf' },
      contactCapability: capability,
    });
    f.render();
    await tick();
    assert.equal(
      elements(f.render()).some((node) => node.type === 'ContactAttachment'),
      false,
    );
    assert.deepEqual(f.materialized, []);
  }
});
