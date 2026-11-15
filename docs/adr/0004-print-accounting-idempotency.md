# ADR-0004: Crash-conservative print accounting

**Status:** Accepted for prototype ledger only; production printing blocked

## Context

An ambiguous print spooler outcome must not free reserved capacity. A crash between persisting completed counts and changing a job from Pending to Confirmed can otherwise double-increment a reservation on retry.

## Decision

Reserve a Pending job before invoking a future print adapter. Persist `completed` together with its unique `confirmed_job_ids` in one ledger document, then persist the job's Confirmed status. If confirmation is retried after a partial crash, do not increment twice. Pending ambiguity consumes quota until explicit verified reconciliation.

## Consequences

Crash boundaries may conservatively count both Completed and Pending, temporarily denying legitimate prints. The current JSON store is not a multi-process ACID database; external OS printing and disk tampering are not controlled. A future adapter must prove completion and restrict access to the spooler before any commercial deployment.
