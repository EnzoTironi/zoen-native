import assert from 'node:assert/strict';
import { test } from 'node:test';
import { navigationScrollFixture } from './navigation-scroll-view.fixture.ts';

test('native header binds this screen scroll node and keeps one system header with conversation material', () => {
  const f = navigationScrollFixture();
  let layout = 0;
  const ref = { current: null };
  const props = {
    title: 'Agent',
    ref,
    onLayout: () => layout++,
    children: 'body',
    keyboardShouldPersistTaps: 'always',
  };
  let tree = f.render(props);
  const node = { getScrollableNode: () => 741 };
  tree.props.ref(node);
  tree.props.onLayout({ nativeEvent: { target: 999 } });
  tree = f.render(props);
  assert.equal(ref.current, node);
  assert.equal(layout, 1);
  assert.equal(tree.props.keyboardShouldPersistTaps, 'always');
  const screen = tree.props.children[0];
  assert.equal(screen.type, 'Screen');
  assert.equal(screen.props.options.header, undefined);
  assert.equal(screen.props.options.headerLeft, undefined);
  assert.equal(screen.props.options.gestureEnabled, undefined);
  assert.equal(screen.props.options.scrollEdgeEffects.top, 'automatic');
  const title = screen.props.options.headerTitle();
  assert.equal(title.type, 'NavigationContainer');
  assert.equal(title.props.scrollViewTag, 741);
  assert.equal(title.props.children.type, 'NavigationGlass');
  assert.equal(title.props.children.props.radius, 22);
  tree.props.ref(null);
  assert.equal(
    f.render(props).props.children[0].props.options.headerTitle().props.scrollViewTag,
    0,
  );
});

test('unresolved native scroll nodes do not borrow the layout event target', () => {
  const f = navigationScrollFixture();
  for (const value of [undefined, -1, 0, 1.5, NaN]) {
    const tree = f.render({ title: 'Loading' });
    tree.props.ref({ getScrollableNode: () => value });
    tree.props.onLayout({ nativeEvent: { target: 999 } });
    assert.equal(
      f.render({ title: 'Loading' }).props.children[0].props.options.headerTitle().props
        .scrollViewTag,
      0,
    );
  }
});

test('other platforms retain their native title and caller layout callback', () => {
  const f = navigationScrollFixture('android');
  let layout = 0;
  const tree = f.render({ title: 'Settings', onLayout: () => layout++ });
  tree.props.ref({
    getScrollableNode: () => {
      throw Error('iOS binding only');
    },
  });
  tree.props.onLayout({});
  assert.equal(layout, 1);
  assert.equal(tree.props.children[0].props.options.headerTitle, undefined);
  assert.equal(tree.props.children[0].props.options.title, 'Settings');
});
