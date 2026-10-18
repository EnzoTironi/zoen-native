import test from 'node:test';
import assert from 'node:assert/strict';
import type { Tool } from '@modelcontextprotocol/client';
import { excalidrawActionDecision } from './remote-grants.ts';

const tool = (name: string, annotations?: Tool['annotations']): Tool => ({
  name,
  inputSchema: { type: 'object' },
  ...(annotations ? { annotations } : {}),
});
const checkpoint = 'owned-current-checkpoint';

test('explicitly consented current-checkpoint save and read stay in this session despite absent write hints', () => {
  for (const name of ['save_checkpoint', 'read_checkpoint']) {
    assert.equal(excalidrawActionDecision(tool(name), { id: checkpoint }, checkpoint), 'session');
    assert.equal(
      excalidrawActionDecision(
        tool(name, { readOnlyHint: false }),
        { id: checkpoint, elements: '[]' },
        checkpoint,
      ),
      'session',
    );
  }
});

test('a foreign checkpoint is rejected even when its advertised operation claims to be read-only', () => {
  for (const name of ['save_checkpoint', 'read_checkpoint'])
    assert.throws(() =>
      excalidrawActionDecision(
        tool(name, { readOnlyHint: true, destructiveHint: false }),
        { id: 'foreign-checkpoint' },
        checkpoint,
      ),
    );
});

test('nested, missing, inherited and cosmetically similar IDs cannot stand in for the exact current checkpoint', () => {
  const read = tool('read_checkpoint');
  for (const args of [
    {},
    { checkpoint: { id: checkpoint } },
    { id: `${checkpoint} ` },
    { id: { value: checkpoint } },
    Object.create({ id: checkpoint }),
  ])
    assert.throws(() => excalidrawActionDecision(read, args, checkpoint));
  assert.throws(() => excalidrawActionDecision(read, { id: '' }, ''));
});

test('export always requires explicit publication approval even if the server calls it read-only and harmless', () => {
  for (const annotations of [
    undefined,
    { readOnlyHint: true, destructiveHint: false },
    { readOnlyHint: false, destructiveHint: true },
  ])
    assert.equal(
      excalidrawActionDecision(
        tool('export_to_excalidraw', annotations),
        { elements: '[]' },
        checkpoint,
      ),
      'publish',
    );
});

test('destructive hints require confirmation even on an otherwise trusted current-checkpoint action or read-only tool', () => {
  for (const name of ['save_checkpoint', 'read_checkpoint', 'inspect_drawing'])
    assert.equal(
      excalidrawActionDecision(
        tool(name, { readOnlyHint: true, destructiveHint: true }),
        { id: checkpoint },
        checkpoint,
      ),
      'confirm',
    );
});

test('unknown app-visible tools require confirmation unless they explicitly declare read-only behavior', () => {
  const args = { drawing: { id: checkpoint } };
  assert.equal(excalidrawActionDecision(tool('new_action'), args, checkpoint), 'confirm');
  assert.equal(
    excalidrawActionDecision(
      tool('new_action', { readOnlyHint: false, destructiveHint: false }),
      args,
      checkpoint,
    ),
    'confirm',
  );
  assert.equal(
    excalidrawActionDecision(tool('inspect_drawing', { readOnlyHint: true }), args, checkpoint),
    'session',
  );
});
