# Contributing

Thank you for improving FPM Lens. Capacity-planning changes can affect
production availability, so the project favors explainable behavior and tests
over clever heuristics.

## Development

Rust 1.85 is the minimum supported compiler.

```bash
make check
```

Pull requests should include a regression test, describe the evidence behind a
tuning rule, and identify any change to the safety boundary. Fixtures must not
depend on a live PHP-FPM service or modify host configuration.

Planner invariants include:

- per-pool bounds are always respected;
- feasible plans never exceed the memory budget;
- insufficient evidence never implies that a pool is idle;
- unselected pools retain their current settings;
- infeasible minimum allocations are reported, not hidden;
- renderer output remains inside its explicit staging directory.

Use `cargo fmt`; Clippy warnings are denied in CI. Keep UI, parsing, policy, and
planning changes in their respective modules. Security issues must be reported
privately as described in `SECURITY.md`.

## Release checklist

Update the version in `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md`, run
`make check`, and push a matching `v<version>` tag. The release workflow builds
the supported Linux binaries, publishes SHA256 files and provenance
attestations, and smoke-tests each downloaded asset before the release job
finishes.

By participating, you agree to follow the code of conduct.
