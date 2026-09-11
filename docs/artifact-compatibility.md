# Artifact compatibility

FPM Lens artifacts are intentionally portable inputs to review and automation.
Treat them as an API.

## Version 1

`Plan` JSON has a required `schema_version` of `1`. The program rejects a plan
whose version it does not understand. The authoritative machine-readable
contracts are `schemas/plan.schema.json` and `schemas/evidence.schema.json`.

Evidence v1 is a JSON object whose keys are pool IDs and whose values conform
to the evidence schema definition. It predates the plan envelope and therefore
has no top-level version field; its v1 schema is frozen by this release line.

## Change policy

- Additive, optional fields may be introduced in a minor release only when old
  readers can safely ignore them.
- A semantic or structural break receives a new schema version and a migration
  command or documented conversion procedure.
- The current and previous two minor FPM Lens releases should be able to read
  supported artifacts for their schema version.
- Fixtures under `tests/fixtures/` are compatibility fixtures and must be kept
  valid by tests.
