Name: phpfpm-auto-optimize
Version: 0.5.0
Release: 1%{?dist}
Summary: Memory-aware PHP-FPM pool capacity optimizer
License: MIT
URL: https://github.com/itchyitchy123/php-fpm_auto-optimize
Source0: %{name}-%{version}.tar.gz
BuildArch: noarch
Requires: bash >= 4.4, coreutils, findutils, procps-ng, util-linux

%description
Conservatively recommends and transactionally applies globally bounded
PHP-FPM process-manager settings across common packaging layouts.

%prep
%autosetup

%build

%check
make syntax test

%install
install -Dm0755 phpfpm-auto-optimize %{buildroot}%{_sbindir}/phpfpm-auto-optimize
install -Dm0644 packaging/phpfpm-auto-optimize.conf %{buildroot}%{_sysconfdir}/phpfpm-auto-optimize.conf
install -Dm0644 docs/phpfpm-auto-optimize.8 %{buildroot}%{_mandir}/man8/phpfpm-auto-optimize.8

%files
%license LICENSE
%doc README.md CHANGELOG.md
%config(noreplace) %{_sysconfdir}/phpfpm-auto-optimize.conf
%{_sbindir}/phpfpm-auto-optimize
%{_mandir}/man8/phpfpm-auto-optimize.8*
