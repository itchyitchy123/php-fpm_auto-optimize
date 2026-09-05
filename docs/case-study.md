# Case study: capacity planning across 42 PHP-FPM pools

This is the structure to use for a production case study. It is intentionally
data-backed rather than filled with invented latency or OOM numbers: replace
the `TBD` values with measurements from one representative before/after load
test, and keep the raw evidence and plan artifacts with the report.

## Before

| Metric | Value |
|---|---:|
| Host memory | 32 GB |
| PHP-FPM pools | 42 |
| Configured `max_children` total | 612 |
| PHP-FPM memory reserve | `TBD` MB |
| Planned FPM budget | `TBD` MB |

The original configuration used a single aggregate worker estimate. That made
the largest pools and the quietest pools look interchangeable even though
their representative worker footprints differed.

## Observed

Collect a representative traffic window with FPM Lens and record the evidence
artifact alongside the report:

```bash
sudo fpm-lens observe --samples 180 --interval-seconds 5 \
  --status-url 'checkout=http://127.0.0.1/fpm-status-checkout?json' \
  --output before.evidence.json
fpm-lens report before.evidence.json
```

Summarize the important observations here:

| Metric | Value |
|---|---:|
| Observation window | `TBD` minutes |
| Peak active workers | `TBD` |
| Representative PSS / pool | `TBD` MB |
| P95 worker PSS | `TBD` MB |
| Peak listen queue | `TBD` |
| `max children reached` events | `TBD` |
| Pools with incomplete/low-confidence evidence | `TBD` |

Missing status or memory observations must remain visible in the write-up. A
pool with no observation is uncertainty, not evidence that it is idle.

## Recommended

Save the policy and generated plan so the recommendation is reproducible:

```bash
fpm-lens --policy production.toml --evidence before.evidence.json \
  --memory-mb 32768 plan --json --output recommended.plan.json
fpm-lens validate recommended.plan.json
fpm-lens diff recommended.plan.json
fpm-lens render recommended.plan.json --output-dir build/review
```

| Metric | Value |
|---|---:|
| Recommended `max_children` total | `TBD` |
| Recommended FPM memory envelope | `TBD` MB |
| Lowest pool minimum | `TBD` |
| Highest pool worker-memory estimate | `TBD` MB |
| Plan status | `TBD` (feasible / infeasible) |

Explain at least one trade-off: which pools were reduced, which were
preserved because evidence was weak, and how the global budget constrained the
final allocation. Include the plan's warnings verbatim or link to the plan
artifact.

## Load test

Run the same workload against the baseline and staged configuration, keeping
request mix, duration, traffic shape, and success criteria constant.

| Metric | Before | After |
|---|---:|---:|
| p95 latency | `TBD` ms | `TBD` ms |
| p99 latency | `TBD` ms | `TBD` ms |
| Throughput | `TBD` req/s | `TBD` req/s |
| Peak queue depth | `TBD` | `TBD` |
| OOM events | `TBD` | `0` / `TBD` |
| Error rate | `TBD`% | `TBD`% |

The case study is complete only when the after-values are measured. The
repository's committed fixture remains a deterministic planning demonstration,
not a production throughput claim.
