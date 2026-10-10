# HIG source audit

The [hig-doctor](https://github.com/raintree-technology/hig-doctor) CLI is a local,
advisory source-pattern checker. Its findings help find places to review; its
exit status and positive-pattern count do not certify HIG compliance.

```sh
npm ci --prefix tools/hig-audit --ignore-scripts --no-audit --no-fund
bash scripts/audit-hig.sh apple > apple-hig.json
bash scripts/audit-hig.sh web > web-hig.json
```

Use the tool's declared Node 24.x runtime for reproducible checks. Evaluation on
10 October 2026 used Node 26.11.0 and Bun 1.3.14; Node 26 produces an engine
warning. The installation is locked to CLI 2.0.3 and TypeScript 5.9.3. It does not
install an MCP server or an agent skill. The wrapper disables configuration and
baseline discovery, never requests fixes or cache writes, and excludes generated
Rust bindings and native test fixtures. It prints JSON; a zero exit status means
the scan completed, including when concerns were found.

The published CLI bundle matched a local build of upstream commit
`5e2055877b83ec23930f4db7b062d0d3f725fa5a` byte for byte:
SHA-256 `0610463370f664a32bf97b8c7da170141f65cf7da5422260d2552b786acab33e`.
The core/CLI suite passed 232 tests. The upstream benchmark has 27 examples;
its accuracy on those examples does not establish accuracy on Zoen.

Independent calibration confirmed useful detection of bare Swift gestures,
clickable HTML divs without keyboard behavior and removed focus outlines.
It also reproduced false positives for labeled native button icons, nested
sidebar layout divs and replacement focus rings. A gesture with a button trait
and an unused Reduce Motion declaration escaped the relevant checks. Review
these cases in [validation.json](validation.json).

The tool's HIG snapshot is dated **2 February 2025**, before the current Liquid
Glass guidance. Do not use its bundled references as the current design policy.
Resolve findings against the [HIG acceptance matrix](../../docs/product/hig-compliance.md)
and actual accessibility, layout and interaction journeys. Do not suppress real
defects merely to obtain a clean report. CI gating remains disabled while rule
coverage and false positives are being evaluated.
