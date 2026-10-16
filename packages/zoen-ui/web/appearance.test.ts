import test from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
import { applyAppearance } from './appearance.ts';
test('production appearance preference applies explicit mode and restores system mode', t => {
  const dom = new JSDOM('<!doctype html><html></html>');
  const original = Object.getOwnPropertyDescriptor(globalThis, 'document');
  Object.defineProperty(globalThis, 'document', { configurable: true, value: dom.window.document });
  t.after(() => { if (original) Object.defineProperty(globalThis, 'document', original); else Reflect.deleteProperty(globalThis, 'document'); dom.window.close(); });
  applyAppearance('dark'); assert.equal(document.documentElement.dataset.appearance, 'dark'); assert.equal(document.documentElement.style.colorScheme, 'dark');
  applyAppearance('light'); assert.equal(document.documentElement.dataset.appearance, 'light');
  applyAppearance('system'); assert.equal(document.documentElement.dataset.appearance, undefined); assert.equal(document.documentElement.style.colorScheme, 'light dark');
});
