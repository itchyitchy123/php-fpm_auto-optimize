# Changelog

All notable changes are documented here. This project follows [Semantic
Versioning](https://semver.org/) and the [Keep a Changelog](https://keepachangelog.com/)
format.

## [Unreleased]

## [0.1.0] - 2026-08-22

### Added

- A clean Rust implementation named FPM Lens with typed inventory, policy,
  evidence, plan, and rendering layers.
- Keyboard-driven pool selection and editing for child bounds, explicit child
  targets, request recycling, idle timeout, and request timeout.
- Per-pool memory modeling, confidence-aware recommendations, infeasibility
  reporting, staged atomic rendering, and plan SHA-256 identifiers.
- Strict Rust CI, MSRV checks, release builds, architecture documentation, and
  end-to-end fixtures.

### Changed

- Missing observations preserve current capacity instead of being interpreted
  as a quiet workload.
- Global allocation uses each pool's representative worker memory rather than
  one host-wide worker estimate.

## Bash prototype

Versions 0.3.0 through 0.5.0 below describe the historical Bash prototype.

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
- Container checks now install their complete toolchain, while artifact and
  release jobs use checksum-verified pinned lint tools.

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

[Unreleased]: https://github.com/itchyitchy123/fpm-lens/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/itchyitchy123/fpm-lens/releases/tag/v0.1.0
