import assert from 'node:assert/strict';
import { test } from 'node:test';
import { matchesFolder, newFolder, withoutFolder, cloneFolders } from '../inbox/folders.ts';
import { inboxFixture } from './inbox-actions.fixtures.ts';
import type { InboxFolders } from './generated/InboxFolders';

test('custom folder rules preserve explicit include/exclude precedence, unread reminders and canonical mute/archive', () => {
  const { chat } = inboxFixture();
  const folder = { ...newFolder('work'), name: 'Work', kinds: ['Group'] as const };
  const config: InboxFolders = {
    folders: [{ ...folder, kinds: [...folder.kinds] }],
    default_folder: folder.id,
  };
  assert.equal(matchesFolder(chat, folder.id, config), true);
  config.folders[0].exclude_read = true;
  chat.unread = 0;
  assert.equal(matchesFolder(chat, folder.id, config), false);
  chat.manual_unread = true;
  assert.equal(matchesFolder(chat, folder.id, config), true);
  chat.muted = true;
  config.folders[0].exclude_muted = true;
  assert.equal(matchesFolder(chat, folder.id, config), false);
  chat.archived = true;
  config.folders[0].include.push(chat.id);
  assert.equal(
    matchesFolder(chat, folder.id, config),
    true,
    'Explicit include wins automatic filters',
  );
  config.folders[0].exclude.push(chat.id);
  assert.equal(
    matchesFolder(chat, folder.id, config),
    false,
    'Explicit exclusion wins every inclusion',
  );
});

test('deleting the initial folder falls back to All; cloning freezes ordered rules without aliasing draft arrays', () => {
  const folder = { ...newFolder('travel'), name: 'Viagem 🪁', include: ['first', 'second'] };
  const config = { folders: [folder], default_folder: folder.id };
  const frozen = cloneFolders(config);
  folder.include.reverse();
  folder.name = 'Changed';
  assert.equal(frozen.folders[0].name, 'Viagem 🪁');
  assert.deepEqual(frozen.folders[0].include, ['first', 'second']);
  assert.deepEqual(withoutFolder(config, folder.id), { folders: [], default_folder: 'all' });
  config.default_folder = 'unread';
  assert.equal(withoutFolder(config, folder.id).default_folder, 'unread');
});

test('builtins keep existing archive and manual unread semantics and never include an unknown folder', () => {
  const { chat } = inboxFixture();
  chat.unread = 0;
  chat.manual_unread = true;
  assert.equal(matchesFolder(chat, 'unread'), true);
  assert.equal(matchesFolder(chat, 'unknown'), false);
  chat.archived = true;
  assert.equal(matchesFolder(chat, 'all'), false);
  assert.equal(matchesFolder(chat, 'archived'), true);
});
