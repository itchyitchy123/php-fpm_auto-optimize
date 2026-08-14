# PHP-FPM Auto Optimizer

[![Test](https://github.com/itchyitchy123/php-fpm_auto-optimize/actions/workflows/test.yml/badge.svg)](https://github.com/itchyitchy123/php-fpm_auto-optimize/actions/workflows/test.yml)
[![Latest release](https://img.shields.io/github/v/release/itchyitchy123/php-fpm_auto-optimize)](https://github.com/itchyitchy123/php-fpm_auto-optimize/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Safely calculate and apply memory-aware PHP-FPM pool limits across multiple PHP
versions, with dry runs, validation, backups, and transactional rollback.

PHP-FPM Auto Optimizer is a conservative capacity calculator and configuration generator for
cPanel/EA-PHP and common Debian, Ubuntu, RHEL, AlmaLinux, Rocky Linux, Remi,
and source-installed LAMP layouts.

The tool inventories every discovered PHP-FPM pool, estimates worker memory
conservatively, reserves RAM for the OS/Apache/database, and protects pools
that recently reached `pm.max_children`. Its aggregate recommendation is
scaled to the calculated memory capacity. It is a dry run unless `--apply` is
explicitly supplied.

```text
POOL               MODE      BASELINE  CURRENT      NEW WARNINGS  REASON
www                dynamic         20       20       16        0  quiet:conservative reduction
shop               ondemand        40       40       34        3  busy:3 warnings; capacity-scaled

Aggregate recommendation: 50/50 workers
Pools requiring an effective change: 2
Dry run only. No configuration changes were made.
```

## Installation

For an initial evaluation, download a tagged release, verify it against the
published `SHA256SUMS`, and run it directly:

```bash
chmod +x phpfpm-auto-optimize
sudo ./phpfpm-auto-optimize
```

For a system installation from a source checkout:

```bash
sudo make install
man phpfpm-auto-optimize
```

Release tarballs, checksums, example configuration, a man page, shell
completion, and packaging assets are maintained in this repository.
The default policy file is `/etc/phpfpm-auto-optimize.conf`; command-line
arguments override configured values.

## Quick start

```bash
sudo ./phpfpm-auto-optimize
sudo ./phpfpm-auto-optimize --apply
```

For a web and database server sharing 8 GB, a more conservative run might be:

```bash
sudo ./phpfpm-auto-optimize --reserve-percent 40 --target-percent 75
```

Use `--memory-mb` for a container limit that differs from host RAM, or
`--worker-mb` when there are no representative live PHP-FPM workers. Run
`./phpfpm-auto-optimize --help` for all options.

For monitoring and automation, `--json` emits a single machine-readable dry-run
document. `--no-reload` can install and validate an override without activating
it; without that explicit option, failure to find an affected active service is
treated as an apply failure and rolled back.

For monitoring, `--check` exits with status 2 when it recommends changes:

```bash
sudo phpfpm-auto-optimize --check
phpfpm-auto-optimize --json | jq .status
```

To observe a representative workload before calculating recommendations:

```bash
sudo phpfpm-auto-optimize --monitor-seconds 900 --sample-interval 5
```

The report incorporates peak process concurrency, aggregate FPM PSS where
available (RSS fallback), lowest
available memory, and per-pool peaks when process titles are unambiguous. A
window covering real traffic is substantially more useful than an idle snapshot.

## Supported layouts

- cPanel EA-PHP: `/opt/cpanel/ea-php*/root/etc/php-fpm.d`
- Debian/Ubuntu: `/etc/php/*/fpm/pool.d`
- RHEL-compatible: `/etc/php-fpm.d`
- Remi parallel PHP: `/etc/opt/remi/php*/php-fpm.d`
- Source/package layouts: `/usr/local/etc/php-fpm.d`

An uncommon location can be supplied with `--pool-dir`, which is repeatable.
Applying to an uncommon layout also requires an explicit validator mapping:

```bash
sudo phpfpm-auto-optimize --apply \
  --pool-dir /srv/php/etc/php-fpm.d \
  --instance '/srv/php/etc/php-fpm.d|/srv/php/sbin/php-fpm|/srv/php/etc/php-fpm.conf|php-custom-fpm.service'
```
When several PHP installations use the same pool name, bind manually supplied
logs to their configuration tree with `--log-file '/path/to/pool.d=/path/to/error.log'`.
Unbound warnings for an ambiguous pool name are ignored rather than attributed
to the wrong PHP version.

## Safety model

- Dry-run by default; applying requires root and confirmation.
- Writes only `zzz-auto-optimize.conf`, leaving panel/package files untouched.
- Requires and runs the corresponding PHP-FPM binary with `-tt` before reloads.
- Rolls every touched override back if writing, validation, or reload fails.
- Rolls back on shell errors, interruption, or termination during an apply.
- Saves a prior generated override under `/var/backups/phpfpm-auto-optimize`.
- Uses cPanel's PHP-FPM restart script when available.
- Reloads only affected systemd services and reactivates restored configuration
  if a later reload fails.

The optimizer cannot know the true peak memory of an application from an idle
sample. Exercise representative traffic first, inspect the proposed values,
and leave ample RAM for MySQL/MariaDB and Apache. On cPanel, account pool files
may be regenerated by the panel; the separate late-loading override is designed
to avoid editing those managed files.

## Calculation

```text
FPM budget = (RAM - reserve) × target percentage
workers    = FPM budget / 75th-percentile observed PHP-FPM RSS
```

Observed worker size has a 48 MB safety floor, and a 64 MB fallback is used
without enough live samples. Host memory is automatically capped by a cgroup
v1/v2 memory limit when present. Quiet pools can be reduced by at most 20% from
their panel/domain baseline. Saturated pools are protected or modestly raised.
If all proposals exceed the global capacity, their allocation above the
per-pool minimum is scaled proportionally to fit. `--overcommit-percent` can
explicitly relax that constraint for deployments whose pools cannot peak
simultaneously.

For dynamic pools, `pm.start_servers`, `pm.min_spare_servers`, and
`pm.max_spare_servers` are kept at or below the proposed `pm.max_children`, so
the generated configuration remains internally consistent.
Generated overrides are tracked separately from that baseline, so a second run
is stable and reports the active values accurately. Existing process-manager
mode and intentionally low `pm.max_requests` values are preserved.

The worker percentile, lookback period, minimum/maximum children, change
threshold, request cap, and overcommit policy are all configurable; see
`--help`. The report includes process-manager mode, observed sample count,
aggregate allocation, and the reason for each recommendation.

## Rollback

Remove the generated `zzz-auto-optimize.conf` from each pool directory (or
restore its timestamped backup), validate with the corresponding `php-fpm -tt`,
then reload the PHP-FPM service.

New backups contain restoration manifests:

```bash
sudo phpfpm-auto-optimize --list-backups
sudo phpfpm-auto-optimize --restore RUN_ID --no-reload
```

Restoration validates every recorded master configuration and rolls back on
failure, but does not activate configuration automatically. Reload the affected
PHP-FPM services after inspecting the restored files.

## Scope

This project tunes PHP-FPM process-manager capacity only. It does not rewrite
PHP limits, Apache MPM settings, OPcache, or database configuration; those need
workload-specific analysis and should not be inferred solely from total RAM.

## Documentation

- [Configuration and exit codes](docs/configuration.md)
- [Recommendation algorithm](docs/algorithm.md)
- [Supported platforms](docs/platform-support.md)
- [Troubleshooting and recovery](docs/troubleshooting.md)
- [Security policy](SECURITY.md)
- [Contributing](CONTRIBUTING.md)
- [Changelog](CHANGELOG.md)
- [Roadmap](ROADMAP.md)

## Development

Run `make check` to execute syntax validation, ShellCheck, formatting checks,
the regression suite, and diff hygiene. Pull requests run the same checks on
Debian, Ubuntu, and AlmaLinux. Tagged releases are rebuilt automatically and
published with SHA-256 checksums.
