# Planning model

FPM Lens solves a heterogeneous, bounded allocation problem. It does not infer
that a pool is quiet merely because no workers appeared in a short sample.

## Inputs and budget

Each pool has current settings, observed peak concurrency, representative
worker memory, saturation events, sample count, selection state, and individual
child bounds.

```text
FPM budget = (host memory - fixed reserve) × utilization percentage
pool cost  = proposed children × representative pool worker memory
```

A fixed reserve is easier to audit than two overlapping percentages. Size it
for non-FPM services and expected variance.

## Candidate capacity

Evidence confidence is high with four times the minimum samples plus concurrency
and memory observations, medium with enough samples and one useful observation,
and low otherwise. Medium/high evidence produces an observed peak plus explicit
headroom. Saturation without enough samples permits a small increase. Without
either, current capacity is preserved. Results are clamped to per-pool bounds.

Timeout and request recycling are policy decisions rather than values derivable
from RAM, so they change only when the user sets them.

## Global constraint

Unselected pools retain their allocation and still consume budget. If fixed and
minimum allocations exceed the budget, the plan is infeasible and cannot be
rendered. Otherwise capacity is removed from the least-supported candidate
above its minimum, considering confidence, saturation, observed concurrency,
and that pool's own worker memory cost.

Dynamic start/spare counts are capped by final `pm.max_children`; undocumented
defaults are never invented.

## Known limits

- RSS may over-count shared pages; future evidence formats can add PSS.
- Identically named pools across PHP installations cannot be reliably
  attributed from process titles and are skipped.
- Concurrency is not latency, throughput, or queue-depth evidence. Production
  decisions still need load testing and service objectives.
