# Contributing

Thank you for improving PHP-FPM Auto Optimizer. Changes to a root-level service
configuration tool must remain conservative, testable, and recoverable.

## Development workflow

1. Create a focused branch and describe the operational problem being solved.
2. Run `make check` before submitting a pull request.
3. Add regression coverage for behavior changes and failure paths.
4. Update the changelog and documentation when the CLI or policy changes.

The supported development baseline is Bash 4.4 or newer. Tests must not depend
on a live PHP-FPM installation or modify host configuration.

## Pull requests

Explain the safety impact, rollback behavior, platforms tested, and any new
root-level filesystem or service-manager operations. Keep unrelated formatting
changes separate. All CI checks and review discussions must be resolved before
merge.

By participating, you agree to follow the project code of conduct. Security
problems must be reported privately as described in `SECURITY.md`.
