SHELL := /usr/bin/env bash
VERSION := $(shell ./phpfpm-auto-optimize --version | awk '{print $$2}')
DIST := dist

.PHONY: all check syntax lint format-check test dist clean install

all: check

syntax:
	bash -n phpfpm-auto-optimize tests/run.sh completions/phpfpm-auto-optimize

lint:
	command -v shellcheck >/dev/null || { echo "shellcheck is required" >&2; exit 1; }
	shellcheck phpfpm-auto-optimize tests/run.sh completions/phpfpm-auto-optimize

format-check:
	command -v shfmt >/dev/null || { echo "shfmt is required" >&2; exit 1; }
	shfmt -d -i 2 -ci phpfpm-auto-optimize tests/run.sh completions/phpfpm-auto-optimize

test:
	./tests/run.sh

check: syntax lint format-check test
	git diff --check

dist: check
	mkdir -p $(DIST)
	tar -czf $(DIST)/phpfpm-auto-optimize-$(VERSION).tar.gz \
		--transform 's,^,phpfpm-auto-optimize-$(VERSION)/,' \
		phpfpm-auto-optimize Makefile README.md LICENSE CHANGELOG.md SECURITY.md \
		CONTRIBUTING.md CODE_OF_CONDUCT.md SUPPORT.md ROADMAP.md tests docs packaging completions
	cd $(DIST) && sha256sum phpfpm-auto-optimize-$(VERSION).tar.gz > SHA256SUMS

install:
	install -Dm0755 phpfpm-auto-optimize $(DESTDIR)/usr/sbin/phpfpm-auto-optimize
	if [[ ! -e $(DESTDIR)/etc/phpfpm-auto-optimize.conf ]]; then install -Dm0644 packaging/phpfpm-auto-optimize.conf $(DESTDIR)/etc/phpfpm-auto-optimize.conf; else echo "preserving existing $(DESTDIR)/etc/phpfpm-auto-optimize.conf"; fi
	install -Dm0644 docs/phpfpm-auto-optimize.8 $(DESTDIR)/usr/share/man/man8/phpfpm-auto-optimize.8
	install -Dm0644 completions/phpfpm-auto-optimize $(DESTDIR)/usr/share/bash-completion/completions/phpfpm-auto-optimize
	install -Dm0644 packaging/systemd/phpfpm-auto-optimize-report.service $(DESTDIR)/usr/lib/systemd/system/phpfpm-auto-optimize-report.service
	install -Dm0644 packaging/systemd/phpfpm-auto-optimize-report.timer $(DESTDIR)/usr/lib/systemd/system/phpfpm-auto-optimize-report.timer

clean:
	rm -f -- $(DIST)/phpfpm-auto-optimize-*.tar.gz $(DIST)/SHA256SUMS
