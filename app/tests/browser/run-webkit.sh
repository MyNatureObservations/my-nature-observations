#!/bin/sh
# Isolated native host for the actual editor/reader fixture; no journal backend.
set -eu
app_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
fixture_root=$(mktemp -d /private/tmp/mno-stage2b-webkit-XXXXXX)
mkdir -p "$fixture_root/src" "$fixture_root/icons"
cp "$app_root/src-tauri/icons/32x32.png" "$fixture_root/icons/icon.png"
cp "$app_root/src-tauri/Cargo.lock" "$fixture_root/Cargo.lock"
cat > "$fixture_root/Cargo.toml" <<'TOML'
[package]
name = "mno-stage2b-webkit"
version = "0.1.0"
edition = "2024"
[build-dependencies]
tauri-build = { version = "2", features = [] }
[dependencies]
tauri = { version = "2", features = [] }
TOML
cat > "$fixture_root/build.rs" <<'RUST'
fn main() { tauri_build::build() }
RUST
cat > "$fixture_root/src/main.rs" <<'RUST'
#[tauri::command]
fn report(value: String) {
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/results.json"), value).unwrap();
}
fn main() {
    tauri::Builder::default().invoke_handler(tauri::generate_handler![report])
        .run(tauri::generate_context!()).expect("synthetic fixture");
}
RUST
cat > "$fixture_root/tauri.conf.json" <<'JSON'
{"productName":"MNO Stage 2B WebKit","version":"0.1.0","identifier":"org.mynatureobservations.stage2bfixture20261031","build":{"devUrl":"http://127.0.0.1:1432"},"app":{"windows":[{"title":"MNO Stage 2B — synthetic WebKit fixture","url":"http://127.0.0.1:1432/tests/browser/stage2b.html","width":1200,"height":850}],"security":{"csp":null}}}
JSON
printf 'Native fixture sources and results: %s\nStart the standalone Vite fixture on port 1432 first.\n' "$fixture_root"
cd "$fixture_root"
# The temporary lock may shed unused application dependencies; app locks stay intact.
CARGO_TARGET_DIR="$app_root/src-tauri/target" cargo build --offline
bundle="$fixture_root/MNO Stage 2B WebKit.app"
mkdir -p "$bundle/Contents/MacOS"
cp "$app_root/src-tauri/target/debug/mno-stage2b-webkit" "$bundle/Contents/MacOS/"
cat > "$bundle/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>mno-stage2b-webkit</string><key>CFBundleIdentifier</key><string>org.mynatureobservations.stage2bfixture20261031</string><key>CFBundleName</key><string>MNO Stage 2B WebKit</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>
PLIST
open -n "$bundle"
