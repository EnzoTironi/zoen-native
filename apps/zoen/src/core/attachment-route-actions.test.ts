import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  NormalMessageDrafts,
  type NormalDraftScope,
  type NormalDraftSnapshot,
} from './normal-message-drafts.ts';
import { MediaDraftChannel } from '../media/media-draft-channel.ts';
import type { MediaDraftDto, MediaDraftPort } from '../media/media-draft-types.ts';
import type { TimelineEntry } from './types.ts';
import { captureArticleDraft, stageContactAttachment } from '../media/attachment-route-actions.ts';
import {
  appendAttachmentText,
  articleText,
  contactVCard,
  pollArguments,
  sharedContact,
  staticLocation,
} from '../media/attachment-content.ts';
import { conversationAttachmentCategory } from '../media/attachment-categories.ts';

const scope: NormalDraftScope = {
  owner: {
    mode: 'account',
    identityId: 'synthetic-owner',
    deviceId: 'synthetic-device',
    sessionEpoch: 3,
  },
  spaceId: 'synthetic-space',
  threadRootId: 'synthetic-thread',
};
const tick = async () => {
  for (let index = 0; index < 20; index++) await Promise.resolve();
};
function normalFixture() {
  let stored: NormalDraftSnapshot = {
    revision: '0',
    text: 'previous draft',
    reply_to: 'synthetic-reply',
  };
  const sent: { scope: NormalDraftScope; text: string; reply: string | null }[] = [];
  let writes = 0;
  const port = {
    load: async () => ({ ...stored }),
    save: async (_: NormalDraftScope, revision: string, text: string, reply: string | null) => {
      assert.equal(revision, stored.revision);
      stored = { revision: String(++writes), text, reply_to: reply };
      return { ...stored };
    },
    send: async (target: NormalDraftScope, revision: string) => {
      assert.equal(revision, stored.revision);
      sent.push({ scope: target, text: stored.text, reply: stored.reply_to });
      stored = { revision: String(++writes), text: '', reply_to: null };
      return {
        message_id: 'synthetic-message',
        text_revision: null,
        entry: { id: 'synthetic-message' } as TimelineEntry,
        draft: { ...stored },
      };
    },
  };
  const controller = new NormalMessageDrafts(port);
  return {
    channel: { controller, scope },
    sent,
    read: () => stored,
    reopen: () => new NormalMessageDrafts(port),
  };
}
function mediaFixture(lostAck = false) {
  let stored: MediaDraftDto = { revision: '0', content_json: 'null', assets: [], attempted: false };
  let saved: { accepted_revision: string; draft: MediaDraftDto } | null = null;
  const ingresses: string[] = [];
  let effects = 0,
    releases = 0,
    prepared = 0,
    admitted = 0;
  const port: MediaDraftPort = {
    load: async () => stored,
    async save(_, revision, json, assets) {
      if (assets[0]) ingresses.push(assets[0].uri);
      if (saved && revision === '0') return saved;
      assert.equal(revision, stored.revision);
      effects++;
      stored = {
        revision: '1',
        content_json: json,
        assets: assets.map((asset) => ({
          id: asset.id,
          name: asset.name,
          mime: asset.mime,
          size: 50,
          sha256: 'a'.repeat(64),
        })),
        attempted: false,
      };
      saved = { accepted_revision: '1', draft: stored };
      if (lostAck) {
        lostAck = false;
        throw new Error('synthetic lost acknowledgement');
      }
      return saved;
    },
    send: async () => {
      throw new Error('contact staging cannot send');
    },
    release: async () => {},
    materialize: async () => {
      throw new Error('not used');
    },
  };
  const media = new MediaDraftChannel(
    {
      expected_identity_id: 'synthetic-owner',
      expected_device_id: 'synthetic-device',
      space_id: 'synthetic-space',
      thread_root_id: 'synthetic-thread',
    },
    port,
  );
  const configuration = {
    media,
    contact: sharedContact('Synthetic Name', ['+0000000']),
    current: () => true,
    replyTo: () => 'synthetic-reply',
    nextId: () => 'synthetic-contact-id',
    admitted: () => {
      admitted++;
    },
    prepare: async (text: string) => {
      assert.match(text, /BEGIN:VCARD/);
      prepared++;
      return {
        uri: 'file:///synthetic-cache/contact.vcf',
        release: async () => {
          releases++;
        },
      };
    },
  };
  return {
    media,
    configuration,
    read: () => ({ stored, effects, releases, prepared, admitted, ingresses }),
  };
}

