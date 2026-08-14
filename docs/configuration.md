# Configuration reference

The default configuration path is `/etc/phpfpm-auto-optimize.conf`; choose a
different regular, non-symlink file with `--config`. Unknown keys and malformed
lines are rejected. During privileged application, the file must be owned by
root and must not be writable by group or other users.

Precedence is built-in defaults, configuration file, then CLI arguments.
`pool_dir` and `log_file` may be repeated. Supplying either option on the CLI
replaces all configured values of that type.

Custom layouts used with `--apply` require an instance mapping:

```text
instance=POOL_DIR|PHP_FPM_BINARY|MASTER_CONFIG|SYSTEMD_SERVICE
```

The service field is optional for `--no-reload`. Validation always passes the
recorded master config using `php-fpm -tt -y`, preventing accidental validation
of an unrelated PHP installation.

See `packaging/phpfpm-auto-optimize.conf` for every supported key and its
default. Percentages are integers. `overcommit_percent=100` enforces the direct
calculated capacity; larger values must be an explicit operational decision.

## Exit codes

| Code | Meaning |
|---:|---|
| 0 | Successful operation or no recommendation under `--check` |
| 1 | Invalid input, unsafe state, validation failure, or apply failure |
| 2 | `--check` found recommended changes |

## JSON schema

`--json` is dry-run only. The top-level `schema_version` permits consumers to
reject incompatible future output. `status` is either `no_changes` or
`changes_recommended`. Consumers should ignore unknown fields.

## Monitoring

`monitor_seconds` enables repeated observation before calculation, while
`sample_interval` controls the interval and is limited to 60 seconds. The tool
records peak worker concurrency, aggregate FPM PSS where available (RSS
fallback), lowest available system
memory, and per-pool peaks when a pool name uniquely identifies a process.
Recommendations retain 25 percent headroom above an observed pool peak and
remain constrained by the global memory budget.
