# FPM Lens

**Explainable PHP-FPM capacity planning, with a review-first terminal UI.**

[![CI](https://github.com/itchyitchy123/php-fpm_auto-optimize/actions/workflows/test.yml/badge.svg)](https://github.com/itchyitchy123/php-fpm_auto-optimize/actions/workflows/test.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-0b7285.svg)](LICENSE)

FPM Lens inventories PHP-FPM pools, collects workload evidence, and builds a
globally memory-bounded plan. It explains every decision and keeps configuration
generation separate from installation. Missing observations are treated as
uncertainty—not as proof that a pool is idle.

```text
FPM Lens plan — 1280 MB allocated / 2457 MB budget
POOL                   NOW    PLAN     MIN     MAX  EVIDENCE
checkout                 20       9       4      30  High
wordpress                12      12       4      24  Low
```

## Why it is different

- Per-pool memory costs: a 150 MB application and a 25 MB application are not
  modeled as interchangeable workers.
- Explicit uncertainty: low-confidence pools retain their current capacity.
- User constraints: each pool can have its own minimum, maximum, request cap,
  idle timeout, and request timeout.
- Global feasibility: the planner accounts for selected and unselected pools
  and refuses a plan whose minimums cannot fit.
- Reviewable artifacts: observations, policy, and plans are portable JSON/TOML;
  rendered PHP-FPM fragments are staged for inspection.
- Safe defaults: discovery and planning are read-only. `render` never edits
  `/etc` or reloads a service.

## Quick start

Rust 1.85 or newer is required.

```bash
cargo build --release
sudo target/release/fpm-lens inventory
sudo target/release/fpm-lens observe --samples 12 --interval-seconds 5
sudo target/release/fpm-lens --evidence fpm-lens.evidence.json review
sudo target/release/fpm-lens render fpm-lens.plan.json --output-dir build/review
```

Pass `--pool-dir` repeatedly for fixtures or unusual layouts. Use
`--memory-mb` for a container or deliberate planning envelope.

## Terminal workflow

| Key | Action |
|---|---|
| `↑` / `↓` | Move between pools |
| `Space` | Include or exclude a pool from adjustment |
| `Tab` | Select children, min, max, requests, idle timeout, or request timeout |
| `+` / `-` | Adjust the selected value |
| `Enter` | Save reviewed policy and plan |
| `q` | Exit without saving |

For automation, skip the UI:

```bash
fpm-lens --policy production.toml --evidence evidence.json \
  plan --json --output production.plan.json
```

## Pool policy

Names apply across installations. Use `directory:name` when the same pool name
exists in multiple PHP versions.

```toml
[global]
reserve_memory_mb = 2048
memory_utilization_percent = 80
default_worker_memory_mb = 64
minimum_evidence_samples = 12
headroom_percent = 25
default_min_children = 2
default_max_children = 100

[pools.checkout]
selected = true
target_children = 18
min_children = 6
max_children = 40
max_requests = 500
process_idle_timeout_seconds = 15
request_terminate_timeout_seconds = 120
```

## Safety boundary

FPM Lens does not claim that an idle snapshot predicts peak production load.
Observe a representative traffic window, include memory used by the database,
web server, kernel, queues, and caches in the reserve, and load-test proposed
limits before deployment.

The initial Rust release deliberately stages output instead of installing it.
An operator or configuration-management system should validate generated files
with the matching `php-fpm -tt` and deploy them using the platform's supported
mechanism. This keeps the trust boundary visible and planning testable.

## Project quality

Run `make check` for formatting, Clippy, tests, documentation, and a release
build. See [Architecture](docs/architecture.md), [Algorithm](docs/algorithm.md),
[Security](SECURITY.md), and [Contributing](CONTRIBUTING.md).

The original Bash prototype remains for historical provenance. New product
development is in `src/`; it uses a new model, workflow, planner, configuration
format, and user interface.
