// The platform cards as an HTML string. One function, two callers: the page
// renders it at build time, and the browser re-renders it when GitHub has a
// newer release than the build saw. Same markup either way.
import { formatSize, type DownloadsView, type OS } from './releases.ts';

export interface DownloadsCopy {
  notAvailable: string;
  fromSource: string;
  get: string;
  unsigned: { macos: string; windows: string };
  yourSystem: string;
  linuxNote: string;
}

const esc = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

const NOTE: Partial<Record<OS, keyof DownloadsCopy['unsigned']>> = { macos: 'macos', windows: 'windows' };

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
          ? `<p class="pf-note">${esc(t.unsigned[noteKey])}</p>`
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
