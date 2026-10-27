import assert from "node:assert/strict";
import test from "node:test";
import { fixture, documentPort, current } from "./native-host-navigation-fixture.mjs";

for (const origin of ["home", "chat"])
  test(`Expo default Back waits for actual screen flush and returns to ${origin}`, async () => {
    const doc = documentPort();
    const f = await fixture("android", { controller: doc.controller, origin });
    try {
      assert.equal(f.qa.backHandlers.size, 1);
      assert.ok(f.qa.prevented.has("drawing"));
      assert.ok(
        f.node.querySelector("[data-qa-local-safe-provider] [data-qa-editor]"),
        "actual App layout places editor inside region-local native provider",
      );
      await f.act(() => assert.equal(f.back(), true));
      assert.equal(current(f).name, "miniapp");
      assert.deepEqual(doc.calls, [{ retire: true }]);
      assert.equal(f.qa.disposedDocuments, 0);
      await f.act(() => assert.equal(f.back(), true));
      assert.equal(doc.calls.length, 1, "duplicate Back shares actual screen navigation lock");
      await f.act(() => doc.pending[0].resolve());
      assert.equal(current(f).name, origin);
      assert.equal(f.qa.disposedDocuments, 1);
      assert.equal(f.qa.nativeFallbacks, 0);
      assert.equal(f.qa.backHandlers.size, 1, "root listener survives App dismissal");
    } finally {
      await f.close();
    }
  });
test("actual failed flush retains App, exposes close error, and allows successful Back retry", async () => {
  const doc = documentPort();
  const f = await fixture("android", { controller: doc.controller });
  try {
    await f.act(() => f.back());
    await f.act(() => doc.pending[0].reject(Error("Synthetic save failure")));
    assert.equal(current(f).name, "miniapp");
    assert.equal(f.qa.disposedDocuments, 0);
    assert.ok(f.qa.prevented.has("drawing"));
    assert.match(f.node.textContent, /drawing.closeFailed/);
    await f.act(() => f.back());
    assert.equal(doc.calls.length, 2);
    assert.equal(current(f).name, "miniapp");
    await f.act(() => doc.pending[1].resolve());
    assert.equal(current(f).name, "home");
    assert.equal(f.qa.nativeFallbacks, 0);
  } finally {
    await f.close();
  }
});
test("Expo default Back closes nested management before flushing the App", async () => {
  const doc = documentPort();
  const f = await fixture("android", { controller: doc.controller, management: true });
  try {
    await f.act(() => f.back());
    assert.equal(current(f).name, "miniapp");
    assert.equal(current(f).state.index, 0);
    assert.equal(doc.calls.length, 0);
    await f.act(() => f.back());
    assert.equal(doc.calls.length, 1);
    assert.equal(current(f).name, "miniapp");
    await f.act(() => doc.pending[0].resolve());
    assert.equal(current(f).name, "home");
  } finally {
    await f.close();
  }
});
test("root Back falls through to native after the App has closed", async () => {
  const f = await fixture("android", { loading: true });
  try {
    await f.act(() => assert.equal(f.back(), true));
    assert.equal(current(f).name, "home");
    assert.equal(f.back(), false);
    assert.equal(f.qa.nativeFallbacks, 1);
  } finally {
    await f.close();
    assert.equal(f.qa.backHandlers.size, 0, "root listener cleans up on unmount");
  }
});
test("Web uses Expo's no-op Back listener", async () => {
  const doc = documentPort();
  const f = await fixture("web", { controller: doc.controller });
  try {
    assert.equal(f.qa.backHandlers.size, 0);
    assert.equal(f.back(), false);
    assert.equal(doc.calls.length, 0);
  } finally {
    await f.close();
  }
});
test("App stack Back also consumes while its document is still loading", async () => {
  const f = await fixture("android", { loading: true });
  try {
    assert.ok(f.node.querySelector("[data-qa-loading]"));
    await f.act(() => assert.equal(f.back(), true));
    assert.equal(current(f).name, "home");
  } finally {
    await f.close();
  }
});
test("default Back defers to native while the navigation ref is unavailable", async () => {
  const doc = documentPort();
  const f = await fixture("android", { controller: doc.controller });
  try {
    f.qa.rootRef.current = null;
    assert.equal(f.back(), false);
    assert.equal(f.qa.nativeFallbacks, 1);
    assert.equal(current(f).name, "miniapp");
    assert.equal(doc.calls.length, 0);
  } finally {
    await f.close();
  }
});

