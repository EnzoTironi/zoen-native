// Preserve every original registration/name/order; callback bodies live by behavior.
import test from 'node:test';
import * as connection from './transport-tests/connection.ts';
import * as durability from './transport-tests/durability.ts';
import * as media from './transport-tests/media.ts';
import * as uploads from './transport-tests/uploads.ts';
import * as documents from './transport-tests/documents.ts';
import * as maintenance from './transport-tests/maintenance.ts';
import * as profile from './transport-tests/profile.ts';
import * as history from './transport-tests/history.ts';
import * as attachmentIntent from './transport-tests/attachment-intent.ts';

test('disconnect during Ready FetchLink preserves reconnect', connection.scenario01);
test(
  'a delayed Ready decode from a closed generation cannot authenticate its replacement',
  connection.scenario02,
);
test(
  'closing a generation while auth bytes await never sends stale authentication',
  connection.scenario03,
);
test(
  'native early presence does not replace or prevent authenticated Ready',
  connection.scenario04,
);
test(
  'production message controller terminates the core and emits nothing when IndexedDB save fails',
  durability.scenario05,
);
test(
  'mutation refusals propagate in Portuguese without terminating the session; retry persists before send',
  durability.scenario06,
);
test(
  'mutation persistence failure is fatal and never emits an unsaved annotation',
  durability.scenario07,
);
test(
  'production send persists the ratchet before emitting and reconnect replays only saved state',
  durability.scenario08,
);
test(
  'offline sends persist locally and emit only from the next authenticated bootstrap',
  connection.scenario09,
);
test(
  'native voice metadata and legacy paths survive parsing; forwarding clears only the reply',
  media.scenario10,
);
test(
  'web recordings post a FileDoc with native voice metadata after uploading',
  uploads.scenario11,
);
test(
  'malformed attachments stay local while received updates persist, synchronize and reload',
  media.scenario12,
);
test(
  'files and thumbnails require the complete digest and exact decrypted lengths',
  media.scenario13,
);
test(
  'native media bounds reject hostile descriptors before any network or worker work',
  media.scenario14,
);
test(
  'encrypted chunk reads reject truncation and cancel streamed overruns before decryption',
  media.scenario15,
);
test(
  'failed upload audio opens from retained encrypted chunks when relay has no blob',
  media.scenario16,
);
test(
  'valid chunks assemble in manifest order and snapshot metadata before fetching',
  media.scenario17,
);
test(
  'upload retries stop after four attempts for HTTP failures and network errors',
  uploads.scenario18,
);
test(
  'upload failure keeps the queued voice note as failed instead of losing it',
  uploads.scenario19,
);
test('disconnect after upload retains voice bytes locally without publishing', uploads.scenario20);
test(
  'document blobs persist before upload and publish only after the completed upload is durable',
  documents.scenario21,
);
test(
  'a failed durable upload acknowledgement terminates the core and publishes no document frame',
  documents.scenario22,
);
test(
  'an owner request encoded for an old socket never reaches its authenticated replacement',
  connection.scenario23,
);
test(
  'typing and presence are delivered without rewriting durable snapshots on every keystroke',
  maintenance.scenario24,
);
test(
  'handle_taken never renames locally and a successful alias waits for Registered before durable projection',
  profile.scenario25,
);
test(
  'claim receipts require the negotiated capability and exact relay clock; reconnect clears it',
  connection.scenario26,
);
test(
  'scheduled MLS maintenance persists the ratchet before publish and stops on disconnect',
  maintenance.scenario27,
);
test(
  'history status reports retained progress and retries share the one active receive pump',
  history.scenario28,
);
test(
  'outgoing linked history waits for encrypted persistence before signed PUT and saves the acknowledgement',
  history.scenario29,
);
test('outgoing linked history never uploads when encrypted persistence fails', history.scenario30);
test(
  'a socket closing before onclose cannot terminate a durably saved message',
  durability.scenario31,
);
test(
  'attachment producer IDs come from Rust and UI ACK/drop use the exact issued sequence',
  attachmentIntent.scenario32,
);
test(
  'attachment batch snapshots options order and bytes before sealing and persists once before any publish',
  attachmentIntent.scenario33,
);
test(
  'attachment batch rejects unsupported options or incomplete sealing before admitting any outbox call',
  attachmentIntent.scenario34,
);
test(
  'attachment waits for durable ciphertext/intent before first PUT and saves barrier removal before publish',
  attachmentIntent.scenario35,
);
test(
  'attachment quota failure and owner retirement before PUT emit no upload or publication',
  attachmentIntent.scenario36,
);
test(
  'closing while attachment persistence is held keeps its saved intent and fences later PUT/publication',
  attachmentIntent.scenario37,
);
test(
  'attachment save held across reauthentication cannot pump before replacement bootstrap',
  attachmentIntent.scenario38,
);
