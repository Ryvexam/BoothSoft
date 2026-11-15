# ADR-0002: Latest-frame camera buffer

**Status:** Accepted in v0.9 source, hardware verification pending

## Context

Camera previews are ephemeral. Keeping decoded preview frames in a FIFO wastes RAM and can display stale frames if the GUI is delayed. However, full-resolution captures must never be silently dropped.

## Decision

Use a worker thread and a single overwriteable `Arc<Mutex<Option<RgbImage>>>` preview slot. Notify egui on new images via `Context::request_repaint` rather than periodic 100 ms GUI polling. Keep captured images and camera status in a separate channel. Stop preview decoding during Review, and resume without re-opening the device when practical.

## Consequences

Out-of-date previews are intentionally dropped; the UI always takes the newest frame. The preview slot is bounded, but platform camera drivers, decompression and GPU uploads can still allocate buffers. Full-resolution capture events block their worker if the event channel fills, ensuring captures are not silently discarded. Real devices and reconnect behavior must be profiled.
