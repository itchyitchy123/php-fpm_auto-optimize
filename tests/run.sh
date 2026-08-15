#!/usr/bin/env bash
set -Eeuo pipefail

report_failure() {
  local status=$?
  printf 'Test failed at line %s: %s\n' "${BASH_LINENO[0]}" "$BASH_COMMAND" >&2
  if [[ ${GITHUB_ACTIONS:-} == true ]]; then
    printf '::error file=tests/run.sh,line=%s::Test command failed: %s\n' \
      "${BASH_LINENO[0]}" "$BASH_COMMAND" >&2
  fi
  exit "$status"
}
trap report_failure ERR

ROOT=$(cd "$(dirname "$0")/.." && pwd)
FIXTURE=$(mktemp -d)
trap 'rm -rf -- "$FIXTURE"' EXIT
POOL_DIR="$FIXTURE/pool.d"
mkdir -p "$POOL_DIR"

[[ $("$ROOT/phpfpm-auto-optimize" --version) == "phpfpm-auto-optimize 0.5.0" ]]
grep -q '^Version: 0.5.0$' "$ROOT/packaging/rpm/phpfpm-auto-optimize.spec"
grep -q 'phpfpm-auto-optimize 0.5.0' "$ROOT/docs/phpfpm-auto-optimize.8"

cat >"$POOL_DIR/10-pools.conf" <<'EOF'
[global]
daemonize = yes

[www]
pm = dynamic
pm.max_children = 10
pm.max_requests = 300
pm.start_servers = 12
pm.min_spare_servers = 4
pm.max_spare_servers = 12

[shop]
pm = ondemand
pm.max_children = 30
pm.max_requests = 0
EOF

# A later baseline fragment without max_children must merge with, not replace,
# the earlier section.
cat >"$POOL_DIR/20-extra.conf" <<'EOF'
[www]
catch_workers_output = yes
EOF

run_optimizer() {
  "$ROOT/phpfpm-auto-optimize" --pool-dir "$POOL_DIR" --log-file "$FIXTURE/error.log" \
    --memory-mb 4000 --reserve-percent 20 --target-percent 80 --worker-mb 64 \
    2>"$FIXTURE/stderr"
  [[ ! -s $FIXTURE/stderr ]] || {
    cat "$FIXTURE/stderr" >&2
    return 1
  }
}

: >"$FIXTURE/error.log"
output=$(run_optimizer)
grep -Eq '^www[[:space:]]+dynamic[[:space:]]+10[[:space:]]+10[[:space:]]+8[[:space:]]+0' <<<"$output"
grep -Eq '^shop[[:space:]]+ondemand[[:space:]]+30[[:space:]]+30[[:space:]]+24[[:space:]]+0' <<<"$output"
if grep -Eq '^global[[:space:]]' <<<"$output"; then exit 1; fi
grep -q 'Dry run only' <<<"$output"

# The active override is current state, but the recommendation remains anchored
# to the baseline. A second run therefore reports no effective changes.
cat >"$POOL_DIR/zzz-auto-optimize.conf" <<'EOF'
[www]
pm.max_children = 8
pm.max_requests = 300
pm.start_servers = 8
pm.max_spare_servers = 8

[shop]
pm.max_children = 24
pm.max_requests = 500
EOF
second=$(run_optimizer)
grep -Eq '^www[[:space:]]+dynamic[[:space:]]+10[[:space:]]+8[[:space:]]+8[[:space:]]+0' <<<"$second"
grep -Eq '^shop[[:space:]]+ondemand[[:space:]]+30[[:space:]]+24[[:space:]]+24[[:space:]]+0' <<<"$second"
grep -q 'Pools requiring an effective change: 0' <<<"$second"

# Recent saturation protects/increases from baseline, regardless of an older
# generated override. GNU date accepts this PHP-FPM timestamp format.
now=$(date '+%d-%b-%Y %H:%M:%S')
for _ in 1 2 3 4 5; do
  printf '[%s] WARNING: [pool www] server reached pm.max_children setting (10)\n' "$now" >>"$FIXTURE/error.log"
