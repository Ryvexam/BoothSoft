# ADR-0003: Optimize runtime before binary size

**Status:** Accepted; measured on 15/11/2026 (see below)

## Context

The previous `release` profile used `opt-level = "z"` to minimize binary size, but this choice disables loop vectorization and can slow the image pipeline. Small on-disk executables do not prove lower RSS or CPU.

## Decision

Make `opt-level = 2`, thin LTO, one codegen unit and `panic = "abort"` the standard release profile. Keep `release-small` (`opt-level = "z"`) as an explicitly comparable alternative. Publish no quantitative claim until both profiles are measured on target hardware.

## Consequences

Binary size may increase. RGB filtering and image processing may benefit from runtime optimization but performance depends on CPU and crate implementations. CI compiles the normal release; maintainers can benchmark the size variant separately.

## Measurement (15/11/2026)

Xvfb 1280×800, synthetic 720p MJPEG camera, 20 s of live view, two runs each:

| Profile | Executable | Live CPU (whole process) | UI thread | RSS |
| --- | --- | --- | --- | --- |
| `opt-level = 2`, thin LTO | 13.1 MB | 38.7 / 41.0 % | 6.2 / 7.1 % | 133 MB |
| `opt-level = 3`, fat LTO | 12.6 MB | 40.3 / 40.1 % | 6.9 / 6.6 % | 133 MB |

CPU is within noise (most of it is the software GL rasteriser of the test VM and
libjpeg-turbo, which do not depend on the Rust profile). Fat LTO gives a 4 % smaller
executable, so less code to map: **the release profile is now `opt-level = 3`,
`lto = "fat"`**, one codegen unit, `panic = "unwind"` (required by `mjpeg.rs`).
`scripts/build-booth.sh` adds `-C target-cpu=haswell` for the booth PC only.
