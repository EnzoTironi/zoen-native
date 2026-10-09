import { test } from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { mkdir, readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';

const here = dirname(fileURLToPath(import.meta.url));
await mkdir(join(here, 'dist'), { recursive: true });
const outfile = join(here, 'dist', 'shell.server.mjs');
const result = await build({
  stdin: {
    contents: `export { ZoenShell, ShellIcon } from '@zoen/ui/shell';
      export { CommunityResources, InboxFilters, filterInbox, navigation, sampleChats } from './inbox';
      export { SampleActivity, ActivityUpdates } from './activity';`,
    resolveDir: here,
    loader: 'ts',
  },
  bundle: true,
  format: 'esm',
  platform: 'node',
  jsx: 'automatic',
  external: ['react', 'react/jsx-runtime'],
  outfile,
  metafile: true,
});
const {
  ZoenShell,
  ShellIcon,
  CommunityResources,
  InboxFilters,
  filterInbox,
  navigation,
  sampleChats,
  SampleActivity,
  ActivityUpdates,
} = await import(pathToFileURL(outfile).href);
const props = {
  navigation: [
    { id: 'chats', label: 'Chats', icon: createElement(ShellIcon, { name: 'chats' }), badge: 7 },
  ],
  activeNavigationId: 'chats',
  onNavigate() {},
  chats: [{ id: 'marina', title: 'Marina', preview: 'A sample chat', unreadCount: 3 }],
  activeChatId: 'marina',
  onSelectChat() {},
  searchQuery: '',
  onSearchChange() {},
  theme: 'system',
  onThemeChange() {},
  title: 'Marina',
  children: createElement('h1', null, 'Host content'),
};

test('the exported shell imports and renders without a browser or mini-app SDK', () => {
  assert.equal(typeof globalThis.window, 'undefined');
  assert.ok(
    Object.keys(result.metafile.inputs).every((path) => !path.includes('zoen-miniapp-sdk')),
  );
  const html = renderToStaticMarkup(createElement(ZoenShell, props));
  assert.match(html, /data-theme="system"/);
  assert.match(html, /aria-label="Chats, 7 unread"/);
  assert.match(html, /aria-label="3 unread"/);
  assert.match(html, /aria-expanded="true"/);
  assert.match(html, /<h1>Host content<\/h1>/);
  assert.match(html, /type="search"/);
  assert.match(html, /aria-label="System appearance"[^>]*aria-pressed="true"/);
});

test('the host can select a dark, initially collapsed shell with empty chats', () => {
  const html = renderToStaticMarkup(
    createElement(ZoenShell, {
      ...props,
      theme: 'dark',
      defaultCollapsed: true,
      chats: [],
      emptySidebar: 'No results',
    }),
  );
  assert.match(html, /data-theme="dark"/);
  assert.match(html, /data-collapsed="true"/);
  assert.match(html, /aria-expanded="false"/);
  assert.match(html, /class="zs-sidebar" hidden=""/);
  assert.match(html, /No results/);
  assert.match(html, /<h1>Host content<\/h1>/);
});

test('appearance exposes three direct choices with the host-selected mode centered', () => {
  for (const theme of ['dark', 'system', 'light']) {
    const html = renderToStaticMarkup(createElement(ZoenShell, { ...props, theme }));
    const choices = [...html.matchAll(/<button[^>]*class="zs-theme-target"[^>]*>/g)].map(
      (match) => match[0],
    );
    assert.equal(choices.length, 3);
    assert.equal(choices.filter((choice) => choice.includes('aria-pressed="true"')).length, 1);
    const selected = choices.find((choice) => choice.includes('aria-pressed="true"'));
    assert.match(selected, /data-position="center"/);
    assert.match(
      selected,
      new RegExp(`aria-label="${theme[0].toUpperCase()}${theme.slice(1)} appearance"`),
    );
    assert.match(selected, /tabindex="0"/);
    assert.equal(choices.filter((choice) => choice.includes('tabindex="-1"')).length, 2);
  }
  const system = renderToStaticMarkup(createElement(ZoenShell, props));
  assert.match(system, /data-position="left"[^>]*aria-label="Dark appearance"/);
  assert.match(system, /data-position="right"[^>]*aria-label="Light appearance"/);
});

test('controls stay in the rail and sidebar without an outer banner', () => {
  const html = renderToStaticMarkup(createElement(ZoenShell, props));
  assert.equal(html.includes('<header'), false);
  const rail = html.slice(html.indexOf('class="zs-rail"'), html.indexOf('class="zs-frame"'));
  assert.match(rail, /aria-label="Hide chats"/);
  const sidebar = html.slice(html.indexOf('<aside'), html.indexOf('</aside>'));
  assert.match(sidebar, /type="search"/);
  const content = html.slice(html.indexOf('<main'), html.indexOf('</main>'));
  assert.match(content, /class="zs-context"[^>]*>Marina/);
  assert.match(content, /<h1>Host content<\/h1>/);
});

test('the host can omit content context and mark its Search route as a sidebar control', () => {
  const html = renderToStaticMarkup(
    createElement(ZoenShell, {
      ...props,
      title: undefined,
      defaultCollapsed: true,
      searchNavigationId: 'search',
      navigation: [
        { id: 'search', label: 'Search', icon: createElement(ShellIcon, { name: 'search' }) },
      ],
    }),
  );
  assert.equal(html.includes('class="zs-content-context"'), false);
  assert.match(html, /aria-label="Search"[^>]*aria-expanded="false"[^>]*aria-controls="[^"]+"/);
  assert.match(html, /class="zs-sidebar" hidden=""/);
});

test('the default inbox puts direct, group, and community chats in one Chats navigation', () => {
  assert.equal(
    navigation.some((item) => item.id === 'spaces'),
    false,
  );
  assert.equal(navigation.filter((item) => item.id === 'chats').length, 1);
  const chats = filterInbox(sampleChats, 'all', '');
  assert.deepEqual(
    new Set(chats.map((chat) => chat.kind)),
    new Set(['direct', 'group', 'community']),
  );
  const html = renderToStaticMarkup(createElement(ZoenShell, { ...props, navigation, chats }));
  const rail = html.slice(html.indexOf('class="zs-rail"'), html.indexOf('class="zs-frame"'));
  assert.equal(rail.includes('aria-label="Spaces"'), false);
  const sidebar = html.slice(html.indexOf('<aside'), html.indexOf('</aside>'));
  assert.match(sidebar, /Makers community/);
  assert.match(sidebar, /Marina/);
  assert.match(sidebar, /Saturday crew/);
});

test('inbox filters combine kind and search while keeping the original conversation objects', () => {
  for (const kind of ['direct', 'group', 'community']) {
    const chats = filterInbox(sampleChats, kind, '');
    assert.ok(chats.length > 0);
    assert.ok(chats.every((chat) => chat.kind === kind));
    assert.ok(chats.every((chat) => sampleChats.includes(chat)));
  }
  const makers = sampleChats.find((chat) => chat.id === 'makers');
  assert.deepEqual(filterInbox(sampleChats, 'all', '  MAKERS '), [makers]);
  assert.deepEqual(filterInbox(sampleChats, 'community', 'makers'), [makers]);
  assert.deepEqual(filterInbox(sampleChats, 'direct', 'makers'), []);
  assert.deepEqual(filterInbox(sampleChats, 'all', 'not-a-sample-chat'), []);
  const filters = createElement(InboxFilters, { selected: 'community', onChange() {} });
  const html = renderToStaticMarkup(
    createElement(ZoenShell, { ...props, sidebarFilters: filters }),
  );
  const sidebar = html.slice(html.indexOf('<aside'), html.indexOf('</aside>'));
  assert.match(sidebar, /role="group" aria-label="Filter chats"/);
  assert.match(sidebar, /aria-pressed="true">Communities<\/button>/);
  assert.equal([...sidebar.matchAll(/aria-pressed="true"/g)].length, 1);
  assert.equal([...sidebar.matchAll(/aria-pressed="false"/g)].length, 3);
  assert.ok(sidebar.indexOf('Filter chats') < sidebar.indexOf('class="zs-chat-list"'));
});

test('sample community resources and roles stay inside an expandable conversation card', () => {
  const html = renderToStaticMarkup(createElement(CommunityResources));
  assert.match(html, /<summary>Community resources · Sample<\/summary>/);
  assert.match(html, /<summary>Community guide.txt<\/summary>/);
  assert.match(html, /Sample member roles/);
  assert.match(html, /does not change anyone/);
  assert.equal(html.includes('href='), false);
});

test('Activity opens in sample Cards with List available and a real open-chat control', () => {
  const html = renderToStaticMarkup(createElement(SampleActivity, { onSelectChat() {} }));
  assert.match(html, /Sample updates/);
  assert.match(html, /class="preview-activity-cards"/);
  assert.match(html, /aria-label="Show sample updates as a list">List<\/button>/);
  assert.match(html, /aria-label="Open Design studio conversation">Open chat<\/button>/);
  assert.match(html, /aria-label="Previous sample update" disabled=""/);
  assert.match(html, /aria-label="Next sample update"/);
  assert.equal(html.includes('class="preview-results preview-activity-list"'), false);
});

test('explicit Activity List keeps every sample update and exposes Cards to return', () => {
  const html = renderToStaticMarkup(
    createElement(ActivityUpdates, {
      presentation: 'list',
      onPresentationChange() {},
      onSelectChat() {},
    }),
  );
  assert.match(html, /aria-label="Show sample updates as cards">Cards<\/button>/);
  assert.match(html, /class="preview-results preview-activity-list"/);
  for (const chat of sampleChats.filter((chat) => (chat.unreadCount ?? 0) > 0)) {
    assert.ok(html.includes(chat.title));
  }
  assert.equal(html.includes('class="preview-activity-cards"'), false);
});

test('light, dark, and system colors agree with the native semantic Palette', async () => {
  const [nativeTheme, shellStyles] = await Promise.all([
    readFile(join(here, '../../../apple/Shared/DesignSystem/Theme.swift'), 'utf8'),
    readFile(join(here, '../src/shell.css'), 'utf8'),
  ]);
  const palette = new Map(
    [
      ...nativeTheme.matchAll(
        /static let (\w+) = Color\.adaptive\(light: "(#[\dA-Fa-f]{6})", dark: "(#[\dA-Fa-f]{6})"\)/g,
      ),
    ].map(([, name, light, dark]) => [name, { light, dark }]),
  );
  const lightStyles = shellStyles.match(/\.zs-shell \{([^}]+)\}/)?.[1];
  const darkStyles = shellStyles.match(/\.zs-shell\[data-theme='dark'\] \{([^}]+)\}/)?.[1];
  const systemStyles = shellStyles.match(/\.zs-shell\[data-theme='system'\] \{([^}]+)\}/)?.[1];
  assert.ok(lightStyles && darkStyles && systemStyles);
  const colorValues = (styles) =>
    new Map(
      [...styles.matchAll(/(--[\w-]+):\s*(#[\dA-Fa-f]{6})\s*;/g)].map(([, name, value]) => [
        name,
        value.toUpperCase(),
      ]),
    );
  const light = colorValues(lightStyles);
  const dark = colorValues(darkStyles);
  const system = colorValues(systemStyles);
  const semantics = {
    background: '--zs-bg',
    surface: '--zs-surface',
    surfaceRaised: '--zs-surface-raised',
    surfaceMuted: '--zs-surface-muted',
    hairline: '--zs-border',
    textPrimary: '--zs-ink',
    textSecondary: '--zs-muted',
    textTertiary: '--zs-tertiary',
    action: '--zs-action',
    actionDeep: '--zs-action-deep',
    myBubble: '--zs-bubble-own',
    myBubbleText: '--zs-bubble-own-ink',
    otherBubble: '--zs-bubble-other',
    otherBubbleOnBackdrop: '--zs-bubble-other-on-backdrop',
  };
  for (const [semantic, property] of Object.entries(semantics)) {
    const native = palette.get(semantic);
    assert.ok(native, `Native Palette.${semantic} must exist`);
    assert.equal(light.get(property), native.light.toUpperCase(), `Light ${semantic}`);
    assert.equal(dark.get(property), native.dark.toUpperCase(), `Dark ${semantic}`);
    assert.equal(system.get(property), native.dark.toUpperCase(), `System dark ${semantic}`);
  }
});