done
hot=$(run_optimizer)
grep -Eq '^www[[:space:]]+dynamic[[:space:]]+10[[:space:]]+8[[:space:]]+13[[:space:]]+5[[:space:]]+[0-9]+[[:space:]]+hot:' <<<"$hot"

# An effective fragment that changes only max_requests must retain the earlier
# effective max_children value.
cat >>"$POOL_DIR/zzz-auto-optimize.conf" <<'EOF'
[www]
pm.max_requests = 250
EOF
merged=$(run_optimizer)
grep -Eq '^www[[:space:]]+dynamic[[:space:]]+10[[:space:]]+8[[:space:]]+13[[:space:]]+5[[:space:]]+0' <<<"$merged"

# Recommendations are scaled to the actual global capacity, not merely warned.
: >"$FIXTURE/error.log"
limited=$("$ROOT/phpfpm-auto-optimize" --pool-dir "$POOL_DIR" --log-file "$FIXTURE/error.log" \
  --memory-mb 1280 --worker-mb 64)
grep -q 'Aggregate recommendation: 12/12 workers' <<<"$limited"
grep -q 'capacity-scaled' <<<"$limited"

# Hard maximums are enforced after hysteresis and cannot be restored to an
# above-policy baseline by the change threshold.
printf '[%s] WARNING: [pool shop] server reached pm.max_children setting (30)\n' "$(date '+%d-%b-%Y %H:%M:%S')" >"$FIXTURE/one-warning.log"
capped=$("$ROOT/phpfpm-auto-optimize" --pool-dir "$POOL_DIR" --log-file "$FIXTURE/one-warning.log" \
  --memory-mb 8000 --worker-mb 64 --max-children 25)
grep -Eq '^shop[[:space:]]+ondemand[[:space:]]+30[[:space:]]+24[[:space:]]+25' <<<"$capped"

# JSON mode emits one valid document without human-readable prefixes.
json=$("$ROOT/phpfpm-auto-optimize" --json --pool-dir "$POOL_DIR" --log-file "$FIXTURE/error.log" \
  --memory-mb 4000 --worker-mb 64)
python3 -c 'import json,sys; data=json.load(sys.stdin); assert data["schema_version"] == 1; assert data["program_version"] == "0.5.0"; assert data["status"] in ("no_changes", "changes_recommended"); assert len(data["pools"]) == 2' <<<"$json"

# A bound log attributes a duplicate pool name to the correct PHP tree only.
SECOND_POOL_DIR="$FIXTURE/php-other/pool.d"
mkdir -p "$SECOND_POOL_DIR"
cat >"$SECOND_POOL_DIR/pools.conf" <<'EOF'
[www]
pm = ondemand
pm.max_children = 20
pm.max_requests = 200
EOF
now=$(date '+%d-%b-%Y %H:%M:%S')
printf '[%s] WARNING: [pool www] server reached pm.max_children setting (10)\n' "$now" >"$FIXTURE/bound.log"
bound=$("$ROOT/phpfpm-auto-optimize" --pool-dir "$POOL_DIR" --pool-dir "$SECOND_POOL_DIR" \
  --log-file "$POOL_DIR=$FIXTURE/bound.log" --memory-mb 8000 --worker-mb 64)
[[ $(grep -c '^www.*1.*0.*protect:recent warning' <<<"$bound") == 1 ]]

# Timed monitoring samples live PHP-FPM process titles and feeds observed peak
# concurrency into the recommendation.
cat >"$FIXTURE/block-worker" <<'EOF'
#!/usr/bin/env bash
exec 3<>"$1"
read -r -t 5 <&3 || :
EOF
declare -a worker_pids=()
for _ in 1 2 3 4 5 6 7 8; do
  worker_fifo="$FIXTURE/worker-$_.fifo"
  mkfifo "$worker_fifo"
  bash -c 'exec -a "php-fpm: pool www" bash "$1" "$2"' _ \
    "$FIXTURE/block-worker" "$worker_fifo" &
  worker_pids+=("$!")
