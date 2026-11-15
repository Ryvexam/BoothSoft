# ADR-0016 — File replacement and bounded media decode

**Status:** Accepted in v0.16 source; runtime tests pending

## Context

The kiosk processes large images and persists printer reservations. A truncated or unexpected local file must never reset a printer quota or administrative credential. GDI printing historically retained a decoded RGB image and a second full BGR image in memory simultaneously.

## Decision

1. Use `durable_file::replace` for local JSON snapshots: exclusive temporary creation, file synchronization and same-directory rename. Fail closed on write failures. Do not claim full power-loss durability across all filesystems; verify the final storage solution on the target devices.
2. Bound JSON reads before deserialization. Never silently reset ledger or authentication state after corruption.
3. Decode theme frames with explicit `image::Limits`, and decode print JPEGs through a single shared format/dimension-limited entry point. JPEG allocations may be decoder-specific; caps reduce risk rather than guaranteeing a total process memory ceiling.
4. Convert the owned Windows GDI RGB buffer in place, expanding for padded DIB scanlines only when necessary, rather than retaining a separate full BGR buffer.

## Consequences

- Stronger process-crash behavior and predictable allocation budgets with no additional runtime dependencies.
- File synchronization adds I/O latency on administrative changes and print state transitions, not to the preview loop.
- Limits restrict huge images, requiring a conscious policy change if a future camera exceeds the supported size.
- The print driver may still allocate independently. Real measurements and spooler/physical-print tests remain mandatory.
