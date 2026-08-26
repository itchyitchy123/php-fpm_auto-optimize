# FPM Lens user guide

FPM Lens measures PHP-FPM pools, creates an explainable memory-bounded plan,
and stages configuration overrides for review. It never installs files into
`/etc` and never reloads PHP-FPM.

This guide describes a production-safe workflow from installation through
rollback. Run collection during representative traffic and validate every
staged change with the PHP-FPM binary used by that installation.

## 1. Install and verify

Download the binary matching the host architecture:

This requires a published GitHub release. If the project has not published a
release yet, use the source-build instructions below.

```bash
arch=$(uname -m)
case "$arch" in
  x86_64) target=x86_64-unknown-linux-musl ;;
  aarch64|arm64) target=aarch64-unknown-linux-musl ;;
  *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac

curl -fLO "https://github.com/itchyitchy123/php-fpm_auto-optimize/releases/latest/download/fpm-lens-$target"
curl -fLO "https://github.com/itchyitchy123/php-fpm_auto-optimize/releases/latest/download/fpm-lens-$target.sha256"
sha256sum -c "fpm-lens-$target.sha256"
```

The checksum is required. If the GitHub CLI is installed, verify the signed
build provenance before installation:

```bash
gh attestation verify "fpm-lens-$target" --repo itchyitchy123/php-fpm_auto-optimize
```

Install the verified (or checksum-verified) binary:

```bash
sudo install -Dm0755 "fpm-lens-$target" /usr/local/bin/fpm-lens
fpm-lens --version
```

Building from source requires Rust 1.85 or newer:

```bash
cargo build --release --locked
sudo install -Dm0755 target/release/fpm-lens /usr/local/bin/fpm-lens
```

## 2. Diagnose the host

```bash
sudo fpm-lens doktor
```

`doktor` checks:

- the host or cgroup memory envelope;
- discovered and readable pool directories;
- pools containing `pm.max_children`;
- procfs access needed for worker memory sampling;
- common PHP-FPM validation binaries on `PATH`.

An essential failed check returns exit code 1. For a nonstandard layout, pass
one or more directories explicitly:

```bash
sudo fpm-lens --pool-dir /srv/php/8.3/pool.d doktor
```

Inspect discovery before collecting evidence:

```bash
sudo fpm-lens inventory
sudo fpm-lens --pool-dir /etc/php/8.3/fpm/pool.d inventory
```

Inventory is JSON and is suitable for saving or processing with `jq`.

## 3. Enable status evidence safely

Process-table inspection supplies worker PSS, falling back to RSS. Active
workers, listen queues, and saturation must come from the PHP-FPM JSON status
page; existing idle workers are intentionally not treated as demand.

Enable a status path in each pool that will be measured:

```ini
[checkout]
pm.status_path = /fpm-status-checkout
```

Expose it through the host's existing web server on a loopback-only listener or
otherwise restrict it to local administrative access. For example, the routing
concept for Nginx is:

```nginx
location = /fpm-status-checkout {
    allow 127.0.0.1;
    allow ::1;
    deny all;
    include fastcgi_params;
    fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
    fastcgi_pass unix:/run/php/php8.3-fpm.sock;
}
```

Validate the web-server configuration before reload. Confirm that JSON is
available only from the intended administrative interface:

```bash
curl -fsS 'http://127.0.0.1/fpm-status-checkout?json' | jq .
```

Do not expose an FPM status page publicly. FPM Lens accepts only `http://`
status URLs, so use loopback or another trusted local network path.

## 4. Create a policy

Start with `fpm-lens.example.toml` and set a realistic reserve for the kernel,
web server, database, cache, queues, monitoring agents, and traffic variance:

```toml
[global]
reserve_memory_mb = 2048
memory_utilization_percent = 80
default_worker_memory_mb = 64
minimum_evidence_samples = 12
maximum_evidence_age_seconds = 86400
minimum_observation_seconds = 900
minimum_status_success_percent = 80
headroom_percent = 25
default_min_children = 2
default_max_children = 100

[pools.checkout]
selected = true
min_children = 6
max_children = 40
max_requests = 500
request_terminate_timeout_seconds = 120
```

