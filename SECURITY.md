# Security Policy

## Reporting a Vulnerability

The RustSBI team welcomes security reports and is committed to providing prompt attention to security issues.

**Please do not report security vulnerabilities through public GitHub issues, pull requests, or any other public channel.**
Public disclosure before a fix is available may put all RustSBI users at risk.

Instead, please report security issues privately by sending an email to the security team at
[security@rustsbi.com](mailto:security@rustsbi.com).

A member of the security team will acknowledge your report within a few working days and keep you informed of
the progress towards a fix and full announcement.

### What to include in your report

To help us triage and resolve the issue quickly, please include a detailed bug report with the
following information:

- A description of the vulnerability and its potential impact
- The affected RustSBI versions and platforms (e.g. QEMU, specific hardware)
- Step-by-step instructions to reproduce the issue
- A proof of concept or exploit code (**required**)
- Any suggested mitigations or fixes, if you have them

## Disclosure Process

Once a report is received, the security team will:

1. Confirm and validate the vulnerability, and determine the affected versions.
2. Prepare a fix privately. The fix is not committed to the public repository until the embargo lifts.
3. Coordinate a release and disclosure date with the reporter, using
   [GitHub Security Advisories](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/about-repository-security-advisories)
   where applicable. The issue remains embargoed until the fix is published.
4. Publish the fix together with a security announcement.

## Security Advisories

The RustSBI team is committed to transparency in the security disclosure process. Confirmed vulnerabilities are
announced via [GitHub Security Advisories and release notes](https://github.com/rustsbi/rustsbi/releases)
of this repository.
