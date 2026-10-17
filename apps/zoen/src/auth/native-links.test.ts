import assert from 'node:assert/strict';
import { test } from 'node:test';
import { nativeProviderReturnPath } from './native-links.ts';

test('native Google return resumes the protected root without routing credentials into a screen', () => {
  const client = '123-example.apps.googleusercontent.com';
  const callback =
    'com.googleusercontent.apps.123-example:/oauth2callback?code=synthetic&state=opaque';
  assert.equal(nativeProviderReturnPath(callback, client), '/');
  assert.equal(nativeProviderReturnPath(callback, undefined), null);
  for (const value of [
    'com.googleusercontent.apps.456-other:/oauth2callback?code=synthetic',
    'com.googleusercontent.apps.123-example://foreign/oauth2callback',
    'com.googleusercontent.apps.123-example:/chat/private',
    'https://tryzoen.com/oauth2callback?code=synthetic',
    'malformed input',
  ])
    assert.equal(nativeProviderReturnPath(value, client), null);
});
