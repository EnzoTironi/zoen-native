import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MediaDraftChannel } from '../media/media-draft-channel.ts';
import type { MediaDraftDto, MediaDraftPort } from '../media/media-draft-types.ts';
import { recording, find, tick } from './voice-controls.fixtures.ts';

function journal() {
  let value: MediaDraftDto = { revision: '0', content_json: 'null', assets: [], attempted: false };
  let serial = 0;
  const saved: string[] = [];
  const port: MediaDraftPort = {
    load: async () => value,
    save: async (_scope, expected, json, assets) => {
      assert.equal(expected, value.revision);
      const parsed = JSON.parse(json);
      saved.push(json);
      const refs = parsed
        ? [parsed.source_asset_id, ...(parsed.render ? [parsed.render.asset_id] : [])]
        : [];
      value = {
        revision: `r${++serial}`,
        content_json: json,
        attempted: false,
        assets: refs.map((id: string) => {
          const input = assets.find((asset) => asset.id === id);
          const previous = value.assets.find((asset) => asset.id === id);
          if (previous) return previous;
          assert.ok(input);
          return { id, name: input.name, mime: input.mime, size: 128, sha256: 'a'.repeat(64) };
        }),
      };
      return { accepted_revision: value.revision, draft: value };
    },
    send: async () => assert.fail('capture and unlink must never publish'),
    materialize: async () => assert.fail('a newly captured voice does not restore a cache lease'),
    release: async () => {},
  };
  const channel = new MediaDraftChannel(
    {
      expected_identity_id: 'owner',
      expected_device_id: 'device',
      space_id: 'chat',
      thread_root_id: null,
    },
    port,
  );
  return { channel, saved };
}

test('compiled native capture binds its quote before permission and preserves it through Stop despite later composer context', async () => {
  const j = journal();
  await j.channel.load();
  const f = recording(true, 'ios', j.channel);
  const quote = {
    source_id: 'A',
    revision: 'A_r1',
    text: '😀',
    spans: [{ start_utf16: 6, end_utf16: 8 }],
  };
  let options: any = {
    replyTo: 'A',
    replyQuote: quote,
    replyIntent: 'intent_A',
    renderControls: (args: any) => ({
      type: 'Controls',
      props: { reply: args.reply, children: [args.voice, args.review] },
    }),
  };
  const render = () => f.render(options);
  const first = render();
  const start = find(first, 'VoiceRecordButton').props.onStart(false);
  options = {
    ...options,
    replyTo: 'B',
    replyQuote: { ...quote, source_id: 'B' },
    replyIntent: 'intent_B',
  };
  render();
  assert.equal(await start, true, JSON.stringify(f.events));
  let tree = render();
  assert.equal(find(tree, 'Controls').props.reply.props.scope.replyIntent, 'intent_A');
  await find(tree, 'VoiceDock').props.onStop();
  await tick();
  await j.channel.flush();
  await tick();
  tree = render();
  const content = j.channel.getSnapshot().content!;
  assert.equal(content.reply_to, 'A');
  assert.equal(content.reply_quote?.text, '😀');
  assert.equal(content.reply_intent, 'intent_A');
  assert.equal(find(tree, 'Controls').props.reply.props.scope.replyTo, 'A');
  const removed: any[] = [];
  options = { ...options, onReplyCancelled: (reply: any) => removed.push(reply) };
  tree = render();
  find(tree, 'Controls').props.reply.props.onCancel();
  await j.channel.flush();
  assert.equal(removed[0]?.reply_intent, 'intent_A');
  const unlinked = j.channel.getSnapshot().content!;
  assert.equal(unlinked.reply_to, null);
  assert.equal(unlinked.reply_quote, undefined);
  assert.equal(unlinked.kind, 'voice');
  assert.deepEqual(f.sent, []);
  await find(render(), 'VoiceDock').props.onCancel();
  f.app.unmount();
});
