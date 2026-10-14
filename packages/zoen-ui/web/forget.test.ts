import test from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ForgetDialog } from './forget.ts';

function shown(language: string, count: number): string {
  return renderToStaticMarkup(createElement(ForgetDialog, { language, count, open: true, keep: () => {}, forget: () => {} }));
}

test('a Portuguese person sees how many conversations leave this browser and that the phone keeps them', () => {
  const html = shown('pt-BR', 3);
  assert.match(html, /Esquecer este navegador\?/);
  assert.match(html, /As 3 conversas guardadas aqui vão sair deste navegador\./);
  assert.match(html, /No celular tudo continua\./);
  assert.match(html, />Manter<\/button>/);
  assert.match(html, />Esquecer<\/button>/);
  assert.doesNotMatch(html, /vault|snapshot|core/i);
});

test('one or no stored conversation reads naturally in both languages', () => {
  assert.match(shown('pt-BR', 1), /A conversa guardada aqui vai sair deste navegador\./);
  assert.match(shown('pt-BR', 0), /Nenhuma conversa está guardada aqui\./);
  assert.match(shown('en-US', 1), /The conversation stored here will leave this browser\./);
  assert.match(shown('en-US', 4), /The 4 conversations stored here will leave this browser\./);
});

test('keeping the browser is the first choice so a stray Enter erases nothing', () => {
  const html = shown('pt-BR', 2);
  assert.ok(html.indexOf('Manter') < html.indexOf('>Esquecer<'));
  assert.match(html, /autofocus=""[^>]*>Manter/);
});
