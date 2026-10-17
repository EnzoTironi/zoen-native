import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  resolveLocale,
  sessionLocale,
  formatLocale as resolvedFormatLocale,
} from '../../../../packages/zoen-ui/web/locale.ts';
import { formatLocale, initializeLocalization, locale } from '../i18n/index.ts';
import { createLocalization } from '../i18n/runtime.ts';
import { defineFeature } from '../i18n/catalog.ts';

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
