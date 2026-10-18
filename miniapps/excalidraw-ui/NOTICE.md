# Excalidraw MCP Apps renderer attribution

Upstream: https://github.com/excalidraw/excalidraw-mcp

The pinned upstream package.json and README declare MIT. That commit contains no
standalone LICENSE file. We preserve those declarations without inventing a
copyright owner or year. Upstream copyright remains with its respective authors;
standard MIT terms are reproduced in LICENSE.

This explicitly patched browser build retains upstream PR #89, an initial host
context repair, and separate Zoen canonical full-scene adapters. It is not the
unmodified HTML returned by the public Excalidraw MCP server. The optional remote
presentation continues using standard provider tools. Canonical item data uses
Zoen's item-bound standard MCP tools and encrypted Rust state, not a remote copy.

All bundled dependency notices are reproduced in THIRD-PARTY-LICENSES.txt. React,
Excalidraw and morphdom are locally bundled, not loaded from esm.sh. Excalidraw's
MIT license is copied from its exact v0.18.0 tag; Radix's MIT license is copied from
published git head 3c5e05fab070dfa68212230e85baf0879269c57d. Fuzzy's README declares
MIT and its license is copied from published git head
39e3f256ce44411bc20ee79bc6bbf616ac88d163. The published
react-remove-scroll-bar@2.3.8 package declares MIT in metadata/README but contains
no standalone license text; its notice states that fact instead of inventing terms.

The builder's pinned Excalidraw source repairs enforce public aiEnabled=false for
the optional Mermaid entry/paste paths and remove their unreachable converter.
Standard fonts are embedded. Optional Xiaolai subsets use native system-ui for
CJK glyph fallback; Unicode text is retained without an external font download.
Exact module/font/lock/build hashes and CSP are recorded in generated manifests.