test('native Article capture retains exact Markdown, Unicode, whitespace, reply/thread and cold draft; only explicit send posts', async () => {
  const f = normalFixture();
  await f.channel.controller.activate(scope);
  const markdown = '# Heading\n\n**bold** and [link](https://example.invalid)\n  👨‍👩‍👧‍👦 ação  \n';
  await captureArticleDraft(
    f.channel,
    async () => markdown,
    () => true,
  );
  assert.equal(f.read().text, markdown);
  assert.equal(f.read().reply_to, 'synthetic-reply');
  assert.equal(f.sent.length, 0);
  await f.channel.controller.retire({ flush: true });
  const reopened = f.reopen();
  await reopened.activate(scope);
  assert.equal(reopened.getSnapshot(scope).text, markdown);
  assert.equal(reopened.getSnapshot(scope).scope.threadRootId, scope.threadRootId);
  await reopened.send(scope);
  assert.equal(f.sent.length, 1);
  assert.equal(f.sent[0]!.text, markdown);
  assert.equal(f.sent[0]!.reply, 'synthetic-reply');
  assert.deepEqual(f.sent[0]!.scope, scope);
});

test('Article capture refuses a stale completion and invalid native content without replacing the canonical draft', async () => {
  for (const invalid of ['stale', 'null', 'large'] as const) {
    const f = normalFixture();
    await f.channel.controller.activate(scope);
    let current = true;
    await assert.rejects(
      captureArticleDraft(
        f.channel,
        async () => {
          if (invalid === 'stale') current = false;
          return invalid === 'null'
            ? 'hidden\0value'
            : invalid === 'large'
              ? '😀'.repeat(20000)
              : 'stale value';
        },
        () => current,
      ),
    );
    assert.equal(f.read().text, 'previous draft');
    assert.equal(f.sent.length, 0);
  }
});

test('retired Article input is not captured and rendering content helpers causes no dispatch', async () => {
  const f = normalFixture();
  let captures = 0;
  await assert.rejects(
    captureArticleDraft(
      f.channel,
      async () => {
        captures++;
        return 'none';
      },
      () => false,
    ),
  );
  assert.equal(captures, 0);
  assert.equal(f.sent.length, 0);
  assert.equal(articleText('**not yet sent**'), '**not yet sent**');
  assert.equal(f.sent.length, 0);
});

test('Contact stage uses the canonical file draft with exact reply/thread and never sends', async () => {
  const f = mediaFixture();
  await stageContactAttachment(f.configuration);
  await tick();
  const state = f.read();
  assert.equal(state.effects, 1);
  assert.equal(state.admitted, 1);
  assert.equal(state.releases, 1);
  assert.deepEqual(JSON.parse(state.stored.content_json), {
    kind: 'attachments',
    reply_to: 'synthetic-reply',
    options: { presentation: 'file', grouping: 'separate' },
    items: [{ asset_id: 'synthetic-contact-id', mutation_id: 'synthetic-contact-id', caption: '' }],
  });
  assert.equal(f.media.scope.thread_root_id, 'synthetic-thread');
  assert.equal(state.stored.assets[0]!.mime, 'text/vcard');
});

test('lost Contact ingress ACK retains the same URI until exact canonical reconciliation; it creates no second file/effect', async () => {
  const f = mediaFixture(true);
  await assert.rejects(stageContactAttachment(f.configuration));
  await tick();
  assert.equal(f.read().releases, 0);
  assert.equal(f.read().effects, 1);
  await f.media.flush();
  await tick();
  assert.equal(f.read().prepared, 1);
  assert.equal(f.read().effects, 1);
  assert.equal(f.read().releases, 1);
  assert.deepEqual(f.read().ingresses, [
    'file:///synthetic-cache/contact.vcf',
    'file:///synthetic-cache/contact.vcf',
  ]);
});

