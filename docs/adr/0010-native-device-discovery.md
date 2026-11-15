# ADR-0010: Discover native devices only in administration

**Status:** Proposed until validated on all supported operating systems.

## Context

Manually typing a printer name or camera index is error prone. Polling OS devices every preview frame wastes CPU and can block slow drivers.

## Decision

One background on-demand scan provides native webcam indices/names through `nokhwa::query` and installed printer queues through Windows `EnumPrintersW` level 4 or CUPS `lpstat -p`. The scan occurs at first admin opening and on explicit refresh. The UI persists the selected camera index and printer queue name. Unknown printers cannot be enabled in administration.

## Consequences

No periodic scanning or native spool APIs on the UI thread. Device reordering, missing camera permissions, print-queue availability and physically complete output require OS-specific tests. OS-provided webcam functionality is distinct from proprietary DSLR tethering.
