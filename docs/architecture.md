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
- `render` accepts only feasible plans and writes atomically below an explicit
  staging directory.

Inventory and observation read system state. Planning is domain logic.
Installation, service management, and privileged panel APIs remain outside the
initial trust boundary.