done
monitored=$("$ROOT/phpfpm-auto-optimize" --monitor-seconds 1 --sample-interval 1 \
  --pool-dir "$POOL_DIR" --log-file "$FIXTURE/empty.log" --memory-mb 4000 --worker-mb 64)
for pid in "${worker_pids[@]}"; do
  kill "$pid" 2>/dev/null || :
  wait "$pid" 2>/dev/null || :
done
grep -Eq 'Observed: peak workers [8-9]' <<<"$monitored"
grep -q 'monitored-peak:8' <<<"$monitored"

# Configuration values load safely and CLI values take precedence.
cat >"$FIXTURE/optimizer.conf" <<EOF
memory_mb=2000
worker_mb=64
pool_dir=$SECOND_POOL_DIR
log_file=$FIXTURE/empty.log
EOF
: >"$FIXTURE/empty.log"
configured=$("$ROOT/phpfpm-auto-optimize" --config "$FIXTURE/optimizer.conf")
grep -q 'Memory: 2000 MB' <<<"$configured"
overridden=$("$ROOT/phpfpm-auto-optimize" --config "$FIXTURE/optimizer.conf" --memory-mb 3000)
grep -q 'Memory: 3000 MB' <<<"$overridden"
printf 'unsupported_key=yes\n' >"$FIXTURE/invalid.conf"
if "$ROOT/phpfpm-auto-optimize" --config "$FIXTURE/invalid.conf" >/dev/null 2>&1; then
  echo "expected unknown config key to fail" >&2
  exit 1
fi

# Check mode has a stable status 2 when recommendations are pending.
if "$ROOT/phpfpm-auto-optimize" --check --pool-dir "$SECOND_POOL_DIR" \
  --log-file "$FIXTURE/empty.log" --memory-mb 3000 --worker-mb 64 >/dev/null; then
  check_status=0
else
  check_status=$?
fi
[[ $check_status == 2 ]]

# A no-change apply exits before writing, validation, or service management.
: >"$FIXTURE/error.log"
cat >"$POOL_DIR/zzz-auto-optimize.conf" <<'EOF'
[www]
pm.max_children = 8
pm.max_requests = 300
pm.start_servers = 8
pm.max_spare_servers = 8
[shop]
pm.max_children = 24
pm.max_requests = 500
EOF
before=$(sha256sum "$POOL_DIR/zzz-auto-optimize.conf")
no_change=$("$ROOT/phpfpm-auto-optimize" --apply --yes --pool-dir "$POOL_DIR" \
  --log-file "$FIXTURE/error.log" --memory-mb 4000 --worker-mb 64)
grep -q 'Nothing to apply' <<<"$no_change"
[[ $(sha256sum "$POOL_DIR/zzz-auto-optimize.conf") == "$before" ]]

# A successful install-only transaction validates and writes coherent dynamic
# settings without requiring a service manager in the test environment.
mkdir -p "$FIXTURE/bin"
cat >"$FIXTURE/bin/php-fpm" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" > "${VALIDATOR_ARGS_FILE:-/dev/null}"
[[ $1 == -tt && $2 == -y && -r $3 && ! -e ${FAIL_VALIDATION_FILE:-/definitely/missing} ]]
EOF
chmod +x "$FIXTURE/bin/php-fpm"
: >"$FIXTURE/php-fpm.conf"
sed -i 's/pm.max_children = 8/pm.max_children = 9/' "$POOL_DIR/zzz-auto-optimize.conf"
installed=$(PATH="$FIXTURE/bin:$PATH" VALIDATOR_ARGS_FILE="$FIXTURE/validator.args" "$ROOT/phpfpm-auto-optimize" --apply --yes --no-reload \
  --instance "$POOL_DIR|$FIXTURE/bin/php-fpm|$FIXTURE/php-fpm.conf" \
  --pool-dir "$POOL_DIR" --log-file "$FIXTURE/error.log" --memory-mb 4000 --worker-mb 64 \
  --backup-dir "$FIXTURE/backups")
