# Security policy

## Supported versions

Security fixes target the current main branch and the latest published release.
Version numbers in Cargo.toml do not by themselves mean that a release has been
published. Older versions are not maintained as separate security branches.

## Private reports

Please use [GitHub private vulnerability reporting](https://github.com/jacek4yang/rnc/security/advisories/new).
Do not open a public Issue containing exploit details or credentials. Include
an affected version/commit, reproduction steps, expected impact, and platform.
Maintainers will assess the report and coordinate a fix and disclosure; no
response-time SLA is promised.

## Intended behavior

This tool sends and receives raw network bytes. It does not provide TLS,
authentication, or sandboxing. Listener addresses determine which interfaces
accept traffic. Received terminal text can contain control sequences interpreted
by the terminal. These documented properties alone are not vulnerabilities.
Unexpected code execution, memory-safety defects, byte corruption, or bypasses
of the documented behavior should be reported with a minimal reproduction.
