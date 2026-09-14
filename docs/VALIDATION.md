# Validation record

Platform: Windows 11 x64, build 26200. Compiler: Rust 1.98.1 MSVC.
Date: 2026-09-14.

The final `scripts/verify.ps1` run completed successfully:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`: 4 unit tests, 10 network integration tests, 2 Windows integration
  tests (the latter invoke several real console and shell scenarios)
- `cargo build --release`
- `cargo test --release`: all 16 tests passed in the optimized build
- Native console tests against `target/release/nc.exe`
- cmd.exe pipe/redirection tests against `target/release/nc.exe`

The tests exercised UTF-8, GBK and GB18030 network decoding across every chunk
size of the fixtures; malformed/truncated sequences; ASCII auto-detection;
Chinese and supplementary Unicode keyboard input at console code page 437;
a 4,095-character input line crossing the console read boundary; Ctrl+Z followed
by a server reply; raw console byte output and unchanged CRLF; Ctrl+C while
blocked on console input, accept, stdin pipes and stdout pipes.

Network coverage includes TCP/UDP clients and listeners, IPv4/IPv6, DNS,
address-validation failures, bind conflicts, both TCP half-close directions,
simultaneous duplex traffic, 16 MiB raw transport, a 60,000-byte UDP datagram,
zero-length UDP datagrams, all 256 byte values, explicit encodings on piped
handles, idle timeout and delayed exit. cmd.exe tests verified TYPE and file
redirection with 4 MiB files; benchmark outputs verified SHA-256 on 256 MiB files.

Two development runs of the parallel network suite encountered TCP read
timeouts. After adding diagnostics, the failure did not recur across 28
consecutive repetitions or the final verification run; the test deadlines were
not relaxed. No root cause was established for those transient failures. The
extra diagnostics remain in the suite to make any recurrence actionable.

The Windows tests inspect native Unicode screen cells and inject real console
keyboard events in private hidden consoles. They do not verify font rendering
visually, every terminal emulator, Windows 10 hardware, or Windows ARM64.
Linux CI is configured but was not executed locally. See PERFORMANCE.md for
measurement methods and limits; a sampled system-wide CPU profile was not taken.
