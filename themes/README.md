# BoothSoft themes

Each child directory is one selectable, self-contained theme containing `config.toml`, `AGENTS.md`, and an `assets/` folder. `default` ("Party") is the only theme shipped; a brand theme is installed per booth.

The folder name is the theme ID (`--theme my-brand`); the `name` field is only a display label. Theme assets must be relative PNG files inside the theme folder. No remote URL, absolute path, symlink or parent traversal is permitted.


## Selecting installed themes

Open the Maintenance screen (`Ctrl+Shift+D`) and unlock it with the admin PIN. In the **Theme** section, choose a theme, inspect its logo/colors and click **Apply theme**. The choice is persisted across launches. Use **Refresh folders** after copying in a new theme folder. Invalid manifests and unsafe assets are excluded rather than executed.

## Import directly from the application

In Maintenance, authorized administrators can use **Import ZIP theme**. The ZIP needs `config.toml` and PNG files under `assets/`; optional `AGENTS.md`. It can contain these at ZIP root (theme ID from `<archive-name>.zip`) or inside a single named theme folder (ID from that folder). Imports are validated and installed to **per-user local BoothSoft data**, not to this bundled themes directory. See `docs/THEMING.md`.
