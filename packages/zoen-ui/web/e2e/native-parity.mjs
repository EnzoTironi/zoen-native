// Production-component checks against genuine read-only native-core data.
// Narrow viewport corresponds to the authoritative native crop: 62pt top, 0pt bottom.
// Wide geometry follows the canonical iPad balanced split-view render (320pt column).
// Does not claim relay, recording, file transport, or native pixel parity coverage.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
const args = Object.fromEntries(process.argv.slice(2).map((value, i, all) => value.startsWith('--') ? [value.slice(2), all[i + 1]] : null).filter(Boolean));
if (!args.playwright || !args.url || !args.output) throw new Error('Use --playwright /path/to/playwright/index.mjs --url http://localhost:4174/?parity=1 --output /path');
const { webkit } = await import(pathToFileURL(resolve(args.playwright)).href);
const revision = args.revision || execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
const fixture = JSON.parse(await readFile(new URL('../fixtures/native-parity-v1.json', import.meta.url), 'utf8'));
assert.ok(fixture.homeApps?.length > 0, 'actual native home snapshot export must be present');
await mkdir(args.output, { recursive: true });
const deadline = setTimeout(() => { console.error('FAIL WebKit 32-state UI check exceeded its 240-second deadline.'); process.exit(1); }, 240_000);
let browser;
console.log('WebKit matrix starting');
try {
  for (const viewport of ['narrow', 'wide']) for (const appearance of ['light', 'dark']) for (const screen of ['chats', 'spaces', 'notifications', 'profile', 'chat', 'composer', 'settings', 'onboarding']) {
    if (screen === 'chats') { if (browser) await browser.close(); browser = await webkit.launch({ timeout: 20_000 }); }
    const context = await browser.newContext({ viewport: viewport === 'narrow' ? { width: 402, height: 812 } : { width: 1376, height: 988 }, deviceScaleFactor: viewport === 'narrow' ? 3 : 2, isMobile: true, hasTouch: true, locale: 'en-US', timezoneId: 'UTC', colorScheme: appearance });
    const page = await context.newPage();
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    await page.clock.setFixedTime(new Date('2026-01-15T12:00:00Z'));
    page.setDefaultTimeout(10_000);
    await page.goto(args.url, { timeout: 10_000 });
    await page.waitForFunction(() => !!window.__ZOEN_PARITY__);
    await page.evaluate(config => window.__ZOEN_PARITY__.render(config), { fixture_id: 'native-parity-v1', fixed_time: '2026-01-15T12:00:00Z', locale: 'en_US', timezone: 'UTC', seed: 128, source_revision: revision, appearance, screen, viewport });
    assert.equal(await page.locator('.native-parity').getAttribute('data-screen'), screen);
    if (screen !== 'onboarding') assert.equal(await page.locator('.zm-native-production').count(), 1, 'actual production Messenger is rendered');
    if (viewport === 'wide' && screen !== 'onboarding') assert.equal(await page.locator('.na-shell').count(), 1, 'production adaptive shell is rendered');
    assert.equal(await page.evaluate(() => window.__ZOEN_PARITY__.manifest.source_revision), revision);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, 'no horizontal overflow');
    assert.equal(await page.locator('.zm-bubble .zm-meta').evaluateAll(elements => elements.every(element => { const meta = element.getBoundingClientRect(); const bubble = element.closest('.zm-bubble').getBoundingClientRect(); return meta.bottom <= bubble.bottom + 1 && meta.right <= bubble.right + 1; })), true, 'message timestamps stay inside their bubbles');
    if (['chats', 'spaces', 'notifications', 'profile'].includes(screen)) {
      assert.equal(await page.locator('.zm-home-head').count(), 0, 'compact root retains its native bottom-only chrome');
      assert.equal(await page.locator('.native-root-screen > h1').count(), viewport === 'wide' && ['notifications', 'profile'].includes(screen) ? 1 : 0, 'only iPad detail navigation owns a top title');
      assert.equal(await page.locator('[aria-label="Offline"]:not(.zn-navigation [aria-label="Offline"])').count(), 0, 'offline dot belongs only to the bottom bar');
    }
    if (screen === 'composer') {
      assert.deepEqual(await page.locator('.zm-send').first().evaluate(element => { const rect = element.getBoundingClientRect(); return [rect.width, rect.height]; }), [48, 48], 'SpaceView action is 48 logical points');
      assert.deepEqual(await page.locator('.zm-attach').first().evaluate(element => { const rect = element.getBoundingClientRect(); return [rect.width, rect.height]; }), [44, 44], 'ChatAttachmentButton is 44 logical points');
      assert.equal(await page.locator('textarea').inputValue(), 'See you tomorrow');
      await page.locator('textarea').fill('See you tomorrow again');
      assert.equal(await page.locator('textarea').inputValue(), 'See you tomorrow again');
      await page.locator('textarea').fill('See you tomorrow');
    }
    if (viewport === 'wide' && screen !== 'onboarding') {
      assert.equal(await page.locator('.na-sidebar').evaluate(element => element.getBoundingClientRect().width), 320);
      assert.equal(await page.locator('.na-rail').evaluate(element => element.getBoundingClientRect().width), 80);
      assert.equal(await page.locator('.na-chat-sidebar').evaluate(element => element.getBoundingClientRect().width), 228);
    }
    if (screen === 'chats') {
      assert.equal(await page.locator('.zm-chat-open').evaluateAll(rows => rows.every(row => {
        const bounds = row.getBoundingClientRect();
        return [...row.querySelectorAll('.zm-preview, time, .zm-badge')].every(element => {
          const rect = element.getBoundingClientRect();
          return rect.left >= bounds.left && rect.right <= Math.min(bounds.right, innerWidth) + 1;
        });
      })), true, 'chat previews truncate inside the row without pushing timestamps or unread badges offscreen');
    }
    if (screen === 'chats' && viewport === 'narrow') {
      assert.deepEqual(await page.locator('.nw-card[data-snapshot-id]').evaluateAll(elements => elements.map(element => element.getAttribute('data-snapshot-id'))), fixture.homeApps.map(snapshot => snapshot.id), 'the canonical native home snapshots and order are preserved exactly');
      assert.equal(await page.locator('.nw-empty').count(), 0, 'a populated native fixture must not be replaced with an empty state');
      assert.equal(await page.getByRole('button', { name: 'Search', exact: true }).count(), 1);
      const navigation = page.getByRole('navigation', { name: 'Main navigation' });
      assert.equal(await navigation.locator('.zn-tab').count(), 3);
      assert.equal(await navigation.getByRole('button', { name: 'Marketplace', exact: true }).count(), 1);
      assert.ok(await page.locator('.native-avatar').count() > 0, 'actual shared native avatars render');
      assert.equal(await page.locator('.zn-tabs').evaluate(element => element.getBoundingClientRect().height), 61);
      assert.equal(await page.locator('.zm-chat').count(), 6);
      assert.equal(await page.getByRole('button', { name: 'Spaces', exact: true }).count(), 0, 'groups remain in Chats');
    }
    if (screen === 'chats') {
      assert.equal(await page.getByRole('searchbox', { name: 'Search chats', exact: true }).count(), 0);
      assert.equal(await page.locator('[aria-label="Filter chats"]').count(), 0);
      assert.equal(await page.locator(viewport === 'wide' ? '.na-chat-sidebar .nw-strip > button:first-child' : '.zm-sidebar .nw-strip > button:first-child').getAttribute('aria-label'), 'Notifications');
      await page.getByRole('button', { name: 'Search', exact: true }).click();
      const query = page.getByRole('searchbox', { name: 'Search chats, messages and people' });
      await query.fill('Marina');
      assert.equal(await query.inputValue(), 'Marina');
      assert.equal(await page.getByRole('dialog').getByRole('region', { name: 'Chats', exact: true }).getByRole('button').count(), fixture.spaces.filter(space => space.title.includes('Marina')).length);
      await page.screenshot({ path: resolve(args.output, 'search-' + viewport + '-' + appearance + '.png') });
      await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    }
    if (screen === 'spaces') {
      const community = fixture.spaces.find(space => space.kind === 'community');
      assert.ok(community);
      assert.equal(await page.locator('.zm-app').getAttribute('data-selected-space'), community.id);
      assert.equal(await page.getByTestId('spaces-directory').count(), 0, 'historical capture name follows the real community-opening journey');
      assert.equal(await page.locator('.native-hike-card').count(), 1, 'the genuine app state uses the native hike renderer');
    }
    if (screen === 'notifications') {
      const pending = fixture.requests.filter(r => r.status === 'pending');
      const listed = fixture.requests.filter(r => r.status === 'pending' || r.status === 'stale');
      assert.equal(await page.locator('.nr-request').count(), listed.length);
      assert.equal(await page.getByRole('tab', { name: `Approvals ${pending.length}`, exact: true }).count(), 1);
      for (const request of listed) assert.equal(await page.getByRole('button', { name: `Review request: ${request.title}. ${request.audience}, ${request.actionLabel}, ${request.spaceTitle}`, exact: true }).count(), 1);
      await page.getByRole('tab', { name: /^Mentions/ }).click();
      assert.equal(await page.locator('.nr-mention').count(), fixture.mentions.length);
      for (const mention of fixture.mentions) assert.ok((await page.locator('.nr-content').innerText()).includes(mention.entry.text));
      const tasks = fixture.items.filter(item => item.kindId === 'task' || item.plan?.sections.some(section => section.lines.some(line => !line.done)));
      await page.getByRole('tab', { name: /^Tasks/ }).click();
      assert.equal(await page.locator('.nr-task').count(), tasks.length);
      for (const task of tasks) assert.ok((await page.locator('.nr-content').innerText()).includes(task.title));
      assert.equal(await page.getByRole('tab', { name: `Tasks ${tasks.length}`, exact: true }).count(), 1);
      await page.getByRole('tab', { name: /^Approvals/ }).click();
    }
    if (screen === 'profile') {
      const content = page.locator('.native-root-screen[aria-label="Profile"]');
      assert.equal(await content.getByRole('button', { name: `Items ${fixture.stats.items}`, exact: true }).count(), 1);
      assert.equal(await content.getByRole('button', { name: `Chats ${fixture.spaces.length}`, exact: true }).count(), 1);
      for (const agent of fixture.agents) assert.equal(await content.getByRole('img', { name: agent.persona.name, exact: true }).count(), agent.persona.isMine ? 1 : 0);
      assert.equal(await content.getByRole('button', { name: 'Restart the demo', exact: true }).isDisabled(), true);
    }
    if (screen === 'settings') {
      await page.getByRole('searchbox', { name: 'Search settings' }).fill('Theme');
      assert.equal(await page.locator('.ns-row').count(), 1);
      await page.getByRole('searchbox', { name: 'Search settings' }).fill('');
      await page.locator('.ns-row').filter({ hasText: 'Theme' }).click();
      await page.locator('.zw-account select[aria-label="Appearance"]').selectOption(appearance);
      assert.equal(await page.evaluate(() => document.documentElement.dataset.appearance), appearance);
      await page.getByRole('button', { name: 'Back to Settings' }).click();
    }
    if (screen === 'onboarding') assert.equal(await page.getByRole('button', { name: 'Hi, Zoen!', exact: true }).isEnabled(), true);
    assert.equal(await page.locator('img').evaluateAll(images => images.every(image => image.complete && image.naturalWidth > 0)), true, 'all actual native art loaded');
    const image = resolve(args.output, `web-${viewport}-${appearance}-${screen}.png`);
    await page.screenshot({ path: image });
    if (screen === 'chats') {
      await page.getByRole('button', { name: 'Marketplace', exact: true }).click();
      assert.equal(await page.locator('.zm-main [aria-label="Marketplace"]').count(), 1);
      assert.equal(await page.locator('[data-testid="spaces-directory"]').count(), 0);
      await page.getByRole('button', { name: 'Chats', exact: true }).click();
      // The real card opens the same full notification component used by the fixture screen.
      await page.locator(viewport === 'wide' ? '.na-chat-sidebar .nw-notifications' : '.zm-sidebar .nw-notifications').click();
      assert.equal(await page.locator('.native-root-screen[aria-label="Notifications"]').count(), 1);
    }
    assert.deepEqual(errors, [], 'no uncaught browser errors');
    console.log(`PASS ${viewport}/${appearance}/${screen} ${image}`);
    await context.close();
  }
} finally { clearTimeout(deadline); await browser?.close(); }
