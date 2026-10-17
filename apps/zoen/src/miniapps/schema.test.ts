import test from 'node:test';
import assert from 'node:assert/strict';
import { exactJson, formValue } from './schema.ts';

test('miniapp money arrives as an exact numeric JSON token beyond JS safe integers', () => {
  const schema = {
    type: 'object',
    required: ['services'],
    properties: {
      services: {
        type: 'array',
        items: {
          type: 'object',
          required: ['name', 'priceCents'],
          properties: { name: { type: 'string' }, priceCents: { type: 'integer', minimum: 0 } },
        },
      },
    },
  };
  const parsed = formValue(schema, {
    services: [{ name: 'Serviço', priceCents: '92233720368547758,07' }],
  });
  assert.equal(
    exactJson(parsed!),
    '{"services":[{"name":"Serviço","priceCents":9223372036854775807}]}',
  );
  assert.throws(() =>
    formValue(schema, { services: [{ name: 'A', priceCents: '92233720368547758,08' }] }),
  );
});
test('required nested values, enums and integer bounds are checked before signing', () => {
  assert.throws(() =>
    formValue({ type: 'object', required: ['a'], properties: { a: { type: 'string' } } }, {}),
  );
  assert.throws(() => formValue({ type: 'integer', minimum: 1, maximum: 12 }, '13', 'servings'));
  assert.throws(() =>
    formValue({ type: 'string', enum: ['confirm', 'cancel'] }, 'approve', 'action'),
  );
  assert.equal(formValue({ type: 'string' }, '', 'note', false), undefined);
});
