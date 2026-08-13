#!/usr/bin/env bash
set -Eeuo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
FIXTURE=$(mktemp -d)
trap 'rm -rf -- "$FIXTURE"' EXIT

mkdir -p "$FIXTURE/pool.d"
cat > "$FIXTURE/pool.d/www.conf" <<'EOF'
[global]
daemonize = yes

[www]
user = www-data
pm = dynamic
pm.max_children = 10

[shop]
user = shop
pm = ondemand
pm.max_children = 30
EOF

output=$("$ROOT/phpfpm-auto-optimize" \
  --pool-dir "$FIXTURE/pool.d" \
  --memory-mb 1000 \
  --reserve-percent 20 \
  --target-percent 100 \
  --worker-mb 40)

grep -Eq '^www[[:space:]]+10[[:space:]]+5[[:space:]]' <<<"$output"
grep -Eq '^shop[[:space:]]+30[[:space:]]+15[[:space:]]' <<<"$output"
! grep -Eq '^global[[:space:]]' <<<"$output"
grep -q 'Dry run only' <<<"$output"
[[ ! -e "$FIXTURE/pool.d/zzz-auto-optimize.conf" ]]

# A later optimizer override is the effective configuration on subsequent
# runs. The CURRENT column must reflect it instead of repeating the baseline.
cat > "$FIXTURE/pool.d/zzz-auto-optimize.conf" <<'EOF'
[www]
pm.max_children = 5

[shop]
pm.max_children = 15
EOF

second_output=$("$ROOT/phpfpm-auto-optimize" \
  --pool-dir "$FIXTURE/pool.d" \
  --memory-mb 1000 \
  --reserve-percent 20 \
  --target-percent 100 \
  --worker-mb 40)

grep -Eq '^www[[:space:]]+5[[:space:]]+5[[:space:]]' <<<"$second_output"
grep -Eq '^shop[[:space:]]+15[[:space:]]+15[[:space:]]' <<<"$second_output"

if "$ROOT/phpfpm-auto-optimize" --pool-dir "$FIXTURE/missing" >/dev/null 2>&1; then
  echo "expected missing directory to fail" >&2
  exit 1
fi

echo "All tests passed"
