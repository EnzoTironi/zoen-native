import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { messageAddress, nativeLinkPath, parseZoenLink } from '../notifications/links.ts';
import { messageMarkdownLink, messageMayContainMarkdown } from './message-markdown.ts';

test('the installed native parser makes pasted message links actionable without changing text or code', () => {
  const directory = mkdtempSync(join(tmpdir(), 'zoen-message-autolinks-'));
  try {
    const parser = fileURLToPath(
      new URL('../../node_modules/react-native-enriched-markdown/cpp/md4c/', import.meta.url),
    );
    const source = join(directory, 'links.c');
    const binary = join(directory, 'links');
    writeFileSync(
      source,
      String.raw`#include "md4c.h"
#include <stdio.h>
#include <string.h>
static int block(MD_BLOCKTYPE t, void *d, void *u) { return 0; }
static int leave(MD_SPANTYPE t, void *d, void *u) { return 0; }
static int text(MD_TEXTTYPE t, const MD_CHAR *s, MD_SIZE n, void *u) { return 0; }
static int enter(MD_SPANTYPE t, void *d, void *u) {
  if (t == MD_SPAN_A) {
    MD_SPAN_A_DETAIL *a = d;
    fwrite(a->href.text, 1, a->href.size, stdout);
    fputc('\n', stdout);
  }
  return 0;
}
int main(int argc, char **argv) {
  if (argc != 2) return 2;
  MD_PARSER parser = {0};
  parser.flags = MD_DIALECT_GITHUB;
  parser.enter_block = parser.leave_block = block;
  parser.enter_span = enter;
  parser.leave_span = leave;
  parser.text = text;
  return md_parse(argv[1], strlen(argv[1]), &parser, NULL);
}
`,
    );
    execFileSync('cc', ['-I', parser, source, join(parser, 'md4c.c'), '-o', binary], {
      timeout: 30_000,
    });
    const links = (text: string) =>
      execFileSync(binary, [text], { encoding: 'utf8' }).trim().split('\n').filter(Boolean);
    const address = messageAddress('read-chat', 'message')!;
    assert.equal(messageMayContainMarkdown(address, true), true);
    for (const text of [address, `Veja (${address}).`, `[original](${address})`]) {
      assert.deepEqual(links(text), [address], text);
      assert.equal(messageMarkdownLink(address), address);
      assert.deepEqual(parseZoenLink(address), {
        kind: 'message',
        space: 'read-chat',
        message: 'message',
      });
      assert.equal(nativeLinkPath(address), '/message/read-chat/message');
    }
    assert.deepEqual(links('`' + address + '`'), []);
    assert.deepEqual(links('```\n' + address + '\n```'), []);
    assert.deepEqual(links('https://message/read-chat/message'), []);
    assert.deepEqual(links('zoen:///read-chat/message'), []);
    assert.deepEqual(links(`https://example.org/a ${address}`), ['https://example.org/a', address]);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
