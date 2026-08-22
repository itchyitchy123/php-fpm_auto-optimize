# Security policy

## Supported versions

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
