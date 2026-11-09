// Compiles a Claude Design `.dc.html` export into a dependency-free static HTML page.
//
// `.dc.html` is NOT plain HTML — it's a small component format (dc-runtime):
//   - <helmet>...</helmet>            head content (fonts, tokens.css link, base <style>)
//   - <x-dc>...</x-dc>                the component template
//   - <script data-dc-script>         a `class Component extends DCLogic { renderVals() {...} }`
//                                      whose renderVals() returns ALL the data the template needs
//   - {{ expr }}                      mustache-style value/attr interpolation (dotted paths,
//                                      true/false/null/number/quoted-string literals)
//   - <sc-if value="{{ expr }}">...</sc-if>          conditional block (drops if falsy)
//   - <sc-for list="{{ expr }}" as="x">...</sc-for>  repeats block once per array item,
//                                      substituting {{ x.field }} inside with that item's value
//   - style-hover="decls"             becomes a generated `.hvN:hover{decls}` rule
//   - <dc-import name="Other" prop="val" .../>       inlines ANOTHER sibling `Other.dc.html`
//                                      component, rendered with the given prop overrides
//                                      (this project's "spec sheet" files use this to show many
//                                      prop-variants of Workspace/Config as one long document)
//
// Usage (regen a single compiled page):
//   node build.mjs <in.dc.html> <out.html>
//
// Also exports `compileComponent()` for frame-by-frame extraction (see shots.mjs), since a
// dc-import is rendered via the exact same code path as a top-level file.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, basename } from 'node:path';

// ---------- tiny mustache expr resolver (subset of dc-runtime's src/expr.ts: this project only
// ever uses literals + dotted paths, no ==, !, or parens, so that's all we implement) ----------
function resolvePath(vals, expr) {
  return expr.split('.').reduce((cur, key) => (cur == null ? undefined : cur[key]), vals);
}
function evalExpr(vals, rawExpr) {
  const expr = rawExpr.trim();
  if (expr === 'true') return true;
  if (expr === 'false') return false;
  if (expr === 'null') return null;
  if (expr === 'undefined') return undefined;
  if (/^-?\d+(\.\d+)?$/.test(expr)) return Number(expr);
  if (expr.length >= 2 && (expr[0] === '"' || expr[0] === "'") && expr.at(-1) === expr[0]) return expr.slice(1, -1);
  return resolvePath(vals, expr);
}
// literal-ize a resolved value back into `{{ ... }}` source so it can be re-evaluated by evalExpr
// (used when substituting a sc-for loop var into a nested {{ }} that a later pass still parses)
function literal(v) {
  if (typeof v === 'string') return `'${v.replace(/'/g, "\\'")}'`;
  if (v == null) return 'null';
  return String(v);
}
// Replace {{ expr }} tokens whose head identifier is in `scope` (a Set of var names), using
// `resolve(expr)` for the substitution value; anything else is left untouched (for a later pass).
function substituteMustache(html, scope, resolve) {
  return html.replace(/\{\{\s*([\s\S]+?)\s*\}\}/g, (whole, expr) => {
    const head = expr.match(/^[A-Za-z_$][\w$]*/);
    if (!head || !scope.has(head[0])) return whole;
    return `{{ ${literal(resolve(expr))} }}`;
  });
}
// Final pass: every remaining {{ expr }} becomes its rendered text.
function renderMustache(html, vals) {
  return html.replace(/\{\{\s*([\s\S]+?)\s*\}\}/g, (_w, expr) => {
    const v = evalExpr(vals, expr);
    return v == null ? '' : String(v);
  });
}

