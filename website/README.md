# Synapso — website

The landing page: one static page, built with [Astro](https://astro.build) and
plain CSS. No backend; the only thing it asks the network for is the latest
GitHub release.

## Run it

Needs Node 22.23.4 or newer (24 is what `.nvmrc` pins and CI uses).

```sh
cd website
npm ci
npm run dev        # http://localhost:4321, live reload
npm test           # the release-asset matcher (node --test)
npm run check      # astro check: types and template diagnostics
npm run build      # static site in dist/
npm run preview    # serve dist/ to look at the real build
```

`.github/workflows/website.yml` runs `test`, `check` and `build` on every pull
request that touches the site, its screenshots, the tokens or the icon.

## Where things come from

The site reads the app's own files in place, so it cannot drift from the app:

| What | From |
|---|---|
| Every colour | `../design/tokens.css` (the dark theme), imported by `src/styles/site.css` |
| Screenshots | `src/assets/screens/*.png`: real window captures of the app on a Retina Mac (1392 × 870 points at 2×, demo vault from `../fixture`), macOS bar included; Astro turns them into AVIF/WebP at four widths |
| Icon, favicon | `../assets/icon/` |
| The canvas skeleton | `../fixture/seed/architecture.canvas`, reduced by hand to its key points in `src/components/Canvas.astro` |
| Frame times | the README's *Performance* table, copied into `src/content/en.ts` |

Every sentence of copy lives in `src/content/en.ts`. If the app stops doing
something, delete the sentence there. What the drawing and the demos show (file
names, the note's lines, the canvas JSON) is the demo vault's own content and
stays in their components.

## Downloads

`src/lib/releases.ts` reads `releases/latest` from the GitHub API and matches
assets **by file name**, using the names the release scripts already write:

```
synapso-<os>-<arch>.<format>
         macos | windows | linux
                 arm64 | aarch64 | x86_64 | x64 | amd64
                         zip | tar.gz | tar.xz | dmg | pkg | msi | exe | AppImage | deb | rpm
```

- A build that follows the convention shows up by itself, with its
  architecture, format, size and version: a Linux tarball, an Intel Mac zip, a
  `.dmg` next to the `.zip`. Nothing on the page needs editing.
- A platform with no build says **Not available yet** and points at building
  from source. The platforms and the architectures each one is expected to
  have are the `PLATFORMS` table at the top of `releases.ts`; a new
  architecture is one entry there.
- A file that does not follow the convention is ignored, never guessed at.
- If GitHub cannot be reached, every platform says so and links to the
  releases list.

The release is read twice: once at **build time** (so the page works without
JavaScript, and `GITHUB_TOKEN`, if set, lifts the API rate limit) and once in
the **browser** when the download section comes near, so a release published
after the last deploy appears without redeploying. Both render through the same
function, `src/lib/downloads-html.ts`. Nothing ever starts a download on its
own.

The tests in `src/lib/releases.test.ts` hold the real v0.0.1 asset list and a
few future ones.

## Publish

Any static host works. Build settings, whichever host:

| Setting | Value |
|---|---|
| Root directory | `website` (the build reads `../design`, `../docs`, `../assets`, so the host must check out the whole repository, which they all do) |
| Build command | `npm ci && npm run build` |
| Output directory | `website/dist` (or `dist`, relative to the root directory) |
| Node | 24 (`NODE_VERSION=24` where the host asks) |
| `SITE_URL` | the public origin, e.g. `https://synapso.dev`. Needed for the canonical link and the Open Graph tags; without it they are left out |
| `BASE_PATH` | only when served under a sub-path, e.g. `/synapso` on GitHub Pages |
| `GITHUB_TOKEN` | optional, see above |

- **Cloudflare Pages / Netlify / Vercel**: connect the repository and use the
  table above.
- **GitHub Pages**: `SITE_URL=https://webergnr.github.io`,
  `BASE_PATH=/synapso`, and a workflow that runs the build and publishes
  `website/dist` with `actions/upload-pages-artifact` + `actions/deploy-pages`.
  Pages has to be switched on in the repository settings first.

## Social card

`public/og.png` (1200 × 630) is a screenshot of the `/og` page, which draws the
card from the same components. After changing the hero, run `npm run build &&
npm run preview`, open `/og` at 1200 × 630 and save the screenshot over
`public/og.png`.

## Files

```
website/
  src/
    content/en.ts           every string on the page
    lib/releases.ts         GitHub release → what each platform offers
    lib/downloads-html.ts   the platform cards, for the build and the browser
    styles/site.css         site tokens and base styles, on top of the app's
    components/             one file per section; HeroArtwork.astro is the drawing
    pages/index.astro       the page
    pages/og.astro          the social card
    fonts/                  subsets cut from ../assets/fonts (see below)
  public/                   favicon, og.png, robots.txt, font licences
```

## Fonts

All three are under the SIL Open Font License 1.1; the licence texts ship in
`public/fonts/`.

- **Archivo** (headings and text) and **JetBrains Mono** come from
  Fontsource packages.
- `src/fonts/jbmono-symbols-*.woff2` are the key and box glyphs (⌘ ⇧ ⌥ ⏎ → ├ ■)
  that Fontsource's latin subset leaves out, cut from the app's own
  `assets/fonts/JetBrainsMono-*.ttf`.
- `src/fonts/excalifont-latin.woff2` is the app's canvas hand, for the captions
  in the hero drawing.

To cut them again (`pip install fonttools brotli`):

```sh
pyftsubset ../assets/fonts/JetBrainsMono-Regular.ttf \
  --unicodes="U+2190-21FF,U+2300-23FF,U+2500-257F,U+25A0-25FF" \
  --flavor=woff2 --layout-features='*' --output-file=src/fonts/jbmono-symbols-Regular.woff2
pyftsubset ../assets/fonts/Excalifont-Regular.ttf \
  --unicodes="U+0020-007E,U+2014" --flavor=woff2 --output-file=src/fonts/excalifont-latin.woff2
```

## Localising

Copy `src/content/en.ts` to another language with the same shape, then build a
page that passes it to the components (`src/pages/index.astro` is the
template). The release labels (`Apple Silicon (arm64)`, …) live in
`src/lib/releases.ts`.
