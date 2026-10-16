# Mini-app manifest

Every mini-app has a `manifest.json` next to its `src/`. `tools/miniapp-build/build.mjs` validates it strictly and refuses to write a bundle that breaks any rule below.

## Fields

| Field | Required | Rule |
| --- | --- | --- |
| `id` | yes | Lowercase letters, digits and dashes, 2–32 characters, starting with a letter. |
| `name` | yes | 1–40 characters. Shown to people. |
| `description` | no | Free text. |
| `title` | no | Free text. |
| `entry` | yes | A `.ts`, `.tsx`, `.js` or `.jsx` file inside the app folder. |
| `capabilities` | no | Up to 8 entries `{ "id", "purpose" }`. `purpose` is 8–160 characters — **the person reads it** before granting. |
| `allowedDomains` | no | Up to 3 bare hostnames (no scheme, path or wildcard) the app may fetch from, after the person agrees. |
| `networkPurpose` | when `allowedDomains` is set | At least 8 characters explaining why the network is needed. |
| `linkDomains` | no | Up to 6 bare hostnames the app may open as outbound links (with confirmation). |
| `maxKB` | no | Integer ≤ 2048; bundle size budget. Default 400. |

Any other key is an error.

## Capabilities

| Capability | Gives the app |
| --- | --- |
| `photos.pick` | Photos the person chooses (location stripped). |
| `camera.capture` | A photo taken now. |
| `location` / `location.approximate` | Current location, precise or approximate. |
| `calendar.freebusy` | Busy blocks in a time range — not event details. |
| `calendar.events` | Calendar events. |
| `calendar.add` | Add an event; the person sees it before it's saved. |
| `contacts.pick` | Contacts the person chooses. |
| `health.steps` | Daily step counts. |

Host services every app can feature-detect with `zoen.capabilities.has()`: `state`, `members`, `ai`, `haptics`, `share`, `widget`, `theme`, `net:<domain>`.

## Bundle rules

- Bare imports must be on the builder's allowlist (`react`, `react-dom`, `zustand`, `date-fns`, `maplibre-gl` and their subpaths), pinned in `tools/miniapp-build/package-lock.json`.
- Local imports stay inside the app folder or `packages/` (`@zoen/miniapp-sdk`, `@zoen/ui`).
- Assets are inlined as `data:` URLs; no `<script src>`, no remote stylesheet, and no remote URL in the app's code except declared `allowedDomains`.
- The output is one HTML file plus `<id>.manifest.json` carrying its SHA-256. The host checks the hash before loading and injects a CSP that only opens allowed domains after consent.

## Examples

- `miniapps/schedule/manifest.json` — Agenda: one capability (`calendar.add`), no network.
- `miniapps/hike/manifest.json` — three capabilities, one map-tile domain with a `networkPurpose`, two link domains, `maxKB: 1600`.
