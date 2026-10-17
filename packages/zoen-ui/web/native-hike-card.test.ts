import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const output = resolve(import.meta.dirname, '../node_modules/.cache/hike-test.mjs');
buildSync({ entryPoints: [resolve(import.meta.dirname, 'native-hike-card.tsx')], outfile: output, bundle: true, platform: 'node', format: 'esm', packages: 'external', jsx: 'automatic', loader: { '.css': 'empty' } });
const { parseHikeInfo } = await import(pathToFileURL(output).href);
const app = (view: Record<string, unknown> = {}) => ({ appId: 'actual-app', name: 'Hike', view });
test('native defaults, localized trail names and first itinerary step', () => {
  const h = parseHikeInfo(app(), 'en');
  assert.equal(h.title, 'Hike'); assert.equal(h.status, '3 trails • Maps & photos'); assert.equal(h.photo, './assets/native/hike-tomales.jpg');
  const pt = parseHikeInfo(app({ decided: 'steep', day: 'Sábado', itinerary: [{ time: '8:15', text: 'Partida' }] }), 'pt-BR');
  assert.equal(pt.status, 'Pedra Grande • Sábado'); assert.equal(pt.firstStep, '8:15 Partida');
});
test('actual votes, first tied leader, custom names, decision precedence', () => {
  const view = { title: 'Weekend', trails: [{ id: 'steep', name: ' Custom ', votes: ['Ana', 'Bia'] }, { id: 'lands', votes: ['Ana', 'Dan'] }] };
  const h = parseHikeInfo(app(view), 'en');
  assert.equal(h.total, 4); assert.equal(h.leader, 'steep'); assert.equal(h.status, '4 votes •  Custom  leads'); assert.deepEqual(h.voters, ['Ana', 'Bia', 'Ana', 'Dan']);
  const decided = parseHikeInfo(app({ ...view, decided: 'lands', day: 'Sunday' }), 'en');
  assert.equal(decided.status, 'Lands End • Sunday'); assert.equal(decided.photo, './assets/native/hike-lands.jpg');
});
test('typed native fallbacks and unknown asset IDs cannot form URLs', () => {
  const h = parseHikeInfo(app({ title: 12, day: null, decided: '../../remote', trails: [{ id: 'steep', name: '  ', votes: ['Ana', 3] }], itinerary: [{}] }), 'en');
  assert.equal(h.title, 'Hike'); assert.equal(h.total, 0); assert.equal(h.photo, undefined); assert.equal(h.firstStep, ' '); assert.equal(h.displayName('steep'), 'Steep Ravine');
});
