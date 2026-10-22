[Research index](agent-sandbox.md).

## 5. Browser use

### Architectures
- **Browserbase, Steel, Hyperbrowser, Browser Use Cloud** all run Chromium in a remote
  sandbox and hand the agent a CDP endpoint (Playwright, Puppeteer or raw CDP), plus a live
  view URL. Steel's runtime is open source, Apache-2.0 [ST1]; it bundles Chrome, a CDP API on
  9223 and a viewer UI [ST2]. Browser Use (MIT) is the agent library [G].
- **Live view and takeover.** Browser Use returns a live URL a person can open to take over the
  same session, and warns that anyone holding the live or CDP URL controls the browser [BU1].
  Cloudflare has explicit `handoff` and `handoffComplete` calls for human-in-the-loop [CF2].
- **Streaming options.** CDP `Page.startScreencast` streams JPEG/PNG frames with acks and
  needs nothing else in the VM [CDP1]; input goes back through `Input.dispatch*Event`. WebRTC
  (for example neko, Apache-2.0 [G]) gives smoother video and audio but needs a desktop, a
  WebRTC stack and TURN. noVNC is MPL-2.0, which is file-level copyleft; fine unmodified, but
  we do not need it.
- **Memory.** Plan 250–500 MB per typical headless Chrome, 400–700 MB for heavy SPAs, more
  than 1 GB with video or WebGL; measure with PSS or cgroups, not summed RSS [BR1]. Our own
  numbers for light pages are below.
- **Anti-bot.** Datacenter IPs get challenged. Providers sell residential proxies by the GB
  (Browserbase $10–12/GB [BB1], Steel $6–10/GB [ST3], Hyperbrowser $10/GB [HB1]) and CAPTCHA
  solving. The honest alternative is to sign our agent's requests with **Web Bot Auth**
  (HTTP message signatures), which Cloudflare recognizes as "signed agents" [CF3].

### Measured on the box (P2, 2026-10-09)

`cargo test -p zoen-sandboxd --test journey_browser`: Chromium 152 (Alpine 3.24 package) in a
Firecracker VM with 2 vCPUs and 1 GiB, through the egress proxy with Web Bot Auth signing and
TLS interception, on the same nested-KVM box as P1. The shop pages are small, so memory is a
lower bound for real sites.

| What | Measured | In plain words |
|---|---|---|
| Browser template, once per shape (cold boot, Chromium start, 1 GiB snapshot) | 4.1–4.4 s | paid once per node start |
| Warm start: lease from the pool (restore, resume, entropy, clock) | 74–77 ms | the browser is already running when the agent gets it |
| Warm start to the first page on screen (HTTPS through the proxy, signed) | 0.76–0.82 s | most of it is the first page load, not the VM |
| Later page loads / a click to the next page | about 0.36 s / about 0.2 s | each request is its own TLS connection through the proxy today |
| Live view, start to first sealed frame | 48–64 ms; about 8 KiB per frame | |
| Memory of one browser VM after five pages (VMM, cgroup) | about 400–485 MiB charged; guest using about 350–400 MiB | well inside the 1 GiB + 64 MiB cap |
| A second browser from the same template, same pages | adds about 90–180 MiB written since restore; about 230 MiB of the template is shared page cache | snapshots make browsers cheap to run side by side |

**Browsers per host.** On these light pages each extra browser costs about 130 MiB, so about
100 fit on a 16 GiB host and about 470 on 64 GiB (keeping 2 GiB for the host). Real sites
are heavier: at the 250–500 MB per browser the secondary sources give [BR1], that is about
30–55 per 16 GiB and 120–240 per 64 GiB; if every browser grew to its 1 GiB cap, 58 per
64 GiB. The cost model keeps its 2 GiB-per-browser assumption until we measure real sites at
scale (P4).

### Provider prices (per browser-hour)

| Provider | Price | Source |
|---|---|---|
| Browserbase | $0.12 (Developer, $20/mo with 100 h), $0.10 (Startup, $99/mo with 500 h); 1-minute minimum per session | [BB1] |
| Steel cloud | $0.10 (Launch), $0.08 (Scale, $250/mo) | [ST3] |
| Cloudflare Browser Run | $0.09 beyond included hours | [CF4] |
| Hyperbrowser | $0.10 | [HB1] |
