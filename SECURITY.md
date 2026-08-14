# Security policy

## Supported versions

Security fixes are provided for the latest tagged release. Administrators
should upgrade rather than continuing to run older releases as root.

## Reporting a vulnerability

Do not open a public issue for vulnerabilities involving privilege boundaries,
unsafe paths, configuration injection, backup disclosure, or command execution.
Use the repository host's private security-advisory feature and include the
affected version, reproduction steps, impact, and any suggested mitigation.

Reports will be acknowledged within seven days. A fix and coordinated release
timeline will be shared after the issue is reproduced. Please avoid accessing
systems or data you do not own.

## Operational model

`--apply` requires root because it writes PHP-FPM configuration and reloads
services. Dry runs should be executed first. Only trusted, root-owned
configuration and pool directories should be supplied to privileged runs.
