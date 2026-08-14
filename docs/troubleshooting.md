# Troubleshooting

## No pool directories found

Supply one or more `--pool-dir` options and confirm the process can read the
configuration. A directory must contain a pool with `pm.max_children`.

## Warning ignored as ambiguous

Multiple PHP trees contain the same pool name. Bind the log explicitly:

```bash
phpfpm-auto-optimize \
  --log-file '/etc/php/8.3/fpm/pool.d=/var/log/php8.3-fpm.log'
```

## No validator or service found

Application is rolled back. Confirm the expected PHP-FPM binary and systemd
unit exist. For a custom layout, provide `--instance
'DIR|BINARY|MASTER_CONFIG|SERVICE'`. `--no-reload` installs only after explicit
master-config validation; it does not bypass a missing validator.

## Recovering a backup

List runs and their status with `--list-backups`. New-format runs include a manifest and can be
restored using `--restore ID --no-reload`. Then run the relevant PHP-FPM binary
with `-tt -y MASTER_CONFIG` and reload the affected service. Restoration is
transactional and validates automatically, but deliberately does not activate
configuration.
