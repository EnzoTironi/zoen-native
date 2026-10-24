import assert from 'node:assert/strict';
import test from 'node:test';
import { fixture } from './page-duplicate-route.fixtures.ts';

test('the actual Page Markdown action selects the whole document; Add block keeps append semantics', () => {
  const f = fixture();
  const tree = f.render();
  f.button(tree, 'documents.editMarkdown').props.onPress();
  f.button(tree, 'documents.addABlock').props.onPress();
  // Babel/React Compiler executes the production route; Core/native behavior is
  // covered separately, rather than inferred from this navigation intent.
  assert.deepEqual(JSON.parse(JSON.stringify(f.pushes)), [
    {
      pathname: '/page-block/[id]',
      params: { id: 'page', block: 'new', mode: 'markdown', target: 'document' },
    },
    { pathname: '/page-block/[id]', params: { id: 'page', block: 'new' } },
  ]);
  assert.equal(f.commands.length, 0, 'opening an editor never mutates the document');
  f.retire();
});
