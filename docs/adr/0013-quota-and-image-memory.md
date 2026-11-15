# ADR-0013: Stage print quota changes and reuse owned image buffers

- Date: 2026-11-09
- Status: Accepted

## Context

Print quotas are authorization state, whereas an egui numeric drag control updates on each pointer event. Directly mutating the ledger before a deliberate Save produces an in-memory limit that disagrees with the durable reservation state. Furthermore, JPEG pipelines that clone each RGB image and re-encode an identical no-frame image cost RAM and CPU unnecessarily.

## Decision

1. Stage the included-print count in UI state, keyed by event ID; the ledger is not modified until an administrator saves it.
2. Reject saving a lower included quota than the number of **confirmed plus outstanding** jobs. Persist changes before success is reported and restore the previous state on failed writes.
3. Persist the optional 5% allowance immediately when an authorized administrator unlocks it; retain fail-closed behavior on errors.
4. Move the captured RGB image into the compositor. Blend directly in the existing buffer if its dimensions already match the selected frame; otherwise allocate only for the required cover crop.
5. When no frame exists, reuse the already encoded JPEG rather than encoding a second copy.

## Tradeoffs

- A conservative quota policy prevents administrators from reducing a quota below outstanding jobs, even when the device has not physically printed them. These jobs must be reconciled first.
- Copying an unframed JPEG still performs filesystem I/O, but avoids a second compute-intensive JPEG encode. The second file can eventually be replaced by a checked hard link when filesystem and permission semantics are validated.
- Additional unit tests verify buffer ownership but cannot replace real profiler measurements or OS/device tests.

## Acceptance criteria

- Cropping a landscape/portrait camera feed into an arbitrary window produces valid UV coordinates in [0, 1].
- Quota modification does not take effect until a successful save; lower-than-used quota fails closed.
- Unlocking extra prints survives application restart and cannot silently succeed on a filesystem error.
- No-frame export JPEGs contain identical bytes to the captured source JPEG.
