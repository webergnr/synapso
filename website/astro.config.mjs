// @ts-check
import { defineConfig } from 'astro/config';

// The public URL the site is served from (https://example.org, no path).
// The canonical link and the Open Graph tags need it; without it they are
// left out rather than pointing somewhere wrong. BASE_PATH is for hosting
// under a sub-path, e.g. /synapso on GitHub Pages.
const site = process.env.SITE_URL || undefined;
const base = process.env.BASE_PATH || '/';

export default defineConfig({
  site,
  base,
  trailingSlash: 'ignore',
  build: { inlineStylesheets: 'auto' },
  vite: {
    // the screenshots and tokens live in the app's repo, one level up:
    // the site reads them in place so it can never show a stale copy
    server: { fs: { allow: ['..'] } },
  },
});