function pressClose(f) {
  const button = f.node.querySelector("[data-qa-close]");
  assert.ok(button, "actual App layout mounts its native close component");
  button.click();
}
for (const platform of ["ios", "android"]) {
  test(`${platform} floating close uses actual root removal guard and preserves full App safe insets`, async () => {
    const doc = documentPort();
    const f = await fixture(platform, { controller: doc.controller, origin: "chat" });
    try {
      assert.deepEqual(
        f.qa.editorInsets,
        { top: 24, right: 0, bottom: 16, left: 0 },
        "no global 48-point row",
      );
      assert.deepEqual(f.qa.editorChrome, { top: 32, right: 12, width: 44, height: 44 });
      assert.equal(f.qa.closeProps.name, "xmark");
      assert.equal(f.qa.closeProps.size, 44);
      assert.equal(f.qa.closeProps.label, "common.close");
      await f.act(() => pressClose(f));
      assert.equal(current(f).name, "miniapp");
      assert.equal(f.qa.disposedDocuments, 0);
      assert.deepEqual(doc.calls, [{ retire: true }]);
      assert.deepEqual(f.qa.dispatched[0], {
        type: "GO_BACK",
        source: "app-modal",
        target: "root-stack",
      });
      await f.act(() => pressClose(f));
      assert.equal(doc.calls.length, 1, "repeated close shares descendant save lock");
      await f.act(() => doc.pending[0].resolve());
      assert.equal(current(f).name, "chat");
      assert.equal(f.qa.disposedDocuments, 1);
    } finally {
      await f.close();
    }
  });
  test(`${platform} failed close retains document and a new explicit X retries the real flush`, async () => {
    const doc = documentPort();
    const f = await fixture(platform, { controller: doc.controller });
    try {
      await f.act(() => pressClose(f));
      await f.act(() => doc.pending[0].reject(Error("Synthetic durable save failure")));
      assert.equal(current(f).name, "miniapp");
      assert.equal(f.qa.disposedDocuments, 0);
      assert.ok(f.qa.prevented.has("drawing"));
      assert.match(f.node.textContent, /drawing.closeFailed/);
      await f.act(() => pressClose(f));
      assert.equal(doc.calls.length, 2);
      await f.act(() => doc.pending[1].resolve());
      assert.equal(current(f).name, "home");
      assert.equal(f.qa.disposedDocuments, 1);
    } finally {
      await f.close();
    }
  });
}
test("a retained close callback cannot close a refocused or replaced App scope", async () => {
  const doc = documentPort();
  const f = await fixture("ios", { controller: doc.controller });
  try {
    const beforeBlur = f.qa.closeProps.onPress;
    await f.act(() => {
      f.focus(false);
      f.focus(true);
    });
    await f.act(() => beforeBlur());
    assert.deepEqual(f.qa.dispatched, []);
    const beforePath = f.qa.closeProps.onPress;
    await f.act(() => f.path("/miniapp/different-document"));
    await f.act(() => beforePath());
    assert.deepEqual(f.qa.dispatched, []);
    assert.equal(doc.calls.length, 0);
    await f.act(() => pressClose(f));
    assert.equal(doc.calls.length, 1);
    await f.act(() => doc.pending[0].resolve());
    assert.equal(current(f).name, "home");
  } finally {
    await f.close();
  }
});
test("floating close refuses missing state, root mismatch, unfocused route and absent back destination", async () => {
  const doc = documentPort();
  const f = await fixture("ios", { controller: doc.controller });
  try {
    const read = f.qa.navigation.getState,
      canBack = f.qa.navigation.canGoBack;
    f.qa.navigation.getState = () => undefined;
    await f.act(() => pressClose(f));
    f.qa.navigation.getState = read;
    const key = f.qa.routeIdentity.key;
    f.qa.routeIdentity.key = "other-modal";
    await f.act(() => pressClose(f));
    f.qa.routeIdentity.key = key;
    await f.act(() => f.focus(false));
    await f.act(() => pressClose(f));
    await f.act(() => f.focus(true));
    f.qa.navigation.canGoBack = () => false;
    await f.act(() => pressClose(f));
    f.qa.navigation.canGoBack = canBack;
    assert.deepEqual(f.qa.dispatched, []);
    assert.equal(doc.calls.length, 0);
    assert.equal(current(f).name, "miniapp");
  } finally {
    await f.close();
  }
});
test("management shares floating close while nested tool headers keep their native actions", async () => {
  const doc = documentPort();
  const f = await fixture("ios", { controller: doc.controller, management: true });
  try {
    assert.ok(f.node.querySelector("[data-qa-close]"));
    assert.deepEqual(f.qa.editorChrome, { top: 32, right: 12, width: 44, height: 44 });
    await f.act(() => f.path("/miniapp/synthetic-drawing/tool"));
    assert.equal(f.node.querySelector("[data-qa-close]"), null);
    assert.equal(f.qa.editorChrome, null);
    await f.act(() => f.back());
    assert.equal(current(f).state.index, 0);
    assert.equal(doc.calls.length, 0);
    await f.act(() => f.path("/miniapp/synthetic-drawing"));
    assert.ok(f.node.querySelector("[data-qa-close]"));
    assert.ok(f.qa.editorChrome);
    await f.act(() => pressClose(f));
    assert.deepEqual(doc.calls, [{ retire: true }]);
    await f.act(() => doc.pending[0].resolve());
    assert.equal(current(f).name, "home");
    assert.equal(f.qa.disposedDocuments, 1);
  } finally {
    await f.close();
  }
});
test("Web App layout has no native X or host corner exclusion", async () => {
  const f = await fixture("web", { controller: documentPort().controller });
  try {
    assert.equal(f.node.querySelector("[data-qa-close]"), null);
    assert.equal(f.qa.editorChrome, null);
    assert.deepEqual(f.qa.editorInsets, { top: 24, right: 0, bottom: 16, left: 0 });
  } finally {
    await f.close();
  }
});
