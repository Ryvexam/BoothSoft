# ADR-0008: Fail-closed local settings and validated session manifests

Status: **Accepted**

## Context

Corrupted local settings previously fell back silently to defaults, including an empty admin-PIN hash. Session manifests stored on disk also contained unvalidated path segments that could be used to access files outside the session directory if tampered with.

## Decision

- A missing settings file is expected on first run and yields defaults. Any **existing** settings file that cannot be read or parsed now causes startup to fail with an error. Never reset authentication state after a parse failure.
- Session folders and manifest IDs must match, use a strict ASCII identifier, and not be symbolic links.
- Each stored shot filename follows the `photo-NN.jpg` convention, without traversal or path separators.
- Bound the number and size of session manifests read during startup, and ignore invalid historical records.
- Do not rely on the JSON file's own `session.id` to select an arbitrary filesystem path.

## Consequences

An administrator may need to repair corrupted settings or restore a known-good backup rather than seeing a usable but insecure default. Existing historical manifests with unsupported names are ignored, not automatically deleted. Full defense against a hostile OS administrator requires OS-level security beyond this parser.
