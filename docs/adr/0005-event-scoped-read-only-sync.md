# ADR-0005: Single-event, read-only back-office integration (SUPERSEDED by ADR-0009)

**Status:** Proposed — contract schema only

## Context

A physical booth is untrusted hardware. Distributing a central administrator API key (even encrypted with a locally available decryption key) would create a fleet-wide compromise risk.

## Decision

Each booth may eventually obtain a revocable, narrowly scoped credential that permits **read/download access to exactly one assigned event**. It may retrieve manifests, approved PNG frames and static event settings only. Verify signatures, revision, expiry, asset hashes and approved origins before caching. Never embed a global master secret or automatically upload photos, printer counts or telemetry through this API.

## Consequences

Provisioning, signature verification, credential storage, rotation and transport remain to be implemented and audited. The booth will continue to work using the last verified local event snapshot. Future writable telemetry, if requested, will require a separate threat model and permission boundary.
