import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  resolveLocale,
  sessionLocale,
  formatLocale as resolvedFormatLocale,
} from '../../../../packages/zoen-ui/web/locale.ts';
import { formatLocale, initializeLocalization, locale } from '../i18n/index.ts';
import { createLocalization } from '../i18n/runtime.ts';
import { assertCatalogParity, defineFeature } from '../i18n/catalog.ts';
import { activity } from '../i18n/catalogs/activity.ts';
import { chat } from '../i18n/catalogs/chat.ts';
import { conversationCards } from '../i18n/catalogs/conversation-cards.ts';
import { documents } from '../i18n/catalogs/documents.ts';
import { home } from '../i18n/catalogs/home.ts';
import { miniapps } from '../i18n/catalogs/miniapps.ts';
import { nativeSurfaces } from '../i18n/catalogs/nativeSurfaces.ts';

test('one canonical session language follows ordered supported preferences', () => {
  assert.equal(resolveLocale(['fr-FR', 'pt-PT', 'en-US']), 'pt-BR');
  assert.equal(resolveLocale(['es', 'en-GB', 'pt-BR']), 'en');
  assert.equal(resolveLocale(['pt_PT']), 'pt-BR');
  assert.equal(resolveLocale(['ptx', 'x-pt']), 'en');
  assert.equal(resolveLocale([]), 'en');
  assert.equal(resolvedFormatLocale('en'), 'en-US');
  assert.throws(() => sessionLocale('en-US'), /Invalid session language/);
  assert.throws(() => sessionLocale(undefined), /Invalid session language/);
});

test('Node bootstrap selects the same immutable pure language singleton', () => {
  assert.equal(locale, 'en');
  assert.equal(formatLocale, 'en-US');
  initializeLocalization('en');
  assert.throws(() => initializeLocalization('pt-BR'), /cannot change/);
  assert.equal(locale, 'en');
});

test('independent translations preserve exact counts and user content', () => {
  const fixture = defineFeature(
    {
      report: '%{title}: %{events}',
      unread: { one: '%{count} message', other: '%{count} messages' },
    },
    {
      report: '%{title}: %{events}',
      unread: { one: '%{count} mensagem', other: '%{count} mensagens' },
    },
  );
  const en = createLocalization({ fixture }, ['en']);
  const pt = createLocalization({ fixture }, ['pt-PT']);
  const input = { title: '<b>%{value}</b>', events: '9223372036854775807' };
  assert.equal(en.t('fixture.report', input), '<b>%{value}</b>: 9223372036854775807');
  assert.equal(pt.t('fixture.report', input), en.t('fixture.report', input));
  assert.equal(en.t('fixture.unread', { count: 0 }), '0 messages');
  assert.equal(pt.t('fixture.unread', { count: 0 }), '0 mensagem');
  assert.equal(en.t('fixture.unread', { count: 2 }), '2 messages');
  assert.equal(pt.t('fixture.unread', { count: 2 }), '2 mensagens');
});

test('Apps terminology stays consistent across English and Portuguese product copy', () => {
  const catalogs = { activity, chat, conversationCards, documents, home, miniapps, nativeSurfaces };
  const check = (value: unknown) => {
    if (typeof value === 'string') assert.doesNotMatch(value, /\bmini[ -]?apps?\b/i);
    else for (const child of Object.values(value as Record<string, unknown>)) check(child);
  };
  for (const catalog of Object.values(catalogs)) {
    assertCatalogParity(catalog);
    check(catalog);
  }
  const en = createLocalization(catalogs, ['en']);
  const pt = createLocalization(catalogs, ['pt-BR']);
  for (const language of [en, pt]) {
    assert.equal(language.t('home.miniapps'), 'Apps');
    assert.equal(language.t('miniapps.miniapps'), 'Apps');
  }
  assert.equal(en.t('home.addMiniapp'), 'Add an app');
  assert.equal(pt.t('home.addMiniapp'), 'Adicionar app');
  assert.equal(en.t('miniapps.miniappPermissions'), 'App permissions');
  assert.equal(pt.t('miniapps.miniappPermissions'), 'Permissões dos apps');
});

test('Apps copy retains interpolation and never renames user-owned content', () => {
  const en = createLocalization({ miniapps }, ['en']);
  const pt = createLocalization({ miniapps }, ['pt-BR']);
  const chat = 'My miniapp project';
  assert.equal(
    en.t('miniapps.accessScope', { chat }),
    `Only this app, in ${chat}. Nothing goes to the group unless you share it.`,
  );
  assert.equal(
    pt.t('miniapps.accessScope', { chat }),
    `Só este app, em ${chat}. Nada vai para o grupo sem você compartilhar.`,
  );
});

test('localization starts without Intl.PluralRules and preserves CLDR plural selection', () => {
  const descriptor = Object.getOwnPropertyDescriptor(Intl, 'PluralRules');
  try {
    assert.equal(Reflect.deleteProperty(Intl, 'PluralRules'), true);
    const fixture = defineFeature(
      { unread: { one: '%{count} message', other: '%{count} messages' } },
      { unread: { one: '%{count} mensagem', other: '%{count} mensagens' } },
    );
    const en = createLocalization({ fixture }, ['en']);
    const pt = createLocalization({ fixture }, ['pt-BR']);
    const counts = [0, 1, 2, 1_000_000];
    assert.deepEqual(
      counts.map((count) => en.t('fixture.unread', { count })),
      ['0 messages', '1 message', '2 messages', '1000000 messages'],
    );
    assert.deepEqual(
      counts.map((count) => pt.t('fixture.unread', { count })),
      ['0 mensagem', '1 mensagem', '2 mensagens', '1000000 mensagens'],
    );
    assert.equal(
      Reflect.get(Intl, 'PluralRules'),
      undefined,
      'startup must not install a polyfill',
    );
  } finally {
    if (descriptor) Object.defineProperty(Intl, 'PluralRules', descriptor);
    else Reflect.deleteProperty(Intl, 'PluralRules');
  }
  assert.deepEqual(Object.getOwnPropertyDescriptor(Intl, 'PluralRules'), descriptor);
});
