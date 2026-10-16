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

Like the app, the site sits on a sky that follows the visitor's **local time**: dawn, morning, midday, golden hour, dusk and night, blended smoothly, with faint stars at night. `src/sky/living-sky.js` is a small classic script inlined in `<head>` (about 7 KB, 3 KB gzipped) (no dependencies, no location or network). Its palette, hour anchors (00, 06, 09, 12, 17, 20 h), star curve and smoothstep easing are copied verbatim from the app's `SkyClock` in [PR #127](https://github.com/EnzoTironi/zoen-native/pull/127) (`ui/living-sky`, commit `3d86d3b`); `npm test` pins them. Keep the two in sync until the app's version lands on `main` and can be shared.

- **Decorative only**: the layer is `aria-hidden="true"`, ignores the pointer and is hidden when printing.
- **Readable**: text sits on frosted surfaces, and the script caps how strongly the sky tints the page so the background stays light enough (light mode, luminance ≥ 0.50) or dark enough (dark mode, ≤ 0.05) for WCAG 2.2 AA at every minute of the day.
- **Preferences**: `prefers-reduced-motion` removes the colour transitions; `prefers-contrast: more` and forced colours remove the sky and use solid surfaces. The clock is sampled once a minute and stops while the tab is hidden.
- **Preview a time** on `localhost` with `?skyHour=17` (same override as the web app; ignored on any other host).

### Contrast check

```sh
npm run build
npm run check:contrast                     # dawn 06:00, day 12:00, sunset 17:00, night 00:00 × light/dark
npm run check:contrast -- --hours=all      # every hour of the day
```

It needs Chrome (`CHROME=/path/to/chrome`, default `/usr/bin/google-chrome`). For each page, theme and faked local time it hides all text, screenshots the real background (sky, clouds, stars, surfaces), and measures every visible text run against **every pixel** under it (4.5:1 body, 3:1 large text and header icons), then runs axe-core's `color-contrast` rule. It also checks the reduced-motion, more-contrast and `aria-hidden` behaviour, writes `contrast-report/report.json`, and exits non-zero on any failure.
