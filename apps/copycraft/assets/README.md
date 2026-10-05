# Copycraft app icon

`scripts/build_app_icon.sh copycraft` compiles the icon into the `.app` (Assets.car + AppIcon.icns) before signing. Source, first match in this folder:

1. **`AppIcon.icon/`** — Icon Composer document (macOS 26+ Liquid Glass icon). Drop the `.icon` package here when you have one; needs actool from Xcode 26.
2. **`AppIcon.appiconset/`** — asset catalog PNGs + `Contents.json`.
3. **`icon.icns`** — current fallback (also referenced from `Cargo.toml` for cargo-bundle).

There is no Icon Composer source in the tree yet; releases use `icon.icns` until an `AppIcon.icon` is added.
