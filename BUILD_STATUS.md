# Build and validation status — 0.16.0

**Release class:** Open-source developer preview — not an installer/hardware-tested release.

## Delivered source changes

- Shared bounded JSON reader and crash-conscious file replacement with file synchronization.
- Bounded JPEG/PNG decode paths, shared between contact sheets, theme frames and native Windows printing.
- In-place BGR conversion with row padding on Windows, avoiding a second full-size print image allocation.
- Rust regression tests for replacements, size limits, invalid JPEG and Windows DIB conversion.
- Version alignment across Cargo, WiX and macOS bundle.
- Architecture, documentation, changelog, agent instructions and ADR-0016.

## Validated in this environment

- TOML parsing and theme PNG metadata checks.
- Source/manifest reference consistency, archives and ZIP CRC.

## Not validated

Cargo/rustc are unavailable: compilation, rustfmt, unit tests, Clippy and native performance measurements could not be executed. Hardware-dependent USB camera streaming, CUPS/GDI drivers, DNP-specific options and MSI installation still need testing. No CPU, RAM or GPU percentage improvement is claimed.

## Required release gate

```bash
cargo fmt --all
cargo fmt --all --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings -D dead_code
cargo build --release
cargo run --release -- --demo --windowed
cargo run --release --bin boothsoft-bench -- 30
```

Follow [`docs/TEST_PLAN.md`](docs/TEST_PLAN.md), inspect the Windows MSI upgrade and sign/notarize distribution artifacts before public release.

## Publication documentation refresh

The documentation-only repackaging preserves `Cargo.toml` version **0.16.0** and the existing Rust sources. It adds a GitHub-ready landing page, getting-started guide, community policies and structured issue/PR templates. The original Apache-2.0 LICENSE remains unchanged. These are documentation improvements, **not** evidence of successful build/test/hardware validation.
