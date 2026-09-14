# nc for Windows

[![CI](https://github.com/jacek4yang/rnc/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jacek4yang/rnc/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust: stable](https://img.shields.io/badge/Rust-stable-orange.svg)](rust-toolchain.toml)

[中文说明](README.zh-CN.md) · [Contributing](CONTRIBUTING.md) · [Changelog](CHANGELOG.md) · [Security](SECURITY.md)

A small Rust netcat for TCP/UDP, binary pipelines, and CTF terminals. The release
is a native `nc.exe` with a statically linked C runtime. It requires no Rust,
Python, VC redistributable, or other separately installed runtime to run.

```console
nc 127.0.0.1 8080
nc -l 8080
nc -u 127.0.0.1 8080
nc -v localhost 8080
nc -6 ::1 8080
nc -6 -l 8080
nc 127.0.0.1 8080 --encoding utf-8
nc 127.0.0.1 8080 --encoding gbk
nc 127.0.0.1 8080 --encoding gb18030
nc 127.0.0.1 8080 --encoding auto
```

## Build and install

Use the latest stable Rust MSVC toolchain and Visual Studio C++ Build Tools.
`rust-toolchain.toml` selects stable; the checked release was built with Rust
1.98.1 on Windows 11 x64. Windows 10/11 x64 is the intended platform; Windows 10
and ARM64 have not been tested on hardware. The source also supports Unix byte
streams, but native Unicode console conversion is Windows-specific.

```powershell
cargo build --release --locked
.\target\release\nc.exe --help
```

Copy `target\release\nc.exe` to a directory on your PATH, or install locally:

```powershell
cargo install --path . --locked
nc 127.0.0.1 8080
```

Successful [GitHub Actions runs](https://github.com/jacek4yang/rnc/actions/workflows/ci.yml)
also provide a Windows ZIP with license notices and SHA-256 checksums under the
run's **Artifacts** section. These expire after 14 days and are development
builds, not published releases.

Include `LICENSE` and `THIRD-PARTY-NOTICES.md` when redistributing the executable.
`scripts/verify.ps1` runs the Windows release checks with immediate failure on
errors. `scripts/package.ps1` builds a ZIP under `dist/` containing the executable,
documentation, license notices, and SHA-256 checksums.

## Bytes, terminals, and encodings

Socket transport handles only bytes. For a native Windows console, input uses
`ReadConsoleW` and output uses `WriteConsoleW`, with conversion confined to the
console boundary. Neither operation changes the console code page or modes.
Chinese therefore does not depend on `chcp`; the terminal still needs a font
with the desired glyphs.

| Stream | Default behavior |
| --- | --- |
| Windows console input | Unicode input encoded into the selected network encoding; Enter sends LF |
| Windows console output | Incrementally decode the selected network encoding into Unicode |
| Redirected stdin or stdout | Raw bytes, independently for each handle, even with `--encoding` |
| `--raw` | No application encoding conversion or newline handling on either handle |
| Unix or terminal emulators exposing pipes | Raw byte streams |

`--encoding` accepts `auto`, `utf-8`/`utf8`, `gbk`/`cp936`, and `gb18030`.
Explicit encoding is best when the peer's encoding is known. GBK is the
web-compatible CP936 mapping implemented by `encoding_rs`; GB18030 adds its
four-byte sequences. Console decoding replaces malformed/incomplete text with
U+FFFD. Unrepresentable console input fails with an error instead of silently
sending replacement bytes or numeric character references. Raw streams never
replace, decode, or normalize anything, including NUL, CR/LF, Ctrl+Z and 0xff.

Auto mode forwards ASCII immediately without selecting an encoding. It retains
incomplete non-ASCII sequences across reads, prefers valid UTF-8, and falls back
to GB18030 on invalid UTF-8 (covering ordinary GBK text). Detection stops after
selection. This is a small heuristic, **not an infallible charset recognizer**:
GBK byte sequences can also be valid UTF-8, and mixed-encoding streams are not
supported. Outgoing console input uses UTF-8 until incoming console output has
selected an encoding. If stdout is redirected, nothing inspects incoming bytes;
use an explicit encoding for console input in that case.

Normal Windows console input is line-edited. Ctrl+Z followed by Enter at the
start of a line signals EOF. Surrogate pairs and network multibyte characters
may cross read boundaries. `--raw` uses native `ReadFile`/`WriteFile`; on an actual
console Windows itself still interprets bytes according to its console settings.
A console is not a binary storage sink; use files/pipes to preserve arbitrary data.

## Pipes and redirection

In **cmd.exe**, these are byte-exact:

```bat
nc 127.0.0.1 8080 > output.bin
nc 127.0.0.1 8080 < input.bin > output.bin
type input.bin | nc 127.0.0.1 8080
```

PowerShell's `type` is an alias for `Get-Content`, which reads text by default.
Older PowerShell versions also transcode native redirection. To avoid shell-side
conversion, use cmd for binary pipelines from PowerShell:

```powershell
cmd /d /c 'type input.bin | nc 127.0.0.1 8080 > output.bin'
```

The executable cannot undo changes made by the shell before bytes reach stdin.

## Connection and exit behavior

* TCP is full duplex. Stdin EOF sends FIN (`shutdown(Write)`), then continues
  receiving. Peer FIN finishes stdout, while stdin can still send. The process
  finishes when both directions end, unless `-q`, `-w`, an I/O error, or Ctrl+C
  ends it first. `-N` explicitly requests the already-default half-close behavior.
* `-q seconds` exits after the specified delay following stdin EOF. `-q0` exits
  immediately and can discard pending replies. Without `-q`, a peer that never
  sends FIN can keep a TCP session open. For an interactive peer that has already
  closed its sending direction, use Ctrl+Z/Enter or Ctrl+C to end local input.
* `-w seconds` bounds setup (including DNS/listen wait) and subsequent network
  inactivity, in either direction. Positive fractional seconds are accepted.
  Timer checks have roughly 20 ms granularity plus OS scheduling delay. It also
  bounds stalled socket/pipe writes. A timeout exits with status 1.
* TCP listeners serve one connection and then exit. `-l port` binds IPv4 wildcard;
  `-6 -l port` binds IPv6 wildcard, IPv6-only. Supply `-l bind-address port` to bind
  a specific interface. `-4`/`-6` filter DNS results; clients try matching addresses
  in resolver order. `-n` disables DNS and requires a numeric IP address.
* UDP has no connection handshake, reliability, or EOF. Each stdin read becomes
  one datagram, up to 16 KiB; pipe/read boundaries are not application messages.
  Received datagrams (up to the protocol limit) are concatenated on stdout.
  Zero-length datagrams are valid. A UDP listener selects its first sender and
  then accepts only that peer; stdin starts after the first datagram arrives.
  Use `-q1`, `-w5`, or Ctrl+C to finish a UDP session. A UDP "peer" diagnostic is
  not proof that a service is reachable.
* Ctrl+C shuts down the active TCP socket and exits with status 130, including
  while blocked on stdin, accept, or network I/O. Pending unsent data can be lost.
  Status 0 means normal completion or `-q`; 1 means runtime failure; 2 means CLI
  misuse. Diagnostics go only to stderr. `-v` enables connection diagnostics.

This implements the common netcat subset above. It does not implement TLS,
proxies, command execution (`-e`), port scanning, port ranges, or persistent
multi-client listeners (`-k`). It is not a drop-in implementation of every
OpenBSD, traditional netcat, or Ncat option.

## Architecture and validation

* `src/transport.rs`: blocking sockets, reused 256 KiB TCP buffers, 16 KiB UDP
  sends, 64 KiB UDP receives, two I/O workers, and a coordinator for exit/timeout.
  No text inspection, per-byte transformations, or async runtime in this path.
* `src/encoding.rs`: incremental decoder and shared automatic selection.
* `src/terminal.rs`: console detection and Unicode/byte adapters. All application
  unsafe code is isolated here, with safety comments for native API calls.
* `src/cli.rs`: small CLI parser. `src/main.rs`: signal handler and exit codes.

The coordinator holds a socket clone for orderly shutdown. After each output
write has completed, normal EOF is reported. On exit, the OS cancels remaining
blocked reads and releases handles. Workers blocked on a console or inherited
pipe are deliberately not joined; portable cancellation would otherwise require
an additional I/O framework. No console modes are modified, and no buffered
application output awaits a destructor. The transport runner is an internal
CLI component, not an embeddable long-lived server API.

Run the release checks (Python 3.10+ is needed **only for Windows test scripts**):

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
python scripts/windows_console_test.py --exe target/release/nc.exe
python scripts/windows_io_test.py --exe target/release/nc.exe
cargo bench --bench paths
python scripts/benchmark.py --output benchmark-results.json
```

Tests include real TCP/UDP IPv4 and IPv6 sockets, DNS, both half-close directions,
concurrent multi-megabyte streams, all 256 byte values, explicit encodings on raw
pipes, timeouts, and decoder splits/truncation. The Windows suite starts private
hidden native consoles, sets code page 437, injects Unicode keyboard events,
reads Unicode screen cells, and sends real Ctrl+C events. It also runs actual
cmd.exe TYPE pipelines and file redirection. No user console is altered.

See [performance measurements](docs/PERFORMANCE.md) and the recorded JSON files.
The [validation record](docs/VALIDATION.md) includes test coverage and known limits.
Results are local measurements, not guarantees or comparisons against Linux nc.
The executable's PE imports were inspected to verify that only Windows system
DLLs are required.

Native console API references: [ReadConsoleW](https://learn.microsoft.com/en-us/windows/console/readconsole),
[WriteConsoleW](https://learn.microsoft.com/en-us/windows/console/writeconsole).
Decoder reference: [encoding_rs](https://docs.rs/encoding_rs/latest/encoding_rs/struct.Decoder.html).
