import { test } from 'node:test';
import assert from 'node:assert/strict';
import { eventPreview } from './event-preview.ts';
test('frozen event preview uses actual authored schedule content without identity metadata', () => {
  const preview = eventPreview(JSON.stringify({ schema: 1, owner_id: 'private-identity', token: 'private-capability', events: [{ id: 'internal-event', title: 'Kickoff', description: 'Bring a notebook', at_ms: 1700000000000 }] }));
  assert.match(preview, /Kickoff/); assert.match(preview, /Bring a notebook/); assert.match(preview, new RegExp(new Date(1700000000000).getFullYear().toString()));
  assert.doesNotMatch(preview, /private-|internal-event|schema|owner_id/);
});
test('malformed event state has an honest error instead of a phone-only placeholder', () => {
  assert.equal(eventPreview('{'), 'Event details could not be loaded.');
  assert.equal(eventPreview('{"description":"<script>not executable</script>"}'), 'description: <script>not executable</script>');
});
