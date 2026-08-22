# Roadmap

## Toward 1.0

- Collect PSS where kernel permissions permit and record measurement method.
- Import FPM status-page queue, active-process, and saturation evidence.
- Validate generated fragments against matching PHP-FPM binaries in an
  unprivileged staging workflow.
- Publish JSON Schemas for evidence and plan artifacts.
- Add snapshot comparisons and historical trend reports.
- Package signed binaries for common Linux architectures.

## Deliberately out of scope for now

- Automatic installation into `/etc`.
- Automatic service reloads or panel API calls.
- Treating one idle snapshot as a production sizing signal.

The default workflow will remain read-only and review-first.
