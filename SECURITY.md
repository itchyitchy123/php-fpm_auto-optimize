# Security policy

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x | Yes |
| Bash prototype | No |

Security fixes are provided for the latest tagged release. Administrators
should upgrade rather than continuing to use older releases for production
planning.

## Reporting a vulnerability

Do not open a public issue for vulnerabilities involving privilege boundaries,
unsafe paths, configuration injection, evidence spoofing, or command execution.
Use the repository host's private security-advisory feature and include the
affected version, reproduction steps, impact, and any suggested mitigation.

Reports will be acknowledged within seven days. A fix and coordinated release
timeline will be shared after the issue is reproduced. Please avoid accessing
systems or data you do not own.

## Operational model

FPM Lens reads PHP-FPM configuration and `/proc`, but does not install into
`/etc` or reload services. `render` writes beneath an explicit staging
directory. Treat policy, evidence, and plan files as security-sensitive input;
review staged fragments and validate them with the matching `php-fpm -tt`
before deployment. Observation may require elevated access to inspect worker
processes, but planning and fixture-based review should run unprivileged.

## Threat model

- Policy, evidence, and plan artifacts are untrusted until parsed and
  validated. Unknown fields, inconsistent totals, duplicate pools, unsafe pool
  names, invalid bounds, and infeasible plans are rejected.
- Status URLs are operator-supplied network destinations. Responses have short
  deadlines, a 1 MiB limit, strict required fields, and never directly control
  rendered text. Prefer a loopback-only endpoint with web-server access control.
- Source directory names never become staged path components. Rendering uses a
  SHA-256 directory identifier, rejects symlink traversal, and removes only
  obsolete files recorded in a strictly validated manifest.
- The plan digest identifies exact content but is not an authenticity
  signature. Move artifacts over an authenticated channel and use release
  provenance attestations to verify downloaded binaries.
- `validate --php-fpm` executes only the binary path explicitly supplied by the
  operator. It does not invoke shell parsing, install files, or reload services.
