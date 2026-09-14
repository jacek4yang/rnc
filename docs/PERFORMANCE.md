# Performance measurements

Measured on 2026-09-14, Windows 11 build 26200 x64, AMD Ryzen 7 5800H,
Rust 1.98.1 MSVC, Python 3.14.7. Release builds use thin LTO, one codegen
unit, symbol stripping, and a statically linked C runtime.

## End-to-end release measurement

The complete results are in [benchmark-results.json](../benchmark-results.json).
Five loopback echo runs each sent a 256 MiB binary file and received the same
256 MiB into another file. Every output matched the input SHA-256. The peer is
Python, uses a reused 64 KiB receive buffer, and echoes while receiving.

| Measurement | Result |
| --- | --- |
| Executable | 580,096 bytes (566.5 KiB) |
| Warm `--version` startup, median of 30 | 7.60 ms |
| Warm startup, p95 | 11.72 ms |
| Connected idle working set, one sample | 4,743,168 bytes (4.52 MiB) |
| Pipe-to-socket 4-byte delivery, median of 200 | 18.7 microseconds |
| Pipe-to-socket delivery, p95 | 36.1 microseconds |
| Echo payload throughput, median of five | 748.6 MiB/s |
| Echo payload throughput, observed range | 677.4–878.8 MiB/s |
| nc process CPU time per echo run | 0.234–0.313 seconds |

Throughput is **payload size / wall time**, not the sum of both directions.
A 256 MiB echo moves 512 MiB across the two socket directions. This is a warm,
cached, local end-to-end workload; filesystem cache, Python, scheduler load, and
antivirus can affect it. Startup includes Python process creation/wait and tests
the `--version` path; it is not a cold connection-start benchmark. Delivery
latency includes the pipe write and local socket receive, and is not a network
RTT. Process CPU time is kernel + user time from `GetProcessTimes`, with coarse
Windows accounting granularity. No claim is made about WAN performance,
terminal rendering speed, or superiority to other netcat implementations.

## Buffer-size experiment

The TCP buffer was temporarily built as 16, 64, and 256 KiB, with five 256 MiB
runs per configuration. The original raw records are under `benchmarks/`.
These runs preceded the final event-driven idle coordinator and timestamp
updates. No other transport parameter was changed between buffer variants.

| Buffer per direction | Median payload MiB/s | Median nc CPU seconds |
| --- | --- | --- |
| 16 KiB | 430.6 | 0.547 |
| 64 KiB | 395.9 | 0.297 |
| 256 KiB | 679.7 | 0.281 |

There is substantial run-to-run variation and this is not a controlled lab
comparison. The 256 KiB buffer delivered the best median with similar CPU cost
to 64 KiB. The final implementation uses **256 KiB per TCP direction**, a bounded
512 KiB of transport buffer capacity. UDP retains a 16 KiB send buffer and a
64 KiB receive buffer; a TCP-sized buffer must not become a UDP datagram size.

## Encoding hot paths

`cargo bench --bench paths` repeatedly feeds about 128 MiB to the production
incremental UTF-16 decoder using approximately 64 KiB chunks. The output slice
is passed through `black_box`; no console rendering is included. One run:

| Path | Input MiB/s |
| --- | --- |
| Auto, ASCII remains undecided | 6,811 |
| UTF-8 Chinese | 683 |
| GBK Chinese | 281 |
| GB18030 Chinese + supplementary characters | 596 |
| Auto selecting GB18030 on the first chunk | 589 |

These microbenchmarks are short and CPU-load-sensitive. They isolate the codec
rather than estimate visible terminal throughput. Auto after selection follows
the same decoder path as explicit GB18030; ASCII-only traffic stays undecided
and uses a vectorizable widening loop. Raw traffic never enters these paths.

## Hot-path review

Raw TCP uses one reusable buffer per worker, synchronous reads/writes, and no
payload decoding or per-chunk heap allocation. Partial writes advance a slice.
`TCP_NODELAY` avoids delayed tiny writes in interactive use. Activity timestamps
are updated only when `-w` is enabled, and monotonic atomic updates prevent two
workers from moving the shared last-activity time backwards. Without a pending
timer, the coordinator blocks on an event instead of waking periodically.

Console-only allocations are bounded buffers plus line encoding scratch space;
they do not affect raw file transfers. Detection allocates only while deciding
the initial non-ASCII prefix, then permanently reuses the streaming decoder.
No async runtime, message queue of payload buffers, or per-byte socket loop is
used. CPU time, throughput comparisons and decoder microbenchmarks were used
to inspect the obvious paths; a sampled system-wide CPU profile was not taken.

## Reproduce

```powershell
cargo build --release --locked
cargo bench --bench paths
python scripts/benchmark.py --mib 256 --rounds 5 --output benchmark-results.json
llvm-readobj --coff-imports target/release/nc.exe
```

The last command requires LLVM only as a development inspection tool. The
inspected PE imports were Kernel32, Winsock, Ntdll and Windows synchronization
API sets, with no dynamic VC runtime DLL. Python is also only a development
dependency. Keep competing workloads consistent when comparing measurements.
