import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as React from 'react';
import {
  useRenderedMessageText,
  type RenderedMessageText,
} from '../components/use-rendered-message-text.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';

test('only the current native source/revision can supply a grouped or selection label', async () => {
  const root = createReactTestRoot();
  let owner = 'space:message',
    markdown = String.raw`QA758 café\! \*literal\*`;
  let current!: RenderedMessageText;
  function Row() {
    const value = useRenderedMessageText(owner, markdown, true);
    React.useLayoutEffect(() => {
      current = value;
    }, [value]);
    return null;
  }
  const render = () => root.render(React.createElement(Row));
  try {
    await render();
    assert.equal(current.text, undefined, 'never speak raw Markdown while awaiting the native AST');
    const old = current;
    await root.act(() =>
      current.accept({ revision: current.revision, markdown: 'foreign', text: 'wrong' }),
    );
    assert.equal(current.text, undefined);
    await root.act(() => current.accept({ revision: 'foreign revision', markdown, text: 'wrong' }));
    assert.equal(current.text, undefined);
    await root.act(() =>
      current.accept({ revision: current.revision, markdown, text: 'QA758 café! *literal*' }),
    );
    assert.equal(current.text, 'QA758 café! *literal*');
    markdown = '**new revision**';
    await render();
    assert.equal(current.text, undefined);
    await root.act(() =>
      old.accept({
        revision: old.revision,
        markdown: String.raw`QA758 café\! \*literal\*`,
        text: 'old',
      }),
    );
    assert.equal(current.text, undefined);
    const beforeRecycle = current;
    owner = 'space:recycled-message';
    await render();
    await root.act(() =>
      current.accept({
        revision: beforeRecycle.revision,
        markdown,
        text: 'same source, retired owner',
      }),
    );
    assert.equal(current.text, undefined);
    await root.act(() =>
      current.accept({ revision: current.revision, markdown, text: 'new revision' }),
    );
    assert.equal(current.text, 'new revision');
    const retired = current;
    await root.close();
    retired.accept({ revision: retired.revision, markdown, text: 'late after unmount' });
    assert.deepEqual(root.errors, []);
  } catch (error) {
    await root.close();
    throw error;
  }
});

test('ordinary text stays exact and a current row shares one native receipt with its selection wrapper', async () => {
  const root = createReactTestRoot();
  let plain!: RenderedMessageText, parent!: RenderedMessageText, child!: RenderedMessageText;
  function Row() {
    const plainValue = useRenderedMessageText('plain', 'C:\\Temp\\note.txt', false);
    const parentValue = useRenderedMessageText('shared', '**body**', true);
    const childValue = useRenderedMessageText('shared', '**body**', true, parentValue);
    React.useLayoutEffect(() => {
      plain = plainValue;
      parent = parentValue;
      child = childValue;
    }, [plainValue, parentValue, childValue]);
    return null;
  }
  try {
    await root.render(React.createElement(Row));
    assert.equal(plain.text, 'C:\\Temp\\note.txt');
    assert.equal(child, parent);
    await root.act(() =>
      child.accept({ revision: child.revision, markdown: '**body**', text: 'body' }),
    );
    assert.equal(child, parent);
    assert.equal(parent.text, 'body');
  } finally {
    await root.close();
  }
});
