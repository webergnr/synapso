// The platform cards and the release line as HTML strings. One function each,
// two callers: the page renders them at build time, and the browser re-renders
// them when GitHub has a newer release than the build saw. Same markup either
// way, so a build that could not read a release recovers fully in the browser.
import { formatDate, formatSize, RELEASES_URL, type DownloadsView, type OS } from './releases.ts';

export interface DownloadsCopy {
  latest: string;
  published: string;
  unknown: string;
  allReleases: string;
  notAvailable: string;
  fromSource: string;
  get: string;
  unsigned: { title: string; macos: string; windows: string };
  yourSystem: string;
  linuxNote: string;
}

const esc = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

const NOTE: Partial<Record<OS, 'macos' | 'windows'>> = { macos: 'macos', windows: 'windows' };

export function releaseHTML(view: DownloadsView, t: DownloadsCopy): string {
  const date = formatDate(view.publishedAt);
  const line = view.version
    ? `<span>${esc(t.latest)}</span>
<a class="ver" href="${esc(view.releaseUrl)}">${esc(view.version)}</a>${date ? `
<span>${esc(t.published)} ${esc(date)}</span>` : ''}`
    : `<span class="warn">${esc(t.unknown)}</span>`;
  return `${line}
<a class="all" href="${RELEASES_URL}">${esc(t.allReleases)} ↗</a>`;
}

export function platformsHTML(view: DownloadsView, t: DownloadsCopy): string {
  return view.platforms
    .map((p) => {
      const builds = p.builds
        .map((b) => {
          const label = `${t.get} Synapso ${view.version ?? ''} for ${p.name} ${b.archLabel}, ${b.format}, ${formatSize(b.size)}`;
          return `<li class="build">
  <div class="spec">
    <span class="arch">${esc(b.archLabel)}</span>
    <span class="file">${esc(b.file)}</span>
    <span class="meta">${esc(b.format)} · ${formatSize(b.size)}${view.version ? ` · ${esc(view.version)}` : ''}</span>
  </div>
  <a class="btn btn-primary get" href="${esc(b.url)}" aria-label="${esc(label.replace(/\s+/g, ' '))}">${esc(t.get)}<span class="arr down" aria-hidden="true">↓</span></a>
</li>`;
        })
        .join('');
      const missing = p.missing
        .map(
          (m) => `<li class="build na">
  <div class="spec">
    <span class="arch">${esc(m.archLabel)}</span>
    <span class="na-t">${esc(t.notAvailable)}</span>
  </div>
  <a class="src" href="#build">${esc(t.fromSource)} →</a>
</li>`,
        )
        .join('');
      const noteKey = NOTE[p.os];
      const note =
        p.builds.length && noteKey
          ? `<p class="pf-note"><strong>${esc(t.unsigned.title)}</strong> ${esc(t.unsigned[noteKey])}</p>`
          : p.os === 'linux' && !p.builds.length
            ? `<p class="pf-note">${esc(t.linuxNote)}</p>`
            : '';
      return `<article class="pf" data-os="${p.os}">
  <header class="pf-head">
    <h3 class="pf-name">${esc(p.name)}</h3>
    <span class="mine">${esc(t.yourSystem)}</span>
  </header>
  <ul class="builds">${builds}${missing}</ul>
  ${note}
</article>`;
    })
    .join('');
}