grep -q 'Applied' <<<"$installed"
grep -q '^pm.start_servers = 8$' "$POOL_DIR/zzz-auto-optimize.conf"
grep -q '^pm.max_spare_servers = 8$' "$POOL_DIR/zzz-auto-optimize.conf"
grep -Fqx -- "-tt -y $FIXTURE/php-fpm.conf" "$FIXTURE/validator.args"

# Applied runs have discoverable manifests and can restore their exact prior
# generated override without activating it.
backup_id=$("$ROOT/phpfpm-auto-optimize" --backup-dir "$FIXTURE/backups" --list-backups | awk 'NR == 1 {print $1}')
[[ -n $backup_id && -f $FIXTURE/backups/$backup_id/manifest.tsv ]]
"$ROOT/phpfpm-auto-optimize" --backup-dir "$FIXTURE/backups" --list-backups | grep -q $'\tinstalled_not_reloaded$'
"$ROOT/phpfpm-auto-optimize" --backup-dir "$FIXTURE/backups" --restore "$backup_id" \
  --no-reload >/dev/null
grep -q '^pm.max_children = 9$' "$POOL_DIR/zzz-auto-optimize.conf"

# A validation failure during restore atomically returns every target to its
# pre-restore state.
sed -i 's/pm.max_children = 9/pm.max_children = 7/' "$POOL_DIR/zzz-auto-optimize.conf"
before=$(sha256sum "$POOL_DIR/zzz-auto-optimize.conf")
: >"$FIXTURE/fail-validation"
if FAIL_VALIDATION_FILE="$FIXTURE/fail-validation" "$ROOT/phpfpm-auto-optimize" \
  --backup-dir "$FIXTURE/backups" --restore "$backup_id" --no-reload >/dev/null 2>&1; then
  echo "expected invalid restored configuration to roll back" >&2
  exit 1
fi
[[ $(sha256sum "$POOL_DIR/zzz-auto-optimize.conf") == "$before" ]]
rm -f -- "$FIXTURE/fail-validation"

# Mutating operations refuse to race another apply/restore process.
exec 8>"$FIXTURE/backups/.lock"
flock -n 8
if "$ROOT/phpfpm-auto-optimize" --backup-dir "$FIXTURE/backups" --restore "$backup_id" \
  --no-reload >/dev/null 2>&1; then
  echo "expected concurrent restore to be refused" >&2
  exit 1
fi
flock -u 8
exec 8>&-

# With a real change, a missing validator must fail after writing and restore
# the exact prior override.
sed -i 's/pm.max_children = 8/pm.max_children = 9/' "$POOL_DIR/zzz-auto-optimize.conf"
before=$(sha256sum "$POOL_DIR/zzz-auto-optimize.conf")
if "$ROOT/phpfpm-auto-optimize" --apply --yes --pool-dir "$POOL_DIR" \
  --log-file "$FIXTURE/error.log" --memory-mb 4000 --worker-mb 64 \
  --backup-dir "$FIXTURE/backups" >/dev/null 2>&1; then
  echo "expected missing validator to fail apply" >&2
  exit 1
fi
[[ $(sha256sum "$POOL_DIR/zzz-auto-optimize.conf") == "$before" ]]

if "$ROOT/phpfpm-auto-optimize" --pool-dir "$FIXTURE/missing" >/dev/null 2>&1; then
  echo "expected missing directory to fail" >&2
  exit 1
fi
if "$ROOT/phpfpm-auto-optimize" --pool-dir "$POOL_DIR" --reserve-percent 100 >/dev/null 2>&1; then
  echo "expected invalid percentage to fail" >&2
  exit 1
fi

echo "All tests passed"
