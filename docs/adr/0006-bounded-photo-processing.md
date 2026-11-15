# ADR-0006: Bounded background photo processing

Status: **Accepted (source implemented, runtime unverified)**

## Context

The previous capture path encoded high-resolution JPEGs and composed PNG frame overlays synchronously in egui's `App::update`. On slow mini-PCs this can freeze the interface for multiple frames and increase latency under repeated captures. Starting an unlimited number of image threads would increase peak RSS and risk out-of-memory conditions.

## Decision

Use **one** dedicated Rust image-processing worker with a `sync_channel(1)` of owned full-resolution `PhotoCommand`s. The guest UI only schedules one capture at a time, and its state moves to `Processing` while the worker filters, encodes two JPEGs (original and framed), and returns a bounded-size preview. The worker wakes egui on completion. Original and composite JPEGs are saved before the session metadata is committed. Retakes remove the original and its composite.

Theme frame pixels are shared as `Arc<RgbaImage>` between UI and worker, preventing a full-frame clone every capture. The worker never touches egui state.

## Consequences

- UI remains responsive during expensive JPEG encoding and composition (subject to hardware scheduling).
- Bounded in-flight photo work prevents an unbounded queue of large RGB buffers.
- Peak memory still contains decoded raw capture, composited full image, and preview; measure and optimize later.
- A process crash after writing JPEGs and before session registration can leave orphan files. A future recovery pass may reconcile them; do not delete unknown data automatically.
- Dedicated worker remains idle and blocked on a channel between captures; no CPU polling.
