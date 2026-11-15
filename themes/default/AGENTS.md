# AI Agent Instructions — `default` theme

This directory is a self-contained BoothSoft theme. **Do not modify application source code** to customize it.

## Files

- `config.toml`: theme manifest; schema version must stay `1`.
- `assets/logo.png`: transparent brand logo.
- `assets/frame.png`: transparent print overlay; transparent center preserves the photo.

## Editing workflow

1. Change only values in `config.toml` or files inside `assets/`.
2. Use `#RRGGBB` colors, keep visual contrast accessible, and avoid decorative text over the live camera.
3. Keep asset paths relative to this theme root. Never use `../`, absolute paths, URLs, symlinks, or secrets.
4. Asset filenames must be lowercase ASCII and use only `a-z`, `0-9`, `_`, `-`, and `.`; directory names use `a-z`, `0-9`, `_`, `-`.
5. Only PNG assets are supported for `logo` and `frame`. Prefer small PNGs; avoid oversized textures.
6. Validate using `cargo test` and run `cargo run --release -- --theme default` on the target OS.
7. Test the full-screen live view, seven-second countdown, review composition and printing integration with actual hardware.

## Theme selection

`--theme default` loads `themes/default/config.toml` relative to the installation theme directory. The CLI argument is **a folder name, not a path**.

Do not introduce external network fetches in the theme loader; authenticated event downloads are a separate component.


### Administrator theme picker

An authenticated BoothSoft operator can choose this theme from the **Theme** dropdown and click **Apply theme**. The saved selection persists in the local app settings. To see edits to `config.toml` or new assets, click **Refresh folders** and apply the theme again. The dropdown loads only validated theme folders; it never runs code or downloads assets. Keep the main camera screen distraction-free.
