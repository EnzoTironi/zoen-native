import assert from 'node:assert/strict';
import { test } from 'node:test';
import { messageHashtag, messageHashtagLink } from './message-hashtag.ts';
import { messageMayContainMarkdown } from './message-markdown.ts';
import { fixture } from './message-markdown.fixtures.ts';
import { ConversationSearch } from './conversation-search.ts';

test('local hashtag entities validate Unicode and never admit another target or executable URL', () => {
  for (const value of ['#release', '#AÇÃO_2026', '#发布']) {
    assert.equal(messageHashtag(value), value);
    assert.equal(messageHashtagLink('zoen-hashtag:' + encodeURIComponent(value)), value);
    assert.equal(messageMayContainMarkdown(value, true), true);
  }
  for (const value of [
    '#',
    '#___',
    '#a b',
    '#a/other',
    'https://example.com/#tag',
    '#a\n',
    '#a'.repeat(300),
  ])
    assert.equal(messageHashtag(value), null);
  for (const url of [
    'zoen-hashtag:%',
    'zoen-hashtag:https://example.com',
    'zoen-hashtag:%23tag?space=other',
    'zoen://hashtag/%23tag',
  ])
    assert.equal(messageHashtagLink(url), null);
});

test('native message link opens the existing scoped search and keeps a late response fenced', async () => {
  let current = true;
  let finish!: (value: { hits: never[]; next_cursor: null }) => void;
  const seen: string[] = [];
  const search = new ConversationSearch({
    current: () => current,
    search: (query) => {
      seen.push(query);
      return new Promise((resolve) => {
        finish = resolve;
      });
    },
  });
  const f = fixture('ios', {
    hashtag: (tag) => {
      search.openQuery(tag);
    },
  });
  const source = 'Hello #Ação_2026';
  const rendered = f.render({ text: source, color: '#fff', fontSize: 17, maxWidth: 300 });
  assert.equal(rendered.props.markdown, source);
  assert.equal((rendered.props.md4cFlags as { hashtags: boolean }).hashtags, true);
  const press = rendered.props.onLinkPress as (event: { url: string }) => void;
  press({ url: 'zoen-hashtag:%23A%C3%A7%C3%A3o_2026' });
  assert.equal(search.getSnapshot().active, true);
  assert.equal(search.getSnapshot().query, '#Ação_2026');
  const pending = search.search();
  assert.deepEqual(seen, ['#Ação_2026']);
  search.close();
  current = false;
  finish({ hits: [], next_cursor: null });
  await pending;
  press({ url: 'zoen-hashtag:%23other' });
  assert.equal(search.getSnapshot().active, false);
  assert.equal(search.getSnapshot().query, '');
  assert.deepEqual(f.opened, []);
  assert.deepEqual(f.copied, []);
  f.close();
  current = true;
  press({ url: 'zoen-hashtag:%23retired' });
  assert.equal(search.getSnapshot().active, false);
});

test('a preview outside a conversation does not turn hashtags into navigable entities', () => {
  const f = fixture();
  const rendered = f.render({ text: '#release', color: '#fff', fontSize: 17, maxWidth: 300 });
  assert.equal((rendered.props.md4cFlags as { hashtags: boolean }).hashtags, false);
  (rendered.props.onLinkPress as (event: { url: string }) => void)({
    url: 'zoen-hashtag:%23release',
  });
  assert.deepEqual(f.opened, []);
  f.close();
});
