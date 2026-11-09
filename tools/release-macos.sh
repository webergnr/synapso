#!/usr/bin/env bash
# The macOS half of a release, by hand.
#
# CI builds only Windows (see .github/workflows/release.yml): a GitHub-hosted
# macOS runner needs Actions minutes this account does not have. This script is
# the recipe that job used to run, so it does not rot in a deleted YAML block.
#
#   tools/release-macos.sh v0.2.0            # build and zip
#   tools/release-macos.sh v0.2.0 --upload   # and add it to the draft release
#
# The draft is created by the tag push; this only adds a file to it. It refuses
# to touch a release somebody already published.
set -euo pipefail

TAG="${1:?usage: release-macos.sh <tag> [--upload]}"
UPLOAD="${2:-}"
ZIP="synapso-macos-arm64.zip"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# CFBundleShortVersionString only takes dotted numbers: `v0.2.0` -> `0.2.0`,
# `v0.2.0-rc1` -> `0.2.0`, and anything else is nobody's version.
VERSION="${TAG#v}"
VERSION="${VERSION%%[!0-9.]*}"
echo "$VERSION" | grep -qE '^[0-9]+(\.[0-9]+){0,2}$' || VERSION="0.0.0"

cargo test --release --locked
cargo build --release --locked

# the .zip name promises arm64: if this ever runs on an Intel Mac, break loudly
# instead of shipping a zip that lies
lipo -archs target/release/synapso | grep -qw arm64

# a bare Unix binary opens Terminal when you click it: on macOS what you ship
# is a bundle
rm -rf Synapso.app "$ZIP"
mkdir -p Synapso.app/Contents/MacOS Synapso.app/Contents/Resources
cp target/release/synapso Synapso.app/Contents/MacOS/
cp assets/icon/synapso.icns Synapso.app/Contents/Resources/
cat > Synapso.app/Contents/Info.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>synapso</string>
  <key>CFBundleIconFile</key><string>synapso</string>
  <key>CFBundleIdentifier</key><string>com.dizoli.synapso</string>
  <key>CFBundleName</key><string>Synapso</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# the linker signs the binary ad hoc, but not the bundle around it, and a
# bundle that is half signed fails `codesign --verify` ("code has no resources
# but signature indicates they must be present"), which `syspolicy_check
# distribution` calls Fatal. Signed ad hoc as a whole it is only a Warning:
# still not a Developer ID, so Gatekeeper still asks on first launch
codesign --force --sign - Synapso.app
codesign --verify --strict Synapso.app

# `ditto` is the system's own zip — it preserves the executable bit, which
# another machine's `zip` does not guarantee. --norsrc --noextattr: no `._`
# AppleDouble files, which any unzip but Finder's leaves inside the signed
# bundle
ditto -c -k --norsrc --noextattr --keepParent Synapso.app "$ZIP"
echo "built $ZIP (version $VERSION)"

if [ "$UPLOAD" != "--upload" ]; then
  echo "not uploaded. Drop it into the draft by hand, or re-run with --upload."
  exit 0
fi

# ⚠️ only into a draft: swapping the file the world is already downloading is
# not something this script gets to do
if [ "$(gh release view "$TAG" --json isDraft --jq .isDraft)" != "true" ]; then
  echo "error: $TAG is already published — not swapping assets under whoever is downloading them." >&2
  exit 1
fi
gh release upload "$TAG" --clobber "$ZIP"
echo "uploaded $ZIP to draft $TAG"
