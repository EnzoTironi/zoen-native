# Zoen docs site

The documentation website, built with [Astro Starlight](https://starlight.astro.build/).

Pages are **generated from the Markdown in `../docs/`** (`product/`, `guides/`, `technical/`, `reference/`) by `scripts/sync-content.mjs`, so the repository's docs stay the single source of truth. Only the landing page (`src/content/docs/index.mdx`) lives here. The sync fails on any broken relative link.

```sh
cd docs-site
npm ci
npm run dev       # http://localhost:4321 (syncs first)
npm run build     # static site in dist/ (syncs first), with Pagefind search
npm run preview
```

Edit the Markdown in `docs/`, not the generated pages under `src/content/docs/<section>/`.

## Living sky

Like the app, the site sits on a sky that follows the visitor's **local time**: real time-lapse footage of dawn (05:00–08:00), day (08:00–16:00), sunset (16:00–20:00) and night, crossfading over the half hour either side of each change. `src/sky/living-sky.js` is a small classic script inlined in `<head>` (no dependencies, no location or network lookups). The clips are 8-second loops in `public/sky/` (WebM and MP4, each with a poster photo; about 1.4 MB for all four). Only the one or two clips on screen are loaded. Sources and licences: [`src/sky/CREDITS.md`](src/sky/CREDITS.md).

- **Readable**: the footage is blurred and toned (lightened in light mode, darkened in dark mode), and every bit of text sits on a frosted panel: header, sidebars, table of contents, content, and the cards on the home page.
- **Fixed table of contents**: the frosting is drawn on a `::before` layer of each panel, never on the panel itself, so no ancestor of the table of contents gets a `backdrop-filter`.
- **Decorative only**: the sky is `aria-hidden="true"`, ignores the pointer and is hidden when printing.
- **Preferences**: `prefers-reduced-motion` shows a still photo and never downloads video; `prefers-contrast: more` and forced colours remove the sky and use solid panels. The clock is checked once a minute, and videos pause while the tab is hidden.
- **Preview a time** on `localhost` with `?skyHour=17` (ignored on any other host).
- **Mascot**: the header logo, the favicon and the home page art are the app's furball mascot (the waving pose from `docs/screens/mascot.jpg`, cut out onto a transparent background).

### Contrast check

```sh
npm run build
npm run check:contrast                     # dawn 06:30, day 12:00, sunset 18:00, night 01:00 × light/dark
npm run check:contrast -- --hours=all      # every hour of the day
```

It needs Chrome (`CHROME=/path/to/chrome`, default `/usr/bin/google-chrome`). For each page, theme, time of day and window size (1440×900 for every page, plus 1440×2160 for the architecture page with its table of contents), it pauses the footage on several frames (0.5 s, 3 s, 6 s; `--frames=`). On each frame it hides all text, screenshots the real background and measures every visible text run against **every pixel** under it (4.5:1 body, 3:1 large text and header icons). Diagram labels keep their boxes and are judged on the fill under them (10th-percentile pixel), so a crossing arrow line doesn't count as the background. It then runs axe-core's `color-contrast` rule: any node axe cannot decide must be measured by the pixel check (scrolled into view if needed) or the run fails. It also checks the reduced-motion still photo, that the table of contents stays put while scrolling, the more-contrast fallback and `aria-hidden`, writes `contrast-report/report.json`, and exits non-zero on any failure.
