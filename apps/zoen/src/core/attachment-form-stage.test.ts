import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  stageContactAttachment,
  stageLocationAttachment,
} from '../media/attachment-route-actions.ts';
import { fixture, deferred, tick } from './media-draft-channel.fixtures.ts';

const quote = () => ({
  source_id: 'source',
  revision: 'r1',
  text: '😀',
  spans: [{ start_utf16: 6, end_utf16: 8 }],
});
const target = (shared: boolean) =>
  shared
    ? {
        reply_to: null,
        shared_reply: {
          space_id: 'private',
          message_id: 'source',
          revision: 'r1',
          author_id: 'author',
          author_name: 'Author',
          text: '😀',
          selection: quote(),
        },
        reply_intent: 'A',
      }
    : { reply_to: 'source', reply_quote: quote(), reply_intent: 'A' };

for (const form of ['contact', 'location'] as const)
  for (const shared of [false, true]) {
    test(`${form} stages an owned ${shared ? 'shared' : 'selected local'} reply before asynchronous preparation, preserving bytes and later composer context`, async () => {
      const f = fixture();
      await f.channel.load();
      const original = target(shared),
        expected = structuredClone(original),
        ready = deferred();
      let current = original;
      let text = '';
      let released = 0;
      const options = {
        media: f.channel,
        current: () => true,
        replyTo: () => current.reply_to,
        reviewedReply: () => current,
        nextId: () => `selected_${form}`,
        admitted: () => {},
        prepare: async (value: string) => {
          text = value;
          await ready.promise;
          f.sources.set('file:///prepared', value);
          return {
            uri: 'file:///prepared',
            release: async () => {
              released++;
            },
          };
        },
      };
      const work =
        form === 'contact'
          ? stageContactAttachment({
              ...options,
              contact: { name: 'Reviewed person', phones: ['+123'] },
            })
          : stageLocationAttachment({
              ...options,
              point: { latitude: 1, longitude: 2 },
              caption: 'Reviewed place',
            });
      await tick();
      original.reply_intent = 'mutated';
      const selection = original.reply_quote ?? original.shared_reply?.selection;
      selection!.spans[0]!.end_utf16 = 99;
      current = {
        reply_to: 'later',
        reply_quote: { ...quote(), source_id: 'later' },
        reply_intent: 'B',
      };
      ready.resolve();
      await work;
      const content = f.channel.getSnapshot().content!;
      assert.equal(content.kind, 'attachments');
      assert.equal(content.reply_intent, 'A');
      assert.deepEqual(
        content.reply_quote ?? content.shared_reply?.selection,
        expected.reply_quote ?? expected.shared_reply?.selection,
      );
      assert.equal(content.reply_to, expected.reply_to);
      assert.equal(
        f.current().assets[0]!.mime,
        form === 'contact' ? 'text/vcard' : 'application/geo+json',
      );
      assert.equal(f.sources.get('file:///prepared'), text);
      assert.equal(
        f.calls.some((call) => call.op === 'send'),
        false,
      );
      assert.equal(current.reply_intent, 'B');
      assert.equal(released, 0);
      await f.channel.clear();
      await f.channel.retire(false);
      assert.equal(released, 1);
    });
  }
