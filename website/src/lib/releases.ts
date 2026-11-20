// GitHub Releases → what the download section shows.
//
// Assets are matched by NAME, using the convention the release scripts already
// write: `synapso-<os>-<arch>.<format>` (release.yml writes
// synapso-windows-x86_64.zip, tools/release-macos.sh writes
// synapso-macos-arm64.zip). A new build that follows the convention appears on
// the page by itself: a Linux tarball, an Intel Mac zip, a .dmg next to the
// .zip. Nothing here needs editing unless a new OS shows up.

export const REPO = 'webergnr/synapso';
export const REPO_URL = `https://github.com/${REPO}`;
export const RELEASES_URL = `${REPO_URL}/releases`;
export const LATEST_API = `https://api.github.com/repos/${REPO}/releases/latest`;

export type OS = 'macos' | 'windows' | 'linux';
export type Arch = 'arm64' | 'x86_64';

/** The platforms the page always lists, in order, and the builds each is
 * expected to have. An expected build with no asset reads "Not available yet". */
export const PLATFORMS: { os: OS; name: string; expect: Arch[] }[] = [
  { os: 'macos', name: 'macOS', expect: ['arm64', 'x86_64'] },
  { os: 'windows', name: 'Windows', expect: ['x86_64'] },
  { os: 'linux', name: 'Linux', expect: ['x86_64'] },
];

const ARCH_LABEL: Record<OS, Record<Arch, string>> = {
  macos: { arm64: 'Apple Silicon (arm64)', x86_64: 'Intel (x86_64)' },
  windows: { arm64: 'ARM64', x86_64: 'x64 (x86_64)' },
  linux: { arm64: 'ARM64 (aarch64)', x86_64: 'x86_64' },
};

const OS_ALIAS: Record<string, OS> = {
  macos: 'macos', mac: 'macos', darwin: 'macos', osx: 'macos',
  windows: 'windows', win: 'windows', win64: 'windows',
  linux: 'linux',
};
const ARCH_ALIAS: Record<string, Arch> = {
  arm64: 'arm64', aarch64: 'arm64',
  x86_64: 'x86_64', x64: 'x86_64', amd64: 'x86_64',
};

const ASSET = /^synapso-([a-z0-9]+)-([a-z0-9_]+)\.(zip|tar\.gz|tar\.xz|dmg|pkg|msi|exe|appimage|deb|rpm)$/i;

export interface GhAsset { name: string; size: number; browser_download_url: string }
export interface GhRelease {
  tag_name: string;
  html_url: string;
  published_at: string | null;
  draft?: boolean;
  prerelease?: boolean;
  assets: GhAsset[];
}

export interface Build {
  arch: Arch;
  archLabel: string;
  format: string;
  file: string;
  size: number;
  url: string;
}
export interface PlatformView {
  os: OS;
  name: string;
  builds: Build[];
  missing: { arch: Arch; archLabel: string }[];
}
export interface DownloadsView {
  version: string | null;
  publishedAt: string | null;
  releaseUrl: string;
  platforms: PlatformView[];
}

export function parseAsset(asset: GhAsset): (Build & { os: OS }) | null {
  const m = ASSET.exec(asset.name);
  if (!m) return null;
  const os = OS_ALIAS[m[1].toLowerCase()];
  const arch = ARCH_ALIAS[m[2].toLowerCase()];
  if (!os || !arch) return null;
  return {
    os,
    arch,
    archLabel: ARCH_LABEL[os][arch],
    format: '.' + m[3].toLowerCase().replace('appimage', 'AppImage'),
    file: asset.name,
    size: asset.size,
    url: asset.browser_download_url,
  };
}

/** `release` is null when nothing could be fetched: every platform then says
 * so, and the page points at the releases list instead of guessing. */
export function toView(release: GhRelease | null): DownloadsView {
  const usable = release && !release.draft ? release : null;
  const parsed = (usable?.assets ?? []).map(parseAsset).filter((b) => b !== null);
  return {
    version: usable?.tag_name ?? null,
    publishedAt: usable?.published_at ?? null,
    releaseUrl: usable?.html_url ?? RELEASES_URL,
    platforms: PLATFORMS.map(({ os, name, expect }) => {
      const builds = parsed
        .filter((b) => b.os === os)
        .map(({ os: _os, ...b }) => b)
        // expected architectures first, in the order PLATFORMS lists them
        .sort((a, b) => rank(expect, a.arch) - rank(expect, b.arch) || a.file.localeCompare(b.file));
      const missing = expect
        .filter((arch) => !builds.some((b) => b.arch === arch))
        .map((arch) => ({ arch, archLabel: ARCH_LABEL[os][arch] }));
      return { os, name, builds, missing };
    }),
  };
}

function rank(expect: Arch[], arch: Arch): number {
  const i = expect.indexOf(arch);
  return i === -1 ? expect.length : i;
}

/** The latest published release, or null on any failure (offline build, rate
 * limit, no release yet). `releases/latest` never returns drafts or
 * pre-releases. */
export async function fetchLatest(token?: string): Promise<GhRelease | null> {
  try {
    const res = await fetch(LATEST_API, {
      headers: {
        Accept: 'application/vnd.github+json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
      },
    });
    if (!res.ok) return null;
    const data = (await res.json()) as GhRelease;
    return Array.isArray(data.assets) && typeof data.tag_name === 'string' ? data : null;
  } catch {
    return null;
  }
}

/** "macOS Apple Silicon (arm64)", "Windows x64 (x86_64)": what a download
 * button offers, said before anyone clicks it. */
export function offered(view: DownloadsView): string[] {
  return [...new Set(view.platforms.flatMap((p) => p.builds.map((b) => `${p.name} ${b.archLabel}`)))];
}

export function formatSize(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function formatDate(iso: string | null): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  return d.toLocaleDateString('en', { year: 'numeric', month: 'short', day: 'numeric', timeZone: 'UTC' });
}
