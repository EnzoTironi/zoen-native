import assert from 'node:assert/strict';
import { test } from 'node:test';
import { elements, fixture, message } from './message-accessibility.fixtures.ts';

function text(value: unknown): string {
  if (Array.isArray(value)) return value.map(text).join('');
  if (value && typeof value === 'object' && 'props' in value)
    return text((value as { props: { children?: unknown } }).props.children);
  return typeof value === 'string' ? value : '';
}

for (const platform of ['ios', 'web'] as const) {
  test(`${platform} compiled conversation preserves the original display, speaks it and clears it on recycled/removed rows`, async () => {
    const entry = message({ forwarded: true, forwarded_author: 'Original 雨 😀' });
    const f = fixture(entry, false, platform);
    try {
      let output = await f.enable();
      const name = elements(output.tree).find(
        (node) => node.type === 'Text' && node.props.children === '\nOriginal 雨 😀',
      );
      assert.ok(name, 'original attribution has a real text leaf');
      assert.equal((name.props.style as { fontWeight: string }).fontWeight, '500');
      assert.ok(String(output.group.props.accessibilityLabel).includes('Original 雨 😀'));
      assert.equal(
        name.props.onPress,
        undefined,
        'a name snapshot does not grant profile navigation',
      );
      output = f.recycle(message({ id: 'legacy', forwarded: true }));
      assert.ok(!text(output.tree).includes('Original 雨'));
      assert.ok(!String(output.group.props.accessibilityLabel).includes('Original 雨'));
      assert.ok(text(output.tree).includes('Forwarded'));
      output = f.recycle({ ...entry, id: 'removed', deleted: true });
      assert.ok(!text(output.tree).includes('Original 雨'));
      assert.ok(!String(output.group.props.accessibilityLabel).includes('Original 雨'));
      output = f.recycle(
        message({ id: 'other', forwarded: true, forwarded_author: 'Next author' }),
      );
      assert.ok(text(output.tree).includes('Next author'));
      assert.ok(!text(output.tree).includes('Original 雨'));
    } finally {
      f.unmount();
    }
  });
}
