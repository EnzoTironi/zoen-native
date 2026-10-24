import assert from 'node:assert/strict';
import { test } from 'node:test';
import { locationGeoJSON, parseLocationGeoJSON } from '../media/location-file.ts';

test('a static location is one exact RFC7946 Point Feature, including bounded Unicode label', () => {
  const point = { longitude: -43.95, latitude: -19.92, label: 'Praça 👩‍💻' };
  const text = locationGeoJSON(point);
  assert.deepEqual(JSON.parse(text), {
    type: 'Feature',
    geometry: { type: 'Point', coordinates: [-43.95, -19.92] },
    properties: { label: point.label },
  });
  assert.deepEqual(parseLocationGeoJSON(text), point);
  assert.deepEqual(parseLocationGeoJSON(locationGeoJSON({ longitude: 180, latitude: -90 })), {
    longitude: 180,
    latitude: -90,
  });
});

test('GeoJSON refuses extras, collections, altitude, invalid coordinates, controls and oversized data', () => {
  const valid = JSON.parse(locationGeoJSON({ longitude: 1, latitude: 2 }));
  for (const candidate of [
    { ...valid, crs: {} },
    { ...valid, id: 'not-our-document' },
    { ...valid, geometry: { ...valid.geometry, coordinates: [1, 2, 3] } },
    {
      ...valid,
      geometry: {
        type: 'LineString',
        coordinates: [
          [1, 2],
          [3, 4],
        ],
      },
    },
    { ...valid, properties: { label: 'name', owner: 'invented' } },
    { type: 'FeatureCollection', features: [valid] },
  ])
    assert.throws(() => parseLocationGeoJSON(JSON.stringify(candidate)));
  for (const point of [
    { longitude: Infinity, latitude: 2 },
    { longitude: 181, latitude: 2 },
    { longitude: 1, latitude: NaN },
    { longitude: 1, latitude: -91 },
    { longitude: 1, latitude: 2, label: 'a\nname' },
    { longitude: 1, latitude: 2, label: '👩'.repeat(65) },
  ])
    assert.throws(() => locationGeoJSON(point));
  assert.throws(() => parseLocationGeoJSON(' '.repeat(4097)));
});

test('GeoJSON MIME does not infer a location from a filename or generic JSON', async () => {
  const { isLocationFileMime } = await import('../media/location-file.ts');
  assert.equal(isLocationFileMime(' Application/Geo+Json; charset=utf-8 '), true);
  assert.equal(isLocationFileMime('application/json'), false);
});

test('cold draft summary reads only bounded held bytes and requires their exact journal hash before showing a point', async () => {
  const { readDraftLocation } = await import('../media/draft-location-read.ts');
  const { createHash } = await import('node:crypto');
  const text = locationGeoJSON({ longitude: 1, latitude: 2, label: 'Saved place' });
  const sha256 = createHash('sha256').update(text).digest('hex');
  let reads = 0,
    size = Buffer.byteLength(text),
    current = true;
  const port = {
    size: () => size,
    bytes: async (_: string, maximum: number) => {
      assert.equal(maximum, 4097);
      reads++;
      return new TextEncoder().encode(text);
    },
    sha256: async (value: string) => createHash('sha256').update(value).digest('hex'),
  };
  assert.equal(
    (await readDraftLocation({ uri: 'file:///held', sha256 }, port, () => current))!.longitude,
    1,
  );
  size = 4097;
  await assert.rejects(readDraftLocation({ uri: 'file:///held', sha256 }, port, () => current));
  assert.equal(reads, 1, 'oversized files are rejected before reading');
  size = Buffer.byteLength(text);
  await assert.rejects(
    readDraftLocation({ uri: 'file:///held', sha256: 'b'.repeat(64) }, port, () => current),
    /location-changed/,
  );
  current = false;
  assert.equal(await readDraftLocation({ uri: 'file:///held', sha256 }, port, () => current), null);
});
