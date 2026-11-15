# ADR-0001: Native Rust kiosk UI

**Status:** Accepted

## Context

Photobooth installations should run offline on modest Windows, macOS and Linux hardware. The guest screen is mostly a camera preview and a few large controls; shipping a web browser introduces another runtime and complexity.

## Decision

Use Rust with egui/eframe. No Electron, WebView, local JavaScript runtime, web app or embedded HTTP server. Keep the guest screen camera-first and load branding from `themes/<id>/` rather than coding a vendor's identity into the engine.

## Consequences

Native camera/printer integration remains platform-dependent. GPU/text rasterization still has non-zero runtime overhead, and no minimum RAM consumption can be claimed without hardware measurements. Theme customization is data-driven rather than arbitrary code execution.
