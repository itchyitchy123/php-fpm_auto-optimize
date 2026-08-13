#!/usr/bin/env bash
set -Eeuo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
FIXTURE=$(mktemp -d)
trap 'rm -rf -- "$FIXTURE"' EXIT
POOL_DIR="$FIXTURE/pool.d"
mkdir -p "$POOL_DIR"

cat > "$POOL_DIR/10-pools.conf" <<'EOF'
[global]
daemonize = yes

[www]
pm = dynamic
pm.max_children = 10
pm.max_requests = 300

[shop]
pm = ondemand
pm.max_children = 30
pm.max_requests = 0
EOF

# A later baseline fragment without max_children must merge with, not replace,
# the earlier section.
cat > "$POOL_DIR/20-extra.conf" <<'EOF'
[www]
catch_workers_output = yes
EOF

run_optimizer() {
  "$ROOT/phpfpm-auto-optimize" --pool-dir "$POOL_DIR" --log-file "$FIXTURE/error.log" \
    --memory-mb 4000 --reserve-percent 20 --target-percent 80 --worker-mb 64 \
    2>"$FIXTURE/stderr"
  [[ ! -s $FIXTURE/stderr ]] || { cat "$FIXTURE/stderr" >&2; return 1; }
}

: > "$FIXTURE/error.log"
output=$(run_optimizer)
grep -Eq '^www[[:space:]]+10[[:space:]]+10[[:space:]]+8[[:space:]]+0' <<<"$output"
grep -Eq '^shop[[:space:]]+30[[:space:]]+30[[:space:]]+24[[:space:]]+0' <<<"$output"
! grep -Eq '^global[[:space:]]' <<<"$output"
grep -q 'Dry run only' <<<"$output"

# The active override is current state, but the recommendation remains anchored
# to the baseline. A second run therefore reports no effective changes.
cat > "$POOL_DIR/zzz-auto-optimize.conf" <<'EOF'
[www]
pm.max_children = 8
pm.max_requests = 300

[shop]
pm.max_children = 24
pm.max_requests = 500
EOF
second=$(run_optimizer)
grep -Eq '^www[[:space:]]+10[[:space:]]+8[[:space:]]+8[[:space:]]+0' <<<"$second"
grep -Eq '^shop[[:space:]]+30[[:space:]]+24[[:space:]]+24[[:space:]]+0' <<<"$second"
grep -q 'Pools requiring an effective change: 0' <<<"$second"

# Recent saturation protects/increases from baseline, regardless of an older
# generated override. GNU date accepts this PHP-FPM timestamp format.
now=$(date '+%d-%b-%Y %H:%M:%S')
for _ in 1 2 3 4 5; do
  printf '[%s] WARNING: [pool www] server reached pm.max_children setting (10)\n' "$now" >> "$FIXTURE/error.log"
done
hot=$(run_optimizer)
grep -Eq '^www[[:space:]]+10[[:space:]]+8[[:space:]]+13[[:space:]]+5[[:space:]]+hot:' <<<"$hot"

# An effective fragment that changes only max_requests must retain the earlier
# effective max_children value.
cat >> "$POOL_DIR/zzz-auto-optimize.conf" <<'EOF'
[www]
pm.max_requests = 250
EOF
merged=$(run_optimizer)
grep -Eq '^www[[:space:]]+10[[:space:]]+8[[:space:]]+13[[:space:]]+5' <<<"$merged"

# A no-change apply exits before writing, validation, or service management.
: > "$FIXTURE/error.log"
cat > "$POOL_DIR/zzz-auto-optimize.conf" <<'EOF'
[www]
pm.max_children = 8
pm.max_requests = 300
[shop]
pm.max_children = 24
pm.max_requests = 500
EOF
before=$(sha256sum "$POOL_DIR/zzz-auto-optimize.conf")
no_change=$("$ROOT/phpfpm-auto-optimize" --apply --yes --pool-dir "$POOL_DIR" \
  --log-file "$FIXTURE/error.log" --memory-mb 4000 --worker-mb 64)
grep -q 'Nothing to apply' <<<"$no_change"
[[ $(sha256sum "$POOL_DIR/zzz-auto-optimize.conf") == "$before" ]]

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
