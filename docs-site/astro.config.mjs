// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import mermaid from 'astro-mermaid';
import { readFileSync } from 'node:fs';

// The living sky (real time-lapse footage, see src/sky/CREDITS.md) runs as a small classic script in <head>.
const livingSky = readFileSync(new URL('./src/sky/living-sky.js', import.meta.url), 'utf8');

export default defineConfig({
  integrations: [
    // Must come before Starlight so ```mermaid fences become diagrams, themed with the site.
    mermaid({ theme: 'neutral', autoTheme: true }),
    starlight({
      title: 'Zoen',
      description: 'The messenger where friendships, relationships and communities form — and where people build the apps and agents they share.',
      logo: { src: './src/assets/zoen-mascot.png', alt: 'Zoen' },
      favicon: '/favicon.png',
      social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/EnzoTironi/zoen-native' }],
      customCss: ['./src/styles/custom.css'],
      head: [{ tag: 'script', content: livingSky }],
      lastUpdated: false,
      pagination: true,
      tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 3 },
      sidebar: [
        {
          label: 'Start here',
          items: [
            { label: 'Overview', link: '/' },
            { slug: 'product/why-zoen' },
            { slug: 'product/features' },
            { slug: 'product/roadmap' },
          ],
        },
        {
          label: 'Tutorials',
          items: [
            { slug: 'guides/run-locally', label: 'Run Zoen on your machine' },
            { slug: 'guides/first-mini-app', label: 'Build your first mini-app' },
          ],
        },
        {
          label: 'How-to guides',
          items: [
            { slug: 'guides/apple', label: 'Run the Apple apps' },
            { slug: 'guides/android', label: 'Run the Android app' },
            { slug: 'guides/web', label: 'Run the web client' },
            { slug: 'technical/testing-and-ci' },
            { slug: 'technical/contributing' },
          ],
        },
        {
          label: 'Explanation',
          items: [
            { slug: 'technical/architecture' },
            { slug: 'technical/rust-core' },
            { slug: 'technical/encryption-and-devices' },
            { slug: 'technical/relay' },
            { slug: 'technical/sync' },
            { slug: 'technical/clients' },
            { slug: 'technical/agents' },
            { slug: 'technical/mini-apps' },
          ],
        },
        {
          label: 'Reference',
          items: [
            { slug: 'reference/repo-map' },
            { slug: 'reference/scripts' },
            { slug: 'reference/miniapp-manifest' },
            { slug: 'reference/decision-records' },
            { slug: 'product/glossary' },
          ],
        },
      ],
    }),
  ],
});
