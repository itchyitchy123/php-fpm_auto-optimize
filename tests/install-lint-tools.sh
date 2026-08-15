#!/usr/bin/env bash
set -Eeuo pipefail

DESTINATION=${1:?usage: install-lint-tools.sh DESTINATION}
SHFMT_VERSION=3.10.0
SHFMT_SHA256=1f57a384d59542f8fac5f503da1f3ea44242f46dff969569e80b524d64b71dbc
SHELLCHECK_VERSION=0.10.0
SHELLCHECK_SHA256=6c881ab0698e4e6ea235245f22832860544f17ba386442fe7e9d629f8cbedf87

mkdir -p "$DESTINATION"
temporary=$(mktemp -d)
trap 'rm -rf -- "$temporary"' EXIT

curl --fail --location --silent --show-error \
  "https://github.com/mvdan/sh/releases/download/v${SHFMT_VERSION}/shfmt_v${SHFMT_VERSION}_linux_amd64" \
  --output "$temporary/shfmt"
printf '%s  %s\n' "$SHFMT_SHA256" "$temporary/shfmt" | sha256sum --check --status
install -m 0755 "$temporary/shfmt" "$DESTINATION/shfmt"

curl --fail --location --silent --show-error \
  "https://github.com/koalaman/shellcheck/releases/download/v${SHELLCHECK_VERSION}/shellcheck-v${SHELLCHECK_VERSION}.linux.x86_64.tar.xz" \
  --output "$temporary/shellcheck.tar.xz"
printf '%s  %s\n' "$SHELLCHECK_SHA256" "$temporary/shellcheck.tar.xz" | sha256sum --check --status
mkdir "$temporary/shellcheck"
tar --extract --xz --file "$temporary/shellcheck.tar.xz" --directory "$temporary/shellcheck" \
  --strip-components=1 --no-same-owner
install -m 0755 "$temporary/shellcheck/shellcheck" "$DESTINATION/shellcheck"

"$DESTINATION/shfmt" --version
"$DESTINATION/shellcheck" --version
