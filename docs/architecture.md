# Architecture

FPM Lens is a library-first Rust application. The CLI and terminal UI consume
the same typed domain model.

```text
PHP-FPM files ──> inventory ──┐
/proc samples ──> evidence ───┼──> planner ──> immutable JSON plan
TOML policy ────> constraints ┘                    │
                                                   └──> staged renderer
```

- `inventory` parses fragments in lexical load order.
- `observe` creates reusable evidence without changing the host.
- `planner` is deterministic apart from the plan timestamp.
- `tui` edits policy; it contains no tuning rules.
- `render` accepts only self-consistent feasible plans, uses content-addressed
  paths, writes atomically below an explicit staging directory, and records a
  cleanup/deployment manifest.

Inventory and observation read system state. Planning is domain logic.
Installation, service management, and privileged panel APIs remain outside the
initial trust boundary.

## Privilege boundary

```text
                    privileged, read-only
 /proc ────────────────┐
 pool configuration ───┼──> evidence.json
 local FPM status ─────┘          │
                                      unprivileged
 policy.toml ───────────────────> plan.json ──> staged configuration
                                                   │
                                            human / configuration management
```

Collection may need read access unavailable to an ordinary user. Evidence,
planning, review, and rendering do not require write access to PHP-FPM or
`/etc`; deployment remains an operator or configuration-management action.
