# CopyCraft app icon

## Source: `AppIcon.icon/` (Icon Composer)

The app icon is the Icon Composer document `AppIcon.icon/` (the macOS 26 Liquid Glass format).
Open it in Icon Composer (Xcode 26 › Open Developer Tool › Icon Composer) to see or change it.

- **`icon.json`**: the background is a solid fill, sRGB #07112D
  (`extended-srgb:0.02745,0.06667,0.17647,1.00000`), the same navy as `ICON_BACKGROUND` in
  `src/macos_welcome.rs`. There are three groups, bottom to top, each holding one layer with a
  neutral shadow:
  1. **Board** (`Assets/board.svg`): the plate of the clipboard, translucent (glass) at 0.4.
  2. **Clip** (`Assets/clip.svg`): the metal tab with its slot, over the top edge of the board,
     translucent at 0.2.
  3. **Braces** (`Assets/braces.svg`): the cyan-to-blue `[ ]` mark, opaque.
- **The layers** are square 1024 × 1024 SVGs with a transparent background and no mask. There is
  no corner radius, drop shadow or filter in them: Icon Composer adds the system mask, the glass,
  the specular highlight and the shadow. The shapes follow the glass design of `icon.icns`.
- **No dark, clear or tinted variants**: the system derives those appearances from the layers.
  Do not add per-appearance fills or layers.

To change a shape, edit the layer SVG and copy the same `<g>` into `icon.svg` (see below), or
change it in Icon Composer and save; keep the layer file names, which the test and the script
rely on.

## `icon.svg`: flat copy

`icon.svg` is a flat preview of the document: the #07112D background with the three layers
pasted in verbatim, bottom to top. It is not used for the build. The test
`app_icon_layers_and_flat_svg_stay_in_sync` (`src/icon.rs`) fails when a layer and `icon.svg`
differ, when `icon.json` stacks the layers in another order or changes the fill, or when a
baked-in corner (`rx="224"`) or a filter comes back.

## Build: `scripts/build_app_icon.sh`

`scripts/build_app_icon.sh copycraft [CopyCraft.app]` compiles the icon into the `.app` before
signing (the release workflow, `scripts/build_store_pkg.sh` and the CI `app-icon` job run it). It
needs macOS with actool from Xcode 26. It writes:

- `Contents/Resources/Assets.car`, the asset catalog icon (`CFBundleIconName = AppIcon`), with
  the Liquid Glass rendering on macOS 26;
- `Contents/Resources/AppIcon.icns`, every size from 16 to 1024 (`--include-all-app-icons`,
  `CFBundleIconFile = AppIcon`), for Finder and the DMG volume icon.

The script takes the first source it finds here: `AppIcon.icon/`, then `AppIcon.appiconset/`,
then `icon.icns`. With `AppIcon.icon/` in the tree, the other two are never used for the bundle.
`Cargo.toml` has no cargo-bundle `icon`: cargo-bundle only copies an `.icns`, and the bundle has
to get its icon from the Icon Composer document.

## `icon.icns`: welcome window only

`icon.icns` (one 1024 px image) is no longer part of the bundle. The welcome window builds it
into the binary (`include_bytes!` in `src/macos_welcome.rs`) and shows it feathered on the
#07112D window. Keep the file. To make it match the Icon Composer render, run this on a Mac with
Xcode 26 from the repository root, check the welcome window and commit the new file:

```sh
out="$(mktemp -d)" && xcrun actool apps/copycraft/assets/AppIcon.icon --compile "$out" \
  --platform macosx --target-device mac --minimum-deployment-target 26.0 \
  --app-icon AppIcon --include-all-app-icons \
  --output-partial-info-plist "$out/partial.plist" && \
  cp "$out/AppIcon.icns" apps/copycraft/assets/icon.icns
```

## Accent colour

`AccentColor.colorset/` (sRGB #6EB4ED, #7DBFF2 in dark mode) is compiled into the same
`Assets.car`, and the script sets `NSAccentColorName`.