// ---------- <sc-if>/<sc-for> expansion (outside-in, so a sc-for's loop var is substituted
// as a literal BEFORE any nested sc-if/sc-for inside it gets evaluated) ----------
const CTRL_RE = /<sc-(if|for)\b([^>]*)>|<\/sc-(?:if|for)>/g;
function findBlock(html, fromIndex) {
  CTRL_RE.lastIndex = fromIndex;
  const open = CTRL_RE.exec(html);
  const tagType = open[1];
  const attrs = open[2];
  let depth = 1;
  let idx = CTRL_RE.lastIndex;
  const contentStart = idx;
  let contentEnd = -1, blockEnd = -1;
  while (depth > 0) {
    CTRL_RE.lastIndex = idx;
    const m = CTRL_RE.exec(html);
    if (!m) throw new Error('Unbalanced <sc-if>/<sc-for> tags');
    depth += m[1] ? 1 : -1;
    idx = CTRL_RE.lastIndex;
    if (depth === 0) { contentEnd = m.index; blockEnd = idx; }
  }
  return { tagType, attrs, contentStart, contentEnd, blockEnd };
}
function attrVal(attrs, name) {
  const m = new RegExp(`${name}="([^"]*)"`).exec(attrs);
  return m ? m[1] : null;
}
function braceExpr(raw) {
  const m = /^\{\{([\s\S]*)\}\}$/.exec(raw ?? '');
  return m ? m[1].trim() : raw;
}
function expandControlFlow(html, vals) {
  let out = '';
  let pos = 0;
  while (true) {
    const i1 = html.indexOf('<sc-if', pos);
    const i2 = html.indexOf('<sc-for', pos);
    const start = i1 === -1 ? i2 : i2 === -1 ? i1 : Math.min(i1, i2);
    if (start === -1) { out += html.slice(pos); break; }
    out += html.slice(pos, start);
    const b = findBlock(html, start);
    const inner = html.slice(b.contentStart, b.contentEnd);
    if (b.tagType === 'if') {
      if (evalExpr(vals, braceExpr(attrVal(b.attrs, 'value')))) out += expandControlFlow(inner, vals);
    } else {
      const list = evalExpr(vals, braceExpr(attrVal(b.attrs, 'list')));
      const as = attrVal(b.attrs, 'as') || 'item';
      const scope = new Set([as]);
      for (const item of Array.isArray(list) ? list : []) {
        const scoped = substituteMustache(inner, scope, (expr) => {
          const rest = expr === as ? '' : expr.slice(as.length + 1);
          return rest ? resolvePath(item, rest) : item;
        });
        out += expandControlFlow(scoped, vals);
      }
    }
    pos = b.blockEnd;
  }
  return out;
}

// ---------- style-hover="decls" -> generated .hvN:hover{} rule ----------
function convertHover(html) {
  const rules = [];
  const seen = new Map();
  const out = html.replace(/\s+style-hover="([^"]*)"/g, (_m, decls) => {
    if (!seen.has(decls)) {
      seen.set(decls, `hv${rules.length + 1}`);
      rules.push(`.${seen.get(decls)}:hover{${decls}}`);
    }
    return ` class="${seen.get(decls)}"`;
  });
  return { html: out, rules };
}

// ---------- one component: parse -> eval DCLogic -> expand -> substitute ----------
function parseDcFile(src) {
  const helmet = /<helmet>([\s\S]*?)<\/helmet>/.exec(src)[1];
  const content = /<\/helmet>([\s\S]*?)<\/x-dc>/.exec(src)[1];
  const scriptBody = /<script[^>]*data-dc-script[^>]*data-props="([^"]*)"[^>]*>([\s\S]*?)<\/script>/.exec(src);
  const propsSchema = JSON.parse(scriptBody[1].replace(/&quot;/g, '"'));
  return { helmet, content, script: scriptBody[2], preview: propsSchema.$preview || null };
}
class DCLogicStub {
  constructor(props) { this.props = props || {}; }
  setState() {}
}
function renderValsFor(script, props) {
  const fn = new Function('DCLogic', 'props', `${script}\nreturn new Component(props).renderVals();`);
  return fn(DCLogicStub, props);
}
const KEBAB_RE = /-([a-z])/g;
function kebabToCamel(s) { return s.replace(KEBAB_RE, (_, c) => c.toUpperCase()); }
function parseDcImportAttrs(raw) {
  const attrs = {};
  const re = /([\w-]+)="([^"]*)"/g;
  let m;
  while ((m = re.exec(raw))) attrs[m[1]] = m[2];
  return attrs;
}
function parseAttrPropValue(raw, outerVals) {
  const brace = /^\{\{\s*([\s\S]*?)\s*\}\}$/.exec(raw);
  if (!brace) return raw; // plain string, e.g. theme="dark"
  return evalExpr(outerVals, brace[1]);
}

