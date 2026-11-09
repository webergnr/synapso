// Generates one standalone HTML page + one PNG screenshot per UI frame, at native size, 2x
// scale, via headless Chrome. A "frame" here is either:
//   - Workspace.dc.html itself, rendered with its default props, OR
//   - one <dc-import name="X" ...props> inside "Notes Canvas Workspace.dc.html" — that file is
//     a spec-sheet gallery that imports Workspace/Config with specific prop overrides to show
//     every documented UI state (dark/light, palette open, empty state, search, etc).
//
// Usage: node shots.mjs <ds-dir> <out-dir>
//   <ds-dir>  directory containing Workspace.dc.html, "Notes Canvas Workspace.dc.html",
//             Config.dc.html, tokens.css, support.js (the fresh export)
//   <out-dir> where <slug>.html + <slug>.png land
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve as resolvePath } from 'node:path';
import { pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import {
  compileComponent, assemblePage, resetHoverRules,
  parseDcImportAttrs, parseAttrPropValue, kebabToCamel,
} from './build.mjs';

const [dsDir, outDir] = process.argv.slice(2);
if (!dsDir || !outDir) { console.error('Usage: node shots.mjs <ds-dir> <out-dir>'); process.exit(1); }

const CHROME = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';

function shoot(htmlPath, pngPath, width, height) {
  const url = pathToFileURL(resolvePath(htmlPath)).href;
  execFileSync(CHROME, [
    '--headless=new', '--disable-gpu', '--hide-scrollbars',
    '--force-device-scale-factor=2',
    `--window-size=${width},${height}`,
    `--screenshot=${resolvePath(pngPath)}`,
    url,
  ], { stdio: 'inherit' });
}

function renderFrame(slug, componentPath, props, size) {
  resetHoverRules();
  const result = compileComponent(componentPath, props);
  const { width, height } = size || result.preview;
  const page = assemblePage(dsDir, result, slug);
  const htmlPath = join(outDir, `${slug}.html`);
  const pngPath = join(outDir, `${slug}.png`);
  writeFileSync(htmlPath, page);
  shoot(htmlPath, pngPath, width, height);
  console.log(`${slug}  ${width}x${height}  -> ${pngPath}`);
  return { slug, width, height, pngPath };
}

const frames = [];

// 1. Workspace.dc.html, default props.
frames.push(renderFrame('Workspace-default', join(dsDir, 'Workspace.dc.html'), {}, null));

// 2. every <dc-import> inside the Notes Canvas Workspace spec sheet.
const notesSrc = readFileSync(join(dsDir, 'Notes Canvas Workspace.dc.html'), 'utf8');
const SECTION_RE = /<section data-screen-label="([^"]+)"[^>]*>([\s\S]*?)<\/section>/g;
const IMPORT_RE = /<dc-import\b([^>]*)><\/dc-import>/g;
let sm;
while ((sm = SECTION_RE.exec(notesSrc))) {
  const [, label, sectionBody] = sm;
  const slugBase = `Notes Canvas Workspace-${label.replace(/[^\w]+/g, '-')}`;
  let im, n = 0;
  IMPORT_RE.lastIndex = 0;
  const imports = [];
  while ((im = IMPORT_RE.exec(sectionBody))) imports.push(im[1]);
  for (const rawAttrs of imports) {
    n++;
    const attrs = parseDcImportAttrs(rawAttrs);
    const name = attrs.name;
    const props = {};
    for (const [k, v] of Object.entries(attrs)) {
      if (k === 'name' || k === 'hint-size' || k === 'style') continue;
      props[kebabToCamel(k)] = parseAttrPropValue(v, {});
    }
    const slug = imports.length > 1 ? `${slugBase}-${n}` : slugBase;
    frames.push(renderFrame(slug, join(dsDir, `${name}.dc.html`), props, null));
  }
}

console.log(`\n${frames.length} frames rendered.`);
