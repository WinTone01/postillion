# Packaging

## Linux (implemented)

```sh
scripts/package-linux.sh            # release build (thin LTO, stripped)
PROFILE=debug scripts/package-linux.sh   # fast smoke package
```

Produces `target/package/postillion-<version>-linux-<arch>.tar.gz` containing:

- `postillion` — the binary (headed by default; `postillion headless` runs the engine alone)
- `postillion.desktop` — XDG desktop entry
- `postillion.png` — 1024×1024 Postillion app icon
- `install.sh` — installs into `~/.local/{bin,share/applications,share/icons}`

The release profile in the root `Cargo.toml` sets `lto = "thin"` and
`strip = "symbols"` for distribution builds.

## macOS

```sh
scripts/package-macos.sh    # → target/package/postillion-<version>-macos-<arch>.dmg
```

Builds the release binary, assembles `Postillion.app` (Info.plist + icns), ad-hoc
signs it (set `CODESIGN_IDENTITY` for a real Developer ID), and wraps it in a
dmg. The auto-update tarball retains an internal `Postillion.app` path so older
installed builds can update into Postillion. CI runs this on tags
(`.github/workflows/release.yml`). The manual steps it automates, for reference
(run on a macOS host — gpui needs Metal; no cross-build from Linux):

1. Build the universal (or per-arch) binary:
   ```sh
   cargo build --release -p postillion --target aarch64-apple-darwin
   cargo build --release -p postillion --target x86_64-apple-darwin
   lipo -create -output postillion \
     target/aarch64-apple-darwin/release/postillion \
     target/x86_64-apple-darwin/release/postillion
   ```
2. Assemble the bundle:
   ```sh
   mkdir -p Postillion.app/Contents/{MacOS,Resources}
   cp postillion Postillion.app/Contents/MacOS/postillion
   sed "s/__VERSION__/$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')/" \
     dist/macos/Info.plist > Postillion.app/Contents/Info.plist
   ```
3. Icon: generate `postillion.icns` from `dist/macos/icon-1024.png` (the macOS-shaped
   variant of the artwork — squircle mask, margins, and shadow pre-baked, since
   `sips` can't apply an alpha mask) and place it at
   `Postillion.app/Contents/Resources/postillion.icns`:
   ```sh
   mkdir postillion.iconset && sips -z 256 256 dist/macos/icon-1024.png --out postillion.iconset/icon_256x256.png
   iconutil -c icns postillion.iconset -o Postillion.app/Contents/Resources/postillion.icns
   ```
4. Sign + notarize (required for distribution):
   ```sh
   codesign --deep --force --options runtime --sign "Developer ID Application: …" Postillion.app
   xcrun notarytool submit Postillion.zip --keychain-profile … --wait
   xcrun stapler staple Postillion.app
   ```
5. Ship as a `.dmg` (`hdiutil create -volname Postillion -srcfolder Postillion.app -ov -format UDZO Postillion.dmg`).