Pool names work when unique. If two PHP installations contain the same pool
name, qualify a policy key using the directory and pool name:

```toml
[pools."/etc/php/8.3/fpm/pool.d:www"]
selected = true
min_children = 4
max_children = 30
```

Important policy rules:

- Child limits must be positive and ordered.
- Low-confidence pools are not silently reduced for memory pressure.
- Explicit bounds may still change an out-of-bounds current value; the plan
  explains that decision.
- `max_requests` and timeouts change only when explicitly configured.
- Evidence that is stale, incomplete, too short, or too unreliable remains low
  confidence.

## 5. Collect evidence and assess

The guided read-only workflow collects evidence and immediately builds a plan:

```bash
sudo fpm-lens --policy production.toml assess \
  --samples 180 \
  --interval-seconds 5 \
  --status-url 'checkout=http://127.0.0.1/fpm-status-checkout?json' \
  --save-evidence evidence-$(date +%F).json \
  --save-plan plan-$(date +%F).json
```

Use one `--status-url POOL=URL` option per pool. Qualify duplicate pool names in
the same way as policy keys. Collection should cover a representative load
window; a large sample count over a quiet period is not representative.

Ctrl-C saves partial evidence with `complete = false`. Partial evidence remains
low confidence and includes a warning.

Collection and planning can also be separated:

```bash
sudo fpm-lens observe \
  --samples 720 \
  --interval-seconds 5 \
  --status-url 'checkout=http://127.0.0.1/fpm-status-checkout?json' \
  --output evidence.json

fpm-lens --policy production.toml \
  --evidence evidence.json \
  plan --json --output production.plan.json
```

For a container or deliberate planning envelope, override detected memory:

```bash
fpm-lens --memory-mb 4096 --policy production.toml \
  --evidence evidence.json plan
```

Exit code 2 means the plan was written but is infeasible. Increase the planning
envelope, increase the reserve's accuracy rather than blindly lowering it,
adjust explicit pool constraints, or collect better evidence. Do not deploy an
infeasible plan.

## 6. Understand and compare evidence

```bash
fpm-lens report evidence-2026-08-21.json evidence-2026-08-22.json
fpm-lens compare evidence-2026-08-21.json evidence-2026-08-22.json
```

The main evidence values are:

- `peak_workers`: highest active-process count reported by the status page;
- `listen_queue_peak`: highest queued-request count;
- `saturation_events`: increases in the cumulative `max children reached`
  counter during collection;
- `worker_memory_mb`: p75 worker memory used for planning;
- `memory_p50_mb`, `memory_p95_mb`, and `memory_max_mb`: diagnostic percentiles;
- `memory_measurement`: `pss`, `rss`, or `mixed`;
- `status_samples` / `status_attempts`: endpoint reliability;
- `complete`, `observed_at_unix`, and `observation_seconds`: evidence quality.

Snapshot comparison is not a substitute for latency, throughput, or load-test
results. Retain snapshots from known peak windows and compare them over time.

## 7. Review the plan

For an interactive terminal review:

```bash
sudo fpm-lens --policy production.toml --evidence evidence.json review \
  --save-policy production.reviewed.toml \
  --save-plan production.plan.json
```

Keys:

| Key | Action |
|---|---|
| `↑` / `↓` | Select a pool |
| `Space` | Include or exclude it |
| `Tab` | Select target, bounds, requests, or timeout field |
| `+` / `-` | Adjust the selected value |
| `Enter` | Validate and save both artifacts transactionally |
| `q` / `Esc` | Leave without saving |

For noninteractive review, inspect the explanation and exact changes:

```bash
fpm-lens validate production.plan.json
fpm-lens diff production.plan.json
jq '.warnings, .pools[] | {id, confidence, reasons, evidence, proposed}' \
  production.plan.json
```

## 8. Render and validate staged configuration

Rendering accepts only a feasible, internally consistent plan:

```bash
fpm-lens render production.plan.json --output-dir build/fpm-lens-review
```

