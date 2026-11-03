import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';

test('actual FlashList autoscroll yields queued and pending work to reading intent', async () => {
  const compiled = await build({
    entryPoints: ['node_modules/@shopify/flash-list/dist/recyclerview/hooks/useBoundDetection.js'],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    external: ['react'],
    write: false,
  });
  const root = createReactTestRoot();
  const frames = new Map<number, (at: number) => void>();
  let nextFrame = 0;
  const module = { exports: {} as any };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.equal(name, 'react');
      return React;
    },
    global: {
      requestAnimationFrame: (callback: (at: number) => void) => {
        frames.set(++nextFrame, callback);
        return nextFrame;
      },
    },
    cancelAnimationFrame: (id: number) => frames.delete(id),
  });
  const policy: { disabled?: boolean; autoscrollToBottomThreshold?: number } = {
    autoscrollToBottomThreshold: 0.12,
  };
  const manager = {
    props: { data: [1], maintainVisibleContentPosition: policy },
    isOffsetProjectionEnabled: true,
    firstItemOffset: 0,
    ignoreScrollEvents: false,
    hasLayout: () => true,
    getIsFirstLayoutComplete: () => true,
    getAbsoluteLastScrollOffset: () => 400,
    getWindowSize: () => ({ width: 400, height: 600 }),
    getChildContainerDimensions: () => ({ width: 400, height: 1000 }),
  };
  let checkBounds: () => void;
  let scrolls = 0;
  const scroll = { current: { scrollToEnd: () => scrolls++ } };
  function Probe() {
    checkBounds = module.exports.useBoundDetection(manager, scroll).checkBounds;
    return null;
  }
  const render = async () => {
    manager.props.data = [...manager.props.data];
    await root.render(React.createElement(Probe));
  };
  const flush = () => {
    const queued = [...frames.values()];
    frames.clear();
    queued.forEach((callback) => callback(0));
  };
  try {
    await render();
    await root.act(() => checkBounds());
    await render();
    assert.equal(frames.size, 1);
    flush();
    assert.equal(scrolls, 1, 'ordinary bottom-follow still works');

    await root.act(() => checkBounds());
    await render();
    assert.equal(frames.size, 1);
    policy.disabled = true;
    flush();
    assert.equal(scrolls, 1, 'quote expansion must revoke an already queued scroll');

    policy.disabled = false;
    await root.act(() => checkBounds());
    delete policy.autoscrollToBottomThreshold;
    await render();
    assert.equal(frames.size, 0, 'reading intent must clear the pending scroll');
    policy.autoscrollToBottomThreshold = 0.12;
    await render();
    flush();
    assert.equal(scrolls, 1, 'restoring policy cannot revive old pending work');

    await root.act(() => checkBounds());
    await render();
    delete policy.autoscrollToBottomThreshold;
    flush();
    assert.equal(scrolls, 1, 'late callbacks must use the current reading policy');
    policy.autoscrollToBottomThreshold = 0.12;
    await root.act(() => checkBounds());
    await render();
    flush();
    assert.equal(scrolls, 2, 'a new scroll intent can resume ordinary following');
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
