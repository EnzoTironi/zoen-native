import test from 'node:test';
import assert from 'node:assert/strict';
import {
  browserAuthContext,
  mountGoogleButton,
  type GoogleCredential,
  type GoogleIdentity,
} from './browser-providers.ts';
import type { AuthTicket } from './attempt';

test('credential SDK accepts only the current owned button callback/state; no window message or stale source can publish', () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const browser = {
    isSecureContext: true,
    crossOriginIsolated: false,
    top: null as unknown,
    self: null as unknown,
  };
  browser.top = browser;
  browser.self = browser;
  Object.defineProperty(globalThis, 'window', { configurable: true, value: browser });
  try {
    let callback: (response: GoogleCredential) => void = () => {};
    let nonce = '';
    let state = '';
    let canceled = 0;
    let accepted = 0;
    let errors = 0;
    const sdk: GoogleIdentity = {
      initialize: (options) => {
        nonce = options.nonce;
        callback = options.callback;
        assert.equal(options.use_fedcm_for_button, true);
        assert.equal(options.button_auto_select, false);
      },
      renderButton: (_element, options) => {
        state = options.state;
      },
      cancel: () => {
        canceled++;
      },
    };
    const controller = new AbortController();
    const ticket: AuthTicket = {
      provider: 'google',
      client: 'web',
      challenge: {
        challenge_id: 'a'.repeat(64),
        nonce: 'b'.repeat(64),
        state: 'c'.repeat(64),
        audience: 'audience',
        expires_at_ms: Date.now() + 300_000,
      },
      signal: controller.signal,
    };
    const element = { replaceChildren() {} } as unknown as HTMLElement;
    const removeFirst = mountGoogleButton(
      sdk,
      element,
      ticket,
      false,
      312,
      () => {
        accepted++;
      },
      () => {
        errors++;
      },
    );
    const staleCallback = callback;
    const removeCurrent = mountGoogleButton(
      sdk,
      element,
      ticket,
      false,
      312,
      () => {
        accepted++;
      },
      () => {
        errors++;
      },
    );
    assert.equal(nonce, ticket.challenge.nonce);
    assert.equal(state, ticket.challenge.state);
    staleCallback({ credential: 'stale', state });
    assert.equal(accepted, 0);
    callback({ credential: 'foreign', state: 'wrong' });
    assert.equal(errors, 1);
    assert.equal(accepted, 0);
    callback({ credential: 'current', state });
    callback({ credential: 'duplicate', state });
    assert.equal(accepted, 1);
    removeFirst();
    assert.equal(canceled, 0);
    removeCurrent();
    assert.equal(canceled, 1);
    callback({ credential: 'after-teardown', state });
    assert.equal(accepted, 1);
    browser.crossOriginIsolated = true;
    assert.throws(browserAuthContext, /tela de entrada/);
    browser.crossOriginIsolated = false;
    browser.top = {};
    assert.throws(browserAuthContext, /própria aba/);
  } finally {
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  }
});
