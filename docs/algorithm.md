# Recommendation algorithm

1. Discover configuration trees and parse pool assignments in lexical load
   order, excluding the generated late-loading override from the baseline.
2. Detect usable memory from `/proc/meminfo`, capped by the current process's
   cgroup v1/v2 limit resolved through `/proc/self/cgroup` and mount information.
3. Estimate worker memory from the configured percentile of live worker PSS
   (`smaps_rollup`) where permitted, with RSS fallback, a safety floor, and a
   configured fallback when the sample is too small. Monitoring mode repeats
   this sampling and retains concurrency and memory peaks.
4. Count recent, unambiguous `pm.max_children` warnings. Bound logs identify an
   exact configuration tree; ambiguous unbound warnings are ignored.
5. Produce conservative per-pool candidates anchored to their baseline.
6. Scale capacity above the per-pool minimum until the aggregate fits the hard
   memory policy, including explicit overcommit if configured.
7. Keep dynamic start/spare directives no higher than the proposed maximum.

RSS includes shared pages and an idle sample does not establish peak usage.
Exercise representative traffic and leave sufficient reserve for the database,
web server, kernel, and other services.
