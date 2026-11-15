# ADR-0012 — Strict local ZIP theme import

Status: Accepted (2026-11-09)

## Context

Open-source users should install independent themes without modifying the Rust application or the read-only MSI application directory. ZIPs are untrusted binary input and risk path traversal, symlinks, duplicate entries, decompression bombs and content spoofing.

## Decision

Expose an administrator-only native file picker. Parse the archive using `zip` only on request and outside the render thread. Accept strictly declarative TOML and bounded PNG assets with an optional `AGENTS.md` (documentation, never executed). Validate entries before extraction, enforce compressed/decompressed byte and entry count limits, refuse symlinks, special files, traversal and overwrites, stage in the current user's data directory, validate via the same theme loader used at startup, then rename the validated theme to its final directory. Keep theme IDs lowercase ASCII and use either a single wrapper folder name or a sanitized flat ZIP basename.

The catalogue merges bundled and user-local themes, prioritizing bundled themes. Imported themes require an explicit Apply action. No API/network fetches and no ZIP data reaches the GPU directly.

## Consequences

Adds the on-demand `zip` and `rfd` crates and OS picker dependencies. Theme import may temporarily allocate buffers for decompression and PNG header validation; camera loop stays event-driven. Import cannot defend against an attacker with full filesystem/OS administrator control. To update a theme, use a new ID or a trusted manual filesystem procedure (no automatic replacement). Cross-platform tests are required before a stable release.
