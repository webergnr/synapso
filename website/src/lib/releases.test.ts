// node --test src/lib/releases.test.ts  (Node 24 runs TypeScript as is)
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseAsset, toView, RELEASES_URL, type GhRelease } from './releases.ts';

const asset = (name: string) => ({ name, size: 15_827_678, browser_download_url: `https://example.test/${name}` });

// the real v0.0.1 release, as the API returned it on 2026-11-19
const v001: GhRelease = {
  tag_name: 'v0.0.1',
  html_url: 'https://github.com/webergnr/synapso/releases/tag/v0.0.1',
  published_at: '2026-11-09T21:41:23Z',
  assets: [asset('synapso-macos-arm64.zip'), asset('synapso-windows-x86_64.zip')],
};

test('v0.0.1: Apple Silicon and Windows x64 are offered, the rest says so', () => {
  const view = toView(v001);
  assert.equal(view.version, 'v0.0.1');
  const [mac, win, linux] = view.platforms;

  assert.deepEqual(mac.builds.map((b) => [b.arch, b.format, b.file]), [['arm64', '.zip', 'synapso-macos-arm64.zip']]);
  assert.deepEqual(mac.missing.map((m) => m.arch), ['x86_64']);

  assert.deepEqual(win.builds.map((b) => b.archLabel), ['x64 (x86_64)']);
  assert.deepEqual(win.missing, []);

  assert.deepEqual(linux.builds, []);
  assert.deepEqual(linux.missing.map((m) => m.arch), ['x86_64']);
});

test('a build that follows the naming convention shows up without code changes', () => {
  const view = toView({
    ...v001,
    assets: [
      ...v001.assets,
      asset('synapso-linux-aarch64.tar.gz'),
      asset('synapso-linux-x86_64.AppImage'),
      asset('synapso-macos-x86_64.dmg'),
      asset('synapso-macos-arm64.dmg'),
    ],
  });
  const [mac, , linux] = view.platforms;
  // expected arches first (arm64, then Intel), several formats per arch
  assert.deepEqual(mac.builds.map((b) => b.file), [
    'synapso-macos-arm64.dmg', 'synapso-macos-arm64.zip', 'synapso-macos-x86_64.dmg',
  ]);
  assert.deepEqual(mac.missing, []);
  // an unexpected arch is still listed, after the expected one
  assert.deepEqual(linux.builds.map((b) => [b.arch, b.format]), [['x86_64', '.AppImage'], ['arm64', '.tar.gz']]);
});

test('files that do not follow the convention are ignored, not guessed', () => {
  for (const name of ['synapso.exe', 'checksums.txt', 'synapso-freebsd-x86_64.zip', 'synapso-macos-universal.zip', 'source.zip']) {
    assert.equal(parseAsset(asset(name)), null, name);
  }
});

test('no release (fetch failed) or a draft: nothing is offered, the link is the releases list', () => {
  for (const release of [null, { ...v001, draft: true }]) {
    const view = toView(release);
    assert.equal(view.version, null);
    assert.equal(view.releaseUrl, RELEASES_URL);
    assert.ok(view.platforms.every((p) => p.builds.length === 0 && p.missing.length > 0));
  }
});
