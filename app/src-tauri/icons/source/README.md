# Approved MNO application icon

`MNO.png` is the owner's approved 1254×1254 source, copied byte-for-byte from
Desktop/MNO.png. Keep this original; do not redraw or generate replacement artwork.

The source is RGB with near-black pixels outside its rounded cream surface.
`MNO-transparent.png` is an RGBA working copy: only near-black pixels connected to
the canvas boundary were made transparent (maximum RGB channel value <= 24 in
sRGB). There is no crop, lettering change, added symbol or palette adjustment.
The cream surface, proportions and existing rounded silhouette are retained.

Regenerate the desktop assets from `app/` with the existing Tauri CLI:

```sh
npm run tauri icon -- src-tauri/icons/source/MNO-transparent.png
```

Keep the desktop PNGs, ICO, ICNS and existing Square*/StoreLogo outputs. The CLI
also emits Android/iOS folders; they are not part of this desktop-only change.
No new image-generation or build dependency is needed.

- macOS: multi-resolution `icon.icns`, preserving the approved rounded silhouette
  with transparent corners; no additional platform mask or background is baked in.
- Windows: multi-resolution `icon.ico`; Square*/StoreLogo PNGs kept consistent.
- Linux: square RGBA PNGs at the configured sizes, including 64px and `icon.png`.

Production and DEV retain their existing separate identities and data roots.
The icon is shared branding and does not replace the DEV title/badge.
