import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire(join(root, "apps/zoen/package.json"));
const { transformSync } = require("esbuild");
const pkg = dirname(require.resolve("react-native-enriched-markdown/package.json"));
const jsx = (type, props) => ({ type, props });
function wrapper(relative, platform = "ios") {
  const filename = join(pkg, relative);
  const code = transformSync(readFileSync(filename, "utf8"), {
    loader: relative.endsWith(".tsx") ? "tsx" : "jsx",
    format: "cjs",
    jsx: "automatic",
    sourcefile: filename,
  }).code;
  const module = { exports: {} };
  const native = (value) => ({ __esModule: true, default: value });
  runInNewContext(code, {
    module,
    exports: module.exports,
    require(name) {
      name = name.replace(/\.js$/, "");
      if (name === "react")
        return {
          useMemo: (read) => read(),
          useCallback: (fn) => fn,
          useRef: (value) => ({ current: value }),
          useEffect() {},
          useState: (value) => [value, () => {}],
        };
      if (name === "react/jsx-runtime") return { jsx, jsxs: jsx };
      if (name === "react-native") return { Platform: { OS: platform } };
      if (name === "../EnrichedMarkdownTextNativeComponent") return native("NativeCommonMark");
      if (name === "../EnrichedMarkdownNativeComponent") return native("NativeGFM");
      if (name === "../normalizeMarkdownStyle" || name === "../normalizeMarkdownStyle.web")
        return { normalizeMarkdownStyle: (value) => value };
      if (name === "../normalizeMenuItem")
        return {
          normalizeMenuItem: () => ({ label: "Copy", enabled: true }),
          normalizeLegacyBooleanMenuItem: () => ({ label: "Copy", enabled: true }),
        };
      if (name === "../accessibilityLabelDefaults")
        return { resolveAccessibilityLabels: () => ({}) };
      // Web's unsupported flag refuses before any parser/effect/renderer can run.
      if (relative.includes("/web/")) return {};
      throw new Error(`Unexpected native port: ${name}`);
    },
  });
  return module.exports.EnrichedMarkdownText;
}
for (const source of [
  "src/native/EnrichedMarkdownText.tsx",
  "lib/module/native/EnrichedMarkdownText.js",
]) {
  test(`${source}: iOS passes noMedia to both actual renderer bindings without changing Markdown`, () => {
    const render = wrapper(source);
    const markdown =
      "```c\nx < y; // ![literal](file:///not-read)\n```\n\n![alt](https://example.invalid/image)";
    for (const flavor of ["commonmark", "github"]) {
      const tree = render({
        markdown,
        flavor,
        md4cFlags: { noMedia: true },
        enableLinkPreview: false,
      });
      assert.equal(tree.type, flavor === "github" ? "NativeGFM" : "NativeCommonMark");
      assert.equal(tree.props.markdown, markdown);
      assert.equal(tree.props.md4cFlags.noMedia, true);
      assert.equal(tree.props.enableLinkPreview, false);
      assert.equal(render({ markdown, flavor }).props.md4cFlags.noMedia, false);
    }
  });
  test(`${source}: Android explicitly rejects true and keeps the false default`, () => {
    const render = wrapper(source, "android");
    assert.throws(
      () =>
        render({ markdown: "![alt](https://example.invalid/image)", md4cFlags: { noMedia: true } }),
      /supported only on iOS/,
    );
    assert.equal(render({ markdown: "**Ordinary**" }).props.md4cFlags.noMedia, false);
  });
}
for (const source of [
  "src/web/EnrichedMarkdownText.tsx",
  "lib/module/web/EnrichedMarkdownText.js",
]) {
  test(`${source}: unsupported noMedia cannot silently load the Web parser`, () => {
    assert.throws(
      () =>
        wrapper(
          source,
          "web",
        )({ markdown: "![alt](https://example.invalid/image)", md4cFlags: { noMedia: true } }),
      /supported only on iOS/,
    );
  });
}
