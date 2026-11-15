# ADR-0009: Keep the public BoothSoft core offline-only

**Status:** Accepted; supersedes ADR-0005 for the public repository.

## Context

Commercial event provisioning belongs to deployment-specific forks. It does not belong in the generic photo and printing engine.

## Decision

Remove event-network contracts and any API client from BoothSoft OSS. Keep photo capture, local sessions, themes, printing, quotas and local administration independent of network access. A fork may supply its own adapter, never by inserting master credentials into rental kiosks.

## Consequences

No network dependency or sync-related startup overhead. Users configure events and themes locally. Fork integration is owned separately and cannot be advertised as available in the public app.