The output contains:

```text
build/fpm-lens-review/
├── fpm-lens-render-manifest.json
└── pools/<source-directory-sha256>/zz-fpm-lens.conf
```

The manifest maps every staged file to its intended source pool directory. It
also allows a later render into the same directory to remove obsolete generated
files safely.

On the target host, validate the source configuration plus staged override with
the matching PHP-FPM binary:

```bash
fpm-lens validate production.plan.json --php-fpm /usr/sbin/php-fpm8.3
```

For multiple PHP versions, validate separate plans with their matching binary.
Then load-test the proposed limits against service-level objectives.

## 9. Deployment handoff

FPM Lens intentionally does not install or reload services. A human or
configuration-management system should:

1. Read `fpm-lens-render-manifest.json`.
2. Back up any existing `zz-fpm-lens.conf` in each source directory.
3. Copy the corresponding staged file into that directory with root ownership
   and normal PHP-FPM configuration permissions.
4. Run the matching `php-fpm -tt` again against the installed configuration.
5. Reload—not blindly restart—the correct service through the platform's
   supported mechanism.
6. Monitor queue depth, latency, errors, worker memory, OOM activity, and
   saturation through a representative peak.

Example configuration-management logic should use the manifest mapping rather
than decoding the content-addressed directory name.

## 10. Rollback

Prepare rollback before deployment:

1. Preserve the previous override or record that none existed.
2. If service behavior regresses, restore the previous file or remove the newly
   installed override.
3. Run the matching `php-fpm -tt`.
4. Reload the affected service.
5. Confirm queues, latency, errors, and memory return to their prior state.

Never reload after a failed syntax check. Keep the plan, evidence, policy,
manifest, validation output, and monitoring timestamps together for audit.

## 11. Automation and exit codes

| Exit | Meaning |
|---:|---|
| `0` | Command completed successfully |
| `1` | Input, discovery, collection, validation, or I/O failure |
| `2` | Plan was produced but is infeasible |

Example automation:

```bash
if fpm-lens --policy production.toml --evidence evidence.json \
    plan --json --output production.plan.json >plan-summary.json; then
  fpm-lens validate production.plan.json
  fpm-lens render production.plan.json --output-dir build/fpm-lens-review
else
  code=$?
  if [ "$code" -eq 2 ]; then
    echo "Plan needs operator review; refusing deployment" >&2
  fi
  exit "$code"
fi
```

Treat evidence, policy, and plan artifacts as security-sensitive. Transfer them
over authenticated channels and redact private paths before sharing diagnostics.

## 12. Troubleshooting

### No pool directories found

Run `doktor`, locate the platform's pool configuration, and pass it explicitly:

```bash
sudo fpm-lens --pool-dir /custom/php-fpm.d doktor
```

### Status samples remain zero

- Confirm `pm.status_path` is configured in the intended pool.
- Confirm the URL returns JSON locally with `curl`.
- Check web-server routing and access controls.
- Ensure the pool mapping in `--status-url` is unique.
- Review evidence warnings for timeouts, malformed JSON, or missing fields.

### Worker memory is missing

Run observation with permission to read worker `/proc` entries. Duplicate pool
names across PHP installations cannot be attributed reliably from process
titles; qualify status URLs and use a conservative default memory value.

### Evidence remains low confidence

Check its age, duration, completion flag, endpoint success percentage, status
sample count, and memory sample count against policy. Do not lower quality
thresholds solely to obtain a smaller recommendation.

### Plan is infeasible

Read every warning. Unselected and uncertain pools still consume memory. Check
the detected cgroup limit, reserve, pool minimums, current uncertain capacity,
and worker-memory measurements.

### Staged validation fails

Use the PHP-FPM binary matching the source directory. Inspect its full `-tt`
output, existing source fragments, and the staged override. Do not install or
reload until validation succeeds.

### Getting support

Follow `SUPPORT.md`. Include the FPM Lens version, operating system, PHP
packaging source, exact command, and redacted output. Report security issues
privately as described in `SECURITY.md`.
