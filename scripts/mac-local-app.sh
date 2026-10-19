#!/usr/bin/env bash

# Builds a locally signed Browsers .app for this Mac (host architecture only) and installs it.
# Not for distribution - see build-mac.sh for the universal, notarized release build.
#
#   dev      debug build    "Browsers Dev.app"  software.Browsers.dev  ~/Applications
#   release  release build  "Browsers.app"      software.Browsers      /Applications
#
# The two builds have different bundle ids, so macOS treats them as separate apps.
# They never share config, cache, logs or the default browser setting.

set -euo pipefail

cd "$(dirname "$0")/.."

# optional local overrides, e.g CODESIGN_IDENTITY
if test -f .env; then
  source .env
fi

variant="${1:-dev}"
case "$variant" in
  dev)
    profile="debug"
    cargo_flags=""
    bundle_id="software.Browsers.dev"
    app_name="Browsers Dev"
    install_dir="$HOME/Applications"
    ;;
  release)
    profile="release"
    cargo_flags="--release"
    bundle_id="software.Browsers"
    app_name="Browsers"
    install_dir="/Applications"
    ;;
  *)
    echo "usage: $0 dev|release" >&2
    exit 1
    ;;
esac

app="$install_dir/$app_name.app"
lsregister="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

# also re-creates target/universal-apple-darwin/meta/Info.plist (see build.rs)
cargo build $cargo_flags

# quit a running copy of this variant only (both variants have the same executable name)
pkill -f "$app/Contents/MacOS/Browsers" || true

# Assemble the bundle directly in its install location.
# A second copy under target/ would also get picked up by LaunchServices
# and could end up being the one macOS opens links with.
mkdir -p "$install_dir"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
mkdir -p "$app/Contents/Resources/i18n/en-US"
mkdir -p "$app/Contents/Resources/icons/512x512"
mkdir -p "$app/Contents/Resources/repository"

plist="$app/Contents/Info.plist"
cp target/universal-apple-darwin/meta/Info.plist "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $bundle_id" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName $app_name" "$plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $app_name" "$plist"

cp extra/macos/icons/Browsers.icns "$app/Contents/Resources/Browsers.icns"
cp resources/icons/512x512/software.Browsers.png "$app/Contents/Resources/icons/512x512/software.Browsers.png"
cp resources/i18n/en-US/builtin.ftl "$app/Contents/Resources/i18n/en-US/builtin.ftl"
cp resources/repository/application-repository.toml "$app/Contents/Resources/repository/application-repository.toml"
cp "target/$profile/browsers" "$app/Contents/MacOS/Browsers"

# Sign with a stable identity, so macOS keeps recognizing the app as the same app across rebuilds.
# Uses CODESIGN_IDENTITY if set, otherwise the first "Apple Development" certificate in the keychain,
# otherwise falls back to ad-hoc signing.
identity="${CODESIGN_IDENTITY:-}"
if test -z "$identity"; then
  identity="$(security find-identity -v -p codesigning | grep "Apple Development" | head -1 | awk '{print $2}' || true)"
fi
if test -z "$identity"; then
  echo "warning: no Apple Development certificate found, signing ad-hoc" >&2
  identity="-"
fi
codesign --force --sign "$identity" --timestamp=none "$app"

"$lsregister" -f "$app"

echo "Installed $app ($bundle_id)"
