# Media-free message Markdown

The existing managed `react-native-enriched-markdown1.1.1` patch adds the optional `md4cFlags.noMedia` flag. It defaults to false, preserving document behavior. iOS forwards it through generated props, rendering and measurement flags, Objective-C copying and the common C++ parser. The parser disables HTML blocks and replaces media AST nodes with explicit links/alt text before any platform renderer sees them.

Chat Markdown opts in on iOS. Android and Web keep the conservative plain-text fallback for possible media markup, and the library wrappers explicitly reject `noMedia: true` there. No supported flag is silently ignored. Raw signed message text remains unchanged.

Run the focused source checks against the patched installed package:

```sh
node tools/markdown/test-no-media.mjs apps/zoen/node_modules/react-native-enriched-markdown --ios-bridge
node --test tools/markdown/no-media-bindings.test.mjs
```

The parser checks use actual MD4C C/C++ sources. On macOS, `--ios-bridge` also compiles the actual Objective-C flags/parser bridge with Foundation and tests the real measurement cache. Minimal React logging/type header ports replace neither parsing nor cache behavior. These checks do not start a renderer or make network requests.

This option requires a native build containing the matching patch and generated props. The source checks do not qualify a JavaScript-only rollout to an older native binary, or replace native UI validation in the combined build.
