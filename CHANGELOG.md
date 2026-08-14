# Changelog

All notable changes are documented here. This project follows [Semantic
Versioning](https://semver.org/) and the [Keep a Changelog](https://keepachangelog.com/)
format.

## [Unreleased]

## [0.5.0] - 2026-08-14

### Added

- Continuous integration, release automation, packaging assets, and project
  governance documentation.
- Configuration-file support, process locking, versioned JSON output, check
  mode, and backup inspection/restoration commands.
- Timed PHP-FPM worker and memory monitoring with peak-aware recommendations.
- Explicit instance-to-validator/master-config/service mappings.

### Fixed

- Hard child limits can no longer be undone by change hysteresis.
- Custom layouts cannot validate an unrelated default PHP-FPM configuration.
- Restore and failed reload paths now restore and revalidate complete state.
- Current-process cgroup limits, repeated log timestamp parsing, release
  contents, and non-root release jobs.

## [0.4.0] - 2026-08-14

### Added

- Hard aggregate memory-capacity enforcement and configurable overcommit.
- Cgroup-aware memory detection and configurable worker percentile.
- Pool-directory-bound error logs and safe duplicate-pool handling.
- Transactional signal rollback, affected-service reloads, and JSON reports.
- Coherent dynamic process-manager overrides.

## [0.3.0]

- Initial public release with dry-run recommendations, generated overrides,
  validation, backups, and rollback.

[Unreleased]: https://github.com/itchyitchy123/php-fpm_auto-optimize/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/itchyitchy123/php-fpm_auto-optimize/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/itchyitchy123/php-fpm_auto-optimize/releases/tag/v0.4.0
