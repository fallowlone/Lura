#!/bin/bash
set -e

echo "==> Building Folio CLI (Rust)..."
export PATH="$HOME/.cargo/bin:$PATH"
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi
cargo build --release

echo "==> Setting up bundle directories..."
APP_NAME="Lura"
EXT_NAME="LuraPreview"
BUILD_DIR="build"
APP_DIR="$BUILD_DIR/$APP_NAME.app"
EXT_DIR="$APP_DIR/Contents/PlugIns/$EXT_NAME.appex"

rm -rf "$BUILD_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Frameworks"
mkdir -p "$EXT_DIR/Contents/MacOS"
mkdir -p "$EXT_DIR/Contents/Frameworks"

echo "==> Copying libfolio.dylib into host app (editor preview)..."
cp target/release/libfolio.dylib "$APP_DIR/Contents/Frameworks/libfolio.dylib"

echo "==> Copying libfolio.dylib into Extension bundle..."
cp target/release/libfolio.dylib "$EXT_DIR/Contents/Frameworks/libfolio.dylib"

echo "==> Copying Info.plist files..."
cp quicklook/HostInfo.plist "$APP_DIR/Contents/Info.plist"
cp quicklook/ExtensionInfo.plist "$EXT_DIR/Contents/Info.plist"

echo "==> Compiling Host App (SwiftUI)..."
HOST_SWIFT=(
    quicklook/HostApp/LuraDebugLog.swift
    quicklook/HostApp/LuraTemplates.swift
    quicklook/HostApp/RecentFilesStore.swift
    quicklook/HostApp/FolioRenderFFI.swift
    quicklook/HostApp/LuraFileDocument.swift
    quicklook/HostApp/WebPreviewRepresentable.swift
    quicklook/HostApp/LuraEditorView.swift
    quicklook/HostApp/LuraAppModel.swift
    quicklook/HostApp/WelcomeView.swift
    quicklook/HostApp/LuraAppDelegate.swift
    quicklook/HostApp/LuraApp.swift
)
swiftc "${HOST_SWIFT[@]}" \
    -parse-as-library \
    -o "$APP_DIR/Contents/MacOS/$APP_NAME" \
    -target arm64-apple-macos13.0 \
    -framework SwiftUI \
    -framework AppKit \
    -framework UniformTypeIdentifiers \
    -framework WebKit

echo "==> Compiling Quick Look Extension (Swift)..."
swiftc quicklook/Extension/PreviewViewController.swift \
    -parse-as-library \
    -module-name "$EXT_NAME" \
    -o "$EXT_DIR/Contents/MacOS/$EXT_NAME" \
    -framework Cocoa -framework Quartz -framework WebKit \
    -target arm64-apple-macos12.0 \
    -Xlinker -e -Xlinker _NSExtensionMain

echo "==> Creating Entitlements..."
cat << 'EOF' > "$BUILD_DIR/Entitlements.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    <true/>
    <key>com.apple.security.network.client</key>
    <true/>
    <key>com.apple.security.files.user-selected.read-write</key>
    <true/>
</dict>
</plist>
EOF

cat << 'EOF' > "$BUILD_DIR/InheritEntitlements.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    <true/>
    <key>com.apple.security.inherit</key>
    <true/>
</dict>
</plist>
EOF

echo "==> Code Signing..."
# Required for modern macOS otherwise Quick Look will refuse to load the extension
codesign --force --sign - --entitlements "$BUILD_DIR/InheritEntitlements.plist" "$APP_DIR/Contents/Frameworks/libfolio.dylib"
codesign --force --sign - --entitlements "$BUILD_DIR/InheritEntitlements.plist" "$EXT_DIR/Contents/Frameworks/libfolio.dylib"
codesign --force --sign - --entitlements "$BUILD_DIR/Entitlements.plist" "$EXT_DIR"
codesign --force --sign - --entitlements "$BUILD_DIR/Entitlements.plist" "$APP_DIR"

echo "==> Installing to ~/Applications..."
mkdir -p ~/Applications
cp -R "$APP_DIR" ~/Applications/
APP_PATH="$HOME/Applications/$APP_NAME.app"

echo "==> Removing Folio/build/Lura.app staging copy (only ~/Applications copy remains; avoids two Lura entries in Launchpad)."
rm -rf "$APP_DIR"

echo "==> Registering Quick Look Extension..."
# Register host app in LaunchServices to ensure UTIs are known
/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister -f "$APP_PATH"
# Register plugin explicitly with verbose output
pluginkit -v -a "$APP_PATH/Contents/PlugIns/$EXT_NAME.appex" || true
qlmanage -r
qlmanage -r cache

echo "==> Done!"
echo "--------------------------------------------------------"
echo "DIAGNOSTICS: Checking why pluginkit might be failing..."
pluginkit -v -m -i com.fallowlone.lura-document-app.PreviewExtension || true
echo "Retrieving pkd logs (might take a few seconds)..."
log show --predicate 'process == "pkd"' --last 5m | grep -i -E -A 2 -B 2 'Lura|Folio' || echo "No pkd logs found"
echo "--------------------------------------------------------"
echo "Launch: open \"$APP_PATH\""
echo "Host UI debug (live): log stream --style compact --info --predicate 'subsystem == \"com.fallowlone.lura-document-app\"'"
echo "Host UI debug (file):  tail -f \"\$HOME/Library/Containers/com.fallowlone.lura-document-app/Data/Library/Caches/LuraDebug/ui.log\""
echo "Quick Look test: qlmanage -p examples/hello.fol"