test('Contact stale-before and stale-after preparation cannot overwrite a draft and release only their own copy', async () => {
  const before = mediaFixture();
  await assert.rejects(stageContactAttachment({ ...before.configuration, current: () => false }));
  assert.equal(before.read().prepared, 0);
  assert.equal(before.read().effects, 0);
  const after = mediaFixture();
  let current = true;
  await assert.rejects(
    stageContactAttachment({
      ...after.configuration,
      current: () => current,
      prepare: async (text) => {
        const file = await after.configuration.prepare(text);
        current = false;
        return file;
      },
    }),
  );
  assert.equal(after.read().effects, 0);
  assert.equal(after.read().admitted, 0);
  assert.equal(after.read().releases, 1);
});

test('Contact concurrent canonical draft change is preserved rather than overwritten', async () => {
  const f = mediaFixture();
  await assert.rejects(
    stageContactAttachment({
      ...f.configuration,
      prepare: async (text) => {
        const file = await f.configuration.prepare(text);
        await f.media.save(
          {
            kind: 'attachments',
            reply_to: null,
            items: [
              {
                asset_id: 'synthetic-other',
                mutation_id: 'synthetic-other',
                caption: 'concurrent',
              },
            ],
          },
          [
            {
              id: 'synthetic-other',
              uri: 'file:///synthetic-cache/other.vcf',
              name: 'other.vcf',
              mime: 'text/vcard',
            },
          ],
        );
        return file;
      },
    }),
  );
  assert.equal(f.read().admitted, 0);
  assert.equal(f.read().releases, 1);
  assert.equal(f.read().stored.assets[0]!.id, 'synthetic-other');
});

test('vCard exports only reviewed name/phones, escapes injection and folds UTF-8 without breaking a code point', () => {
  const contact = sharedContact('A, B; C\\D\n' + 'é'.repeat(70), ['+123;456']);
  const vcard = contactVCard(contact);
  const lines = vcard.split('\r\n');
  assert.ok(lines.every((line) => new TextEncoder().encode(line).length <= 75));
  assert.equal(vcard.replaceAll('\r\n ', '').includes('é'.repeat(70)), true);
  assert.match(vcard, /FN:A\\, B\\; C\\\\D\\n/);
  assert.match(vcard, /TEL:\+123\\;456/);
  assert.equal(lines.filter((line) => line === 'BEGIN:VCARD').length, 1);
  assert.throws(() => sharedContact('name', ['+1\r\nEMAIL:hidden@example.invalid']));
  assert.throws(() => sharedContact('hidden\0name', []));
});

test('static location is a finite bounded reviewed map link, and preserves existing message text', () => {
  assert.equal(staticLocation(-23.5, -46.6), 'https://maps.apple.com/?ll=-23.5,-46.6');
  assert.equal(
    appendAttachmentText('existing **draft**', staticLocation(0, 0)),
    'existing **draft**\n\nhttps://maps.apple.com/?ll=0,0',
  );
  for (const point of [
    [NaN, 0],
    [Infinity, 0],
    [91, 0],
    [0, 181],
  ])
    assert.throws(() => staticLocation(point[0]!, point[1]!));
});

test('Poll uses bounded canonical existing-App arguments; invalid/duplicate/oversized choices are refused', () => {
  assert.deepEqual(pollArguments(' Q? ', [' A ', 'B']), { question: 'Q?', options: ['A', 'B'] });
  for (const args of [
    ['Q', ['one']],
    ['Q', ['one', 'one']],
    ['', ['a', 'b']],
    ['Q', ['a', '\0']],
    ['Q', ['a', 'x'.repeat(201)]],
  ] as const)
    assert.throws(() => pollArguments(args[0], args[1]));
});

test('attachment route names form a closed set and do not admit arbitrary app/provider input', () => {
  for (const name of ['article', 'poll', 'contact', 'location'])
    assert.equal(conversationAttachmentCategory(name), name);
  for (const name of ['', 'provider', 'http://untrusted.invalid', null, ['article']])
    assert.equal(conversationAttachmentCategory(name), null);
});

test('Article bounds count UTF-8 bytes and preserve the exact maximum without truncation', () => {
  const exact = '😀'.repeat(16_384);
  assert.equal(articleText(exact), exact);
  assert.throws(() => articleText(exact + 'a'));
});
