# Changelog

## Unreleased

### Added

- Native Rust `nc` executable for Windows with a statically linked C runtime.
- TCP and UDP clients/listeners, IPv4/IPv6, DNS, full-duplex streams, TCP
  half-close, clean Ctrl+C exit, idle timeout and delayed exit after stdin EOF.
- Byte-exact redirected streams and `--raw`; native Windows Unicode console
  input/output with UTF-8, GBK/CP936, GB18030 and lightweight automatic selection.
- Windows and Linux CI, real Windows console/shell tests, reproducible benchmarks,
  standalone ZIP packaging and checksums.
- Protected main branch, PR workflow, contribution/security guidance,
  Issue/PR templates and Dependabot updates.

### Known limits

- Automatic encoding detection is heuristic. Use explicit encoding when known.
- Windows 10 and ARM64 hardware have not been validated.
- See `docs/VALIDATION.md` for the recorded coverage and transient development-test
  failures, and `docs/PERFORMANCE.md` for measurement methods and limits.
