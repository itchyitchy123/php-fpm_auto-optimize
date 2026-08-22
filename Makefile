SHELL := /usr/bin/env bash
VERSION := $(shell sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
TARGET ?= target
DIST := dist

.PHONY: all check format lint test build dist install clean legacy-test

all: check
format:
	cargo fmt --all
lint:
	cargo clippy --all-targets --all-features -- -D warnings
test:
	cargo test --all-features
build:
	cargo build --release --locked
check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all-features
	cargo doc --no-deps --all-features
	cargo build --release --locked
	git diff --check
legacy-test:
	./tests/run.sh
dist: check
	mkdir -p $(DIST)
	cp $(TARGET)/release/fpm-lens $(DIST)/fpm-lens-$(VERSION)-linux-$$(uname -m)
	cd $(DIST) && sha256sum fpm-lens-$(VERSION)-linux-* > SHA256SUMS
install: build
	install -Dm0755 $(TARGET)/release/fpm-lens $(DESTDIR)/usr/bin/fpm-lens
	install -Dm0644 fpm-lens.example.toml $(DESTDIR)/usr/share/doc/fpm-lens/fpm-lens.example.toml
clean:
	cargo clean
	rm -f -- $(DIST)/fpm-lens-* $(DIST)/SHA256SUMS