// Compiles one `.dc.html` file (by path) with the given prop overrides, returning its rendered
// body HTML, own helmet, accumulated hover CSS, and native preview size (if it declares one).
function compileComponent(filePath, propsOverride) {
  const dir = dirname(filePath);
  const src = readFileSync(filePath, 'utf8');
  const { helmet, content, script, preview } = parseDcFile(src);
  const vals = renderValsFor(script, propsOverride || {});

  // dc-import: inline sibling components first (their own prop attrs don't reference `vals`
  // in this project, but we resolve any {{ }} ones against it for robustness).
  let expanded = content.replace(/<dc-import\b([^>]*)><\/dc-import>/g, (_m, rawAttrs) => {
    const attrs = parseDcImportAttrs(rawAttrs);
    const name = attrs.name;
    const props = {};
    for (const [k, v] of Object.entries(attrs)) {
      if (k === 'name' || k === 'hint-size' || k === 'style') continue;
      props[kebabToCamel(k)] = parseAttrPropValue(v, vals);
    }
    const sub = compileComponent(join(dir, `${name}.dc.html`), props);
    hoverRulesAll.push(...sub.hoverCss);
    const wrapperStyle = attrs.style || '';
    return `<div style="${wrapperStyle}">${sub.bodyHtml}</div>`;
  });

  expanded = expandControlFlow(expanded, vals);
  expanded = expanded.replace(/\s+on[A-Za-z]+="\{\{[^}]*\}\}"/g, ''); // drop React-style handlers
  const { html: hoverHtml, rules } = convertHover(expanded);
  hoverRulesAll.push(...rules);
  const finalHtml = renderMustache(hoverHtml, vals);

  const leftovers = ['{{', '<sc-if', '<sc-for', '<dc-import', 'style-hover'].filter((t) => finalHtml.includes(t));
  if (leftovers.length) throw new Error(`${basename(filePath)}: unresolved DC artifacts remain: ${leftovers.join(', ')}`);

  return { bodyHtml: finalHtml, helmet, hoverCss: rules, preview };
}
// hoverRulesAll is shared across the whole (possibly recursive) compile so nested dc-imports'
// hover rules land in one final <style> block. Reset per top-level compileFile() call.
let hoverRulesAll = [];

// Wraps a compileComponent() result into a standalone, dependency-free HTML document
// (tokens.css inlined, hover rules collected). Used both for the full-file compile below
// and for per-frame extraction in shots.mjs.
export function assemblePage(dir, result, title) {
  const tokensCss = readFileSync(join(dir, 'tokens.css'), 'utf8');
  const helmetNoTokensLink = result.helmet.replace(/<link rel="stylesheet" href="tokens\.css">/, '');
  const allHoverCss = [...new Set(hoverRulesAll)].join('\n');
  // The component's root div is `width:100%;height:100%` — same as a <dc-import>'s wrapper, a
  // fixed-size container is expected around it. At the top level (no dc-import wrapper) give
  // <body> that same explicit pixel size when the component declares one ($preview); percentage
  // heights on a bare <body> would otherwise collapse to content height and get silently masked
  // by the dark theme's near-black page background (only visible as a gap in the light theme).
  const bodySize = result.preview ? `width:${result.preview.width}px;height:${result.preview.height}px;overflow:hidden;` : '';
  return `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${title}</title>
<style>${tokensCss}</style>
${helmetNoTokensLink}
<style>${allHoverCss}</style>
</head>
<body style="margin:0;${bodySize}">
${result.bodyHtml}
</body>
</html>
`;
}

// ---------- top-level: one file -> one standalone static page ----------
export function compileFile(inPath) {
  hoverRulesAll = [];
  const dir = dirname(inPath);
  const result = compileComponent(inPath, {});
  return assemblePage(dir, result, basename(inPath).replace(/\.dc\.html$/, ''));
}

export { compileComponent, parseDcImportAttrs, parseAttrPropValue, kebabToCamel, evalExpr };
export function resetHoverRules() { hoverRulesAll = []; }

// ---------- CLI: node build.mjs <in.dc.html> <out.html> ----------
if (import.meta.url === `file://${process.argv[1]}`) {
  const [inPath, outPath] = process.argv.slice(2);
  if (!inPath || !outPath) {
    console.error('Usage: node build.mjs <in.dc.html> <out.html>');
    process.exit(1);
  }
  const html = compileFile(inPath);
  writeFileSync(outPath, html);
  console.log(`Built ${outPath} (${html.length} bytes)`);
}
