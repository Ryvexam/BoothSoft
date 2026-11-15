# ADR-0015: Bounded image crop and crash-safe photo lifecycle

**Status:** Accepted for v0.15 source; native test results pending

## Context

The previous cover-layout implementation enlarged the full source before cropping it to the print canvas. An extreme mismatch between portrait and landscape aspect ratios could allocate an intermediate image many times larger than either input or output. The photo saver also attempted to rename an encoded JPEG while its encoder/file handle was still alive; this is unreliable on Windows. Retake deleted the photo before its updated session manifest was durably saved. Finally, rapid sessions could reuse a timestamp-based ID.

## Decision

- Compute the centered crop in *source coordinates* using bounded integer arithmetic, then resize an immutable `SubImage` view directly to the final output size. Avoid all oversize intermediate resizes.
- Finish and drop `JpegEncoder` and its owned file before renaming the temporary JPEG. Remove partial temporary files if encoding or rename fails.
- When retaking, commit the manifest without the shot first, rolling back the in-memory mutation on persistence failure. Delete orphaned JPEGs only after a successful manifest commit; cleanup failure is non-fatal and can leave an orphan.
- Include a process-local monotonic counter in session IDs, and create each session directory exclusively instead of silently reusing an existing directory.
- Release the live camera GPU texture after a successful still preview and when entering maintenance. Recreate it when the camera produces the next preview.

## Consequences

Peak temporary allocation for cover-layout scaling is bounded by the final output buffer instead of the worst-case scaled full source. JPEG rename behavior is more portable across Windows/macOS/Linux. Retakes preserve the source photo after a failed manifest update; manifest-first deletion can leave orphan files after a crash, which is preferable to a manifest referencing a missing image. Rapid session creation does not overwrite previously recorded sessions.

These are *source-level* improvements. Their actual peak RSS, render latency, native compile status and printer interoperability require measurement and validation on the three operating systems before any production claim.
