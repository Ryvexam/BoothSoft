# ADR-0007: Native spooler workers and print accounting

Status: **Accepted (source implemented, hardware unverified)**

## Context

BoothSoft must print through vendor OS drivers on Windows, macOS and Linux without blocking the GUI or allowing repeated requests to bypass event quotas. An operating system reporting "job accepted" does **not** mean a physical photo was printed. Vendor DNP drivers may have proprietary status and completion indications not exposed by a generic spooler.

## Decision

1. Print is **disabled by default** and requires an administrator-configured printer name and event quota.
2. Reserve an idempotent job in the persistent ledger **before** attempting OS print submission.
3. Use a bounded Rust worker for all blocking printing calls. On Windows it attempts native GDI `StartDocW`/`StretchDIBits`; on macOS/Linux it invokes the standard CUPS `lp` command with separately passed arguments (not a shell).
4. Store `Pending` before submission and `Submitted` once the OS accepts it. Both statuses consume quota. `Confirmed` means a human operator has verified the physical print; `Cancelled` is permitted only after confirming no print occurred.
5. Admin-only additional allowance is `ceil(included * 0.05)` prints, not enabled by server data or an untrusted theme.
6. If an operation fails ambiguously, **do not** automatically release the quota reservation.

## Consequences

- Guest capture and UI are not stalled while Windows/CUPS submits a print.
- Crash and spooler failures are conservative and may under-print, requiring admin reconciliation.
- Printer paper size, crop, borderless output, profiles and native DNP completion signals **require device-specific validation**.
- Quota enforcement inside BoothSoft is not an OS-wide print restriction. Deployment must prevent print access from other OS processes/users (printer permissions, kiosk account, OS hardening).
- Windows backend isolates required `unsafe` GDI code inside a small `cfg(windows)` module. It must pass Windows CI and printer integration testing before a stable release.
