# Security Policy

## Supported versions

Only the latest release of each app is supported. Older releases do not receive security fixes.

## Vulnerability scanning

This repository runs [cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit) against `Cargo.lock` in the `Security audit` GitHub Actions workflow (`.github/workflows/security.yml`):

- on every push and pull request to `main` (Markdown, `docs/` and `LICENSE` changes are skipped)
- on a weekly schedule (Mondays)
- on manual workflow dispatch

Known RustSec vulnerabilities fail the job. Unmaintained, unsound and yanked crates are reported as warnings (cargo-audit's defaults). Advisory ignores, if any, live in `.cargo/audit.toml` with a reason.

[Dependabot](https://docs.github.com/en/code-security/dependabot) is enabled for the Cargo workspace and for GitHub Actions, on a weekly schedule, so dependency and action updates can land as pull requests.

## Privacy and network use

Neither app collects personal data, tracks users, or sends telemetry.

- **Copycraft** is fully offline: it has no network code and no network entitlement.
- **Ticker** fetches market prices over HTTPS from the asset URLs in its config (plain GET requests). It does not send identifiers, watch lists or other user data in those requests. Price servers see the device IP address, as any HTTPS server would. Aside from those price fetches, ticker does not phone home.

## Reporting a vulnerability

Please report security issues by email to **mac-apps-support@proton.me**.

You should receive an acknowledgement within 7 days. Do not report vulnerabilities anywhere other than that email address.

## Scope

We take reports seriously, but we do not promise to fix every CVE or advisory that appears in the dependency tree. Severity, exploitability in these apps, available upgrades and maintenance cost all matter. Workarounds or documented ignores may be used when a fix is not practical.
