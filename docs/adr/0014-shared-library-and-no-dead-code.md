# ADR-0014: Shared library, conservative job transitions, and no dead code

**Status:** Accepted in v0.14 source; runtime validation pending

## Context

The UI entrypoint had grown to over a thousand lines. CLI binaries used `#[path]` to copy engine source modules into separate compilation units, then suppressed dead-code warnings. Some print state transitions changed in-memory state before checking whether persistence succeeded. Auxiliary photo exports held full-resolution images simultaneously, increasing peak memory usage.

## Decision

- Introduce one reusable Rust library (`src/lib.rs`). Split the executable into the CLI/bootstrap entrypoint, guest kiosk module, and separate protected admin module.
- Remove duplicate path-based module inclusions and `#[allow(dead_code)]`; use warnings-as-errors in CI. Remove unused public helpers when no consumer exists.
- Process contact-sheet photos sequentially, avoid unnecessary RGB clones when creating display textures, and avoid an immediate redundant preview decode after a still capture.
- Persist job transitions before acknowledging success, restoring their previous in-memory values on failure.
- Interpret the +5% allowance strictly as `floor(included / 20)`; previous rounding-up could exceed 5% for small reservations. Supersedes the rounding rule in ADR-0007.

## Consequences

The module graph is easier to review and diagnostic binaries share exactly the same adapters as the kiosk. This reduces unnecessary allocations and prevents persistence failures from leaving misleading transient job state. Existing reservations with fewer than 20 prints receive no whole extra copy from the 5% allowance. The impact on actual CPU, RAM, GPU and build behavior remains unmeasured until native CI runs.
