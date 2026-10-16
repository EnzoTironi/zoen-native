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
