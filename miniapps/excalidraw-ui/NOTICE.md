# Excalidraw MCP UI, with two explicit source repairs

Upstream: https://github.com/excalidraw/excalidraw-mcp

The pinned upstream `package.json` declares `"license": "MIT"`, and its README
License section says MIT. That commit does not contain a standalone LICENSE file.
We preserve those declarations here without inventing a copyright owner or year.
Copyright in the upstream source remains with its respective authors.

The standard MIT terms are reproduced in LICENSE. Licenses for dependencies
actually included by the UI bundler are reproduced in THIRD-PARTY-LICENSES.txt.
The React, Excalidraw and morphdom runtime imports remain the exact versioned
esm.sh resources specified by the upstream UI, rather than local copies.

This is an explicitly patched, pinned UI build. It is not the unmodified HTML
returned by the public Excalidraw MCP server. Discovery and all diagram/checkpoint
tools still use that server and the standard MCP Apps protocol.
