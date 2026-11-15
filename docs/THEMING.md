# Theme authoring

Themes live at `themes/<theme-id>/`. They are pure, declarative configuration; they must not run scripts or network requests.

## Naming

The directory name is the stable identifier used by `--theme`. Use only lowercase `a-z`, digits, `_`, `-`. Keep it short. Never rename the folder without updating deployment configuration.

## Manifest schema (v1)

| Key | Format | Purpose |
| --- | --- | --- |
| `schema_version` | integer `1` | Manifest version |
| `name` | 1–80 Unicode characters, no control chars | Display branding |
| `colors.background` | `#RRGGBB` | Admin background |
| `colors.foreground` | `#RRGGBB` | Text |
| `colors.primary` | `#RRGGBB` | Main admin highlights |
| `colors.accent` | `#RRGGBB` | Camera shutter / primary actions |
| `colors.surface` | `#RRGGBB` | Admin panel surface |
| `colors.muted` | `#RRGGBB` | Secondary labels |
| `assets.logo` | relative PNG path | Logo displayed in admin; guest rendering pending |
| `assets.frame` | relative PNG path | Frame over review and export |
| `ui.shutter_label` | 1–40 characters | Future localized label (validated only) |
| `ui.retake_label` | 1–40 characters | Future localized label (validated only) |
| `ui.finish_label` | 1–40 characters | Future localized label (validated only) |

## Assets

Images must be inside the theme folder, PNG, at most 16 MiB, dimensions at most 4096×4096. Asset filename components permit lowercase ASCII letters, digits, `_`, `-` and `.` only; hidden files are forbidden. The example assets are in `assets/`.

Photo frames should be RGBA PNG with a transparent middle. For best visual quality, match aspect ratio to the camera and intended print size. The compositor center-crops the photo to match the frame. The supplied PNG frame uses 1200×800 pixels.

## AI-assisted branding

Each theme includes an `AGENTS.md` that an agent should read before making edits. Restrict the agent to that theme directory. Do not add credentials, access tokens, arbitrary URLs, or absolute local paths. Validate `cargo test --all-targets` after changes and inspect the actual running UI on each target platform.

## Future event assets

BoothSoft does not fetch remote assets or embed API credentials. Fork-specific integrations must implement their own independently reviewed asset verification and cache, separate from the public theme loader.

## In-app theme selection

Themes are discovered from `themes/<id>/config.toml` only when administration opens or the operator selects **Refresh folders**. Only valid directories appear in the menu. A symlinked theme folder, a missing config, invalid colors, a symlinked asset, unsupported PNG content or invalid relative paths prevents that theme from being offered.

In Maintenance (shortcut `Ctrl+Shift+D`), use the **Theme** dropdown to preview a theme. Click **Apply theme** to save its folder ID and activate the validated palette, logo and photo frame without a restart. The active theme persists in the app's local `settings.json` as `theme_id`. CLI `--theme <id>` temporarily overrides the saved value on launch; it does not write a new preference.

A folder copy can be detected with **Refresh folders**; BoothSoft does not fetch or install themes from the internet. The theme catalogue is not rescanned during camera frames. Do not use themes for executable code, arbitrary network URLs or API secrets.


## Import a ZIP theme in the application

In **Maintenance** (`Ctrl+Shift+D`), unlock the administrator PIN, then choose **Import ZIP theme**. Select a local `.zip`. BoothSoft validates and extracts it on a bounded background worker, refreshes the theme list and highlights the new theme. Use **Apply theme** to activate it; imports do **not** change the active theme automatically.

Accepted ZIP structures (no remote URLs):

```text
warm-studio.zip             # flat: ZIP basename becomes theme ID
├── config.toml
├── AGENTS.md               # optional documentation, not executed
└── assets/
    ├── logo.png
    └── frame.png

# Alternative: exactly one top-level theme directory
my-wedding.zip
└── wedding-night/          # folder name becomes theme ID
    ├── config.toml
    └── assets/{logo.png,frame.png}
```

**ID:** lowercase ASCII letters, digits, `-` and `_`, up to 48 characters, no reserved Windows device names. On import, an existing ID cannot be replaced, including the bundled theme (`default`). To update a custom theme, use a new ID or maintain it manually with trusted filesystem access.

**Untrusted archive restrictions:** ZIP up to 32 MiB, at most 64 entries, at most 32 MiB extracted in total; PNG at most 16 MiB each with dimensions at most 4096×4096; manifest at most 32 KiB; optional `AGENTS.md` at most 16 KiB; at most 32 PNG assets. Only `config.toml`, `AGENTS.md`, and relative PNG files under `assets/` are accepted. Paths with `..`, absolute locations, Unicode confusables, `%`, `\`, drive prefixes, symlinks, special filesystem entries, duplicates or unexpected files are rejected. The importer never runs code inside ZIPs. Treat imported `AGENTS.md` as untrusted text, not agent instructions.

**Persistence / installers:** bundled themes remain at `<installation>/themes`. Third-party ZIPs install under the current OS user's local data directory (`BoothSoft/themes`, as returned by Rust `dirs::data_local_dir()`). This works when the MSI installs under locked-down Program Files, and it avoids modifying signed app bundles on macOS. Both sources appear in the same selector. Installed/bundled themes take precedence.

Import intentionally uses a native OS file picker. It is created only when the administrator requests it, not at startup or per frame. Linux may require an XDG Desktop Portal backend. ZIP import is available only after local administrator authorization; it does **not** grant filesystem-level security against an OS administrator.
