## Summary

Describe the problem, the change, and why this design is preferable to a simpler alternative.

## Scope

- [ ] No unrelated behavior or dead code added
- [ ] No secrets, customer photos, private event data or proprietary competitor assets
- [ ] Existing theme configuration remains compatible (or migration documented)

## Verification

- [ ] `cargo fmt --all --check`
- [ ] `cargo check --all-targets`
- [ ] `cargo test --all-targets`
- [ ] `cargo clippy --all-targets -- -D warnings -D dead_code`
- [ ] `cargo run --bin boothsoft-theme-check -- themes` (when touching themes)
- [ ] Affected OS/device and failure scenarios tested (or marked as not tested)

**Testing evidence / commands:**

## Resource and security impact

New allocations, threads, dependencies, OS permissions, and measurements (or `not measured`):

## Documentation

- [ ] README, `AGENTS.md`, architecture and/or relevant ADR updated if needed

Target branch: `dev` (not `main`).
