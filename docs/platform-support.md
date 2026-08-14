# Platform support

| Platform family | Pool discovery | Validator | Activation |
|---|---|---|---|
| cPanel EA-PHP | `/opt/cpanel/ea-php*/root/etc/php-fpm.d` | EA-PHP binary | cPanel restart script |
| Debian/Ubuntu | `/etc/php/*/fpm/pool.d` | Versioned PHP-FPM binary | Versioned systemd unit |
| RHEL/AlmaLinux/Rocky | `/etc/php-fpm.d` | `php-fpm` | `php-fpm.service` |
| Remi parallel PHP | `/etc/opt/remi/php*/php-fpm.d` | Remi binary | Remi systemd unit |
| Source/package layout | `/usr/local/etc/php-fpm.d` | `php-fpm` | `php-fpm.service` |

Automated tests exercise generic behavior on Debian, Ubuntu, and AlmaLinux.
Package-specific service and validator naming can vary; always inspect a dry run
and verify the service mapping on a new platform before applying.

Bash 4.4+, GNU coreutils, procps, `find`, `awk`, `sort`, `flock`, and GNU `date`
are required. `systemctl` is required for non-cPanel activation.
