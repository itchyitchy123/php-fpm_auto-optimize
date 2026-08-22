# Case study: why one worker estimate is misleading

Consider a 4 GB host with 512 MB reserved for the operating system and other
services. Policy permits PHP-FPM to use 80% of the remainder: 2,867 MB.

| Pool | Current | Observed peak | Worker RSS | Confidence | Bounds |
|---|---:|---:|---:|---|---|
| `checkout` | 20 | 10 | 72 MB | high | 4–30 |
| `blog` | 12 | unavailable | 64 MB fallback | low | 2–50 |

FPM Lens proposes 13 checkout workers: the observed peak of 10 plus 25%
headroom, rounded up. It preserves all 12 blog workers because missing evidence
does not prove low demand.

```text
FPM Lens plan — 1704 MB allocated / 2867 MB budget
POOL                   NOW    PLAN     MIN     MAX  EVIDENCE
blog                     12      12       2      50  Low
checkout                 20      13       4      30  High
```

The allocation costs 936 MB for checkout and 768 MB for blog. Treating every
worker as the same size would hide this relationship and could remove capacity
from the wrong application.

The example is reproducible from the committed fixtures:

```bash
cargo run -- \
  --pool-dir tests/fixtures/pool.d \
  --policy tests/fixtures/policy.toml \
  --evidence tests/fixtures/evidence.json \
  --memory-mb 4096 plan
```

This is a planning demonstration, not a throughput claim. Production limits
still require a representative observation window and load testing against
service-level objectives.
