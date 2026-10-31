import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, nodes } from './attachment-route-ui.fixtures.ts';
import { sharedContact } from '../media/attachment-content.ts';
import { deferred } from './media-draft-channel.fixtures.ts';
import { fixture as attachments, tick } from './durable-attachment-composer.fixtures.ts';

const selection = (id = 'source') => ({
  source_id: id,
  revision: 'r1',
  text: '😀',
  spans: [{ start_utf16: 6, end_utf16: 8 }],
});
const reviewed = (shared: boolean) =>
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
          selection: selection(),
        },
      }
    : { reply_to: 'source', reply_quote: selection() };

test('compiled attachment chooser offers Contact and Location for quoted replies and fences a retained Poll tab', async () => {
  const f = attachments();
  f.render();
  await tick();
  f.render();
  const previous = f.routes()!;
  assert.ok(previous.categories.includes('poll'));
  f.render({ replyTo: 'source', replyQuote: selection(), replyIntent: 'A' });
  assert.deepEqual([...f.routes()!.categories], ['location', 'contact', 'article']);
  previous.open('poll', () => true);
  assert.equal(f.navigations.length, 0);
  f.routes()!.open('contact', () => false);
  assert.equal(f.navigations.length, 0);
  f.routes()!.open('contact', () => true);
  assert.equal(f.navigations.length, 1);
  assert.deepEqual(JSON.parse(JSON.stringify(f.navigations[0])), {
    pathname: '/chat-attachment/[id]',
    params: {
      id: 'chat',
      kind: 'contact',
      thread: '',
      identity: 'owner',
      device: 'device',
      epoch: '7',
    },
  });
  f.unmount();
});

for (const shared of [false, true]) {
  test(`compiled Contact freezes ${shared ? 'cross-chat' : 'local selected'} reply before the address-book chooser, retains edits and stages without sending`, async () => {
    const f = fixture('contact');
    f.render();
    await f.tick();
    f.chooseReply(reviewed(shared));
    await f.tick();
    const chooser = deferred<ReturnType<typeof sharedContact> | null>();
    f.contactGate(chooser.promise);
    f.button(f.render(), 'attachmentTabs.chooseContact').props.onPress();
    f.chooseReply({ reply_to: 'later', reply_quote: selection('later') });
    chooser.resolve(sharedContact('Chosen person', ['+123']));
    await f.tick();
    let tree = f.render();
    nodes(tree)
      .find((node) => node.type === 'Field' && node.props.label === 'attachmentTabs.contactName')!
      .props.onChangeText('Reviewed name');
    tree = f.render();
    f.button(tree, 'attachmentTabs.addContact').props.onPress();
    await f.tick();
    const value = JSON.parse(f.readMedia().content_json);
    assert.equal(value.reply_to, shared ? null : 'source');
    assert.equal(shared ? value.shared_reply.selection.text : value.reply_quote.text, '😀');
    assert.notEqual(value.reply_intent, f.read().reply_intent);
    assert.equal(f.read().reply_to, 'later');
    assert.equal(f.read().text, '**old draft**');
    assert.match(f.prepared[0]!, /FN:Reviewed name/);
    assert.equal(f.readMedia().assets[0]!.mime, 'text/vcard');
    assert.equal(f.counts().sends, 0);
    assert.equal(f.counts().backs, 1);
  });

  test(`compiled Location retains ${shared ? 'cross-chat' : 'local selected'} reply from its picker session while GPS is pending`, async () => {
    const f = fixture('location');
    f.render();
    await f.tick();
    f.chooseReply(reviewed(shared));
    await f.tick();
    const gps = deferred<{ latitude: number; longitude: number } | null>();
    f.gpsGate(gps.promise);
    f.button(f.render(), 'attachmentTabs.chooseLocation').props.onPress();
    f.chooseReply({ reply_to: 'later', reply_quote: selection('later') });
    gps.resolve({ latitude: 1, longitude: 2 });
    await f.tick();
    f.button(f.render(), 'attachmentTabs.addToMessage').props.onPress();
    await f.tick();
    const value = JSON.parse(f.readMedia().content_json);
    assert.equal(value.reply_to, shared ? null : 'source');
    assert.equal(shared ? value.shared_reply.selection.text : value.reply_quote.text, '😀');
    assert.notEqual(value.reply_intent, f.read().reply_intent);
    assert.equal(f.read().reply_to, 'later');
    assert.equal(f.counts().sends, 0);
    assert.deepEqual(JSON.parse(f.prepared[0]!), {
      type: 'Feature',
      geometry: { type: 'Point', coordinates: [2, 1] },
      properties: {},
    });
  });
}
