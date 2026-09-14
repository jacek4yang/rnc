"""Reproducible loopback/file benchmark; reports wall time, validates SHA-256.
Python and file-system cache contribute to these end-to-end numbers.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
import os
import platform
import socket
import statistics
import subprocess
import tempfile
import threading
import time
from pathlib import Path

class MEMORY(C.Structure):
    _fields_ = [("cb", W.DWORD), ("faults", W.DWORD), ("peak_working", C.c_size_t), ("working", C.c_size_t), ("peak_paged", C.c_size_t), ("paged", C.c_size_t), ("peak_nonpaged", C.c_size_t), ("nonpaged", C.c_size_t), ("pagefile", C.c_size_t), ("peak_pagefile", C.c_size_t)]

def memory(process):
    if os.name != "nt":
        return None
    counters = MEMORY(); counters.cb = C.sizeof(counters)
    fn = C.WinDLL("psapi").GetProcessMemoryInfo
    fn.argtypes = [W.HANDLE, C.POINTER(MEMORY), W.DWORD]
    if not fn(int(process._handle), C.byref(counters), counters.cb):
        raise C.WinError()
    return counters.working

def digest(path):
    with open(path, "rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()

def cpu_seconds(process):
    if os.name != "nt":
        return None
    times = [W.FILETIME() for _ in range(4)]
    fn = C.WinDLL("kernel32").GetProcessTimes
    fn.argtypes = [W.HANDLE, *([C.POINTER(W.FILETIME)] * 4)]
    if not fn(int(process._handle), *(C.byref(t) for t in times)):
        raise C.WinError()
    return sum((t.dwHighDateTime << 32) + t.dwLowDateTime for t in times[2:]) / 10_000_000

def bench(exe, size_mib, rounds):
    starts = []
    for _ in range(30):
        start = time.perf_counter()
        subprocess.run([exe, "--version"], stdout=subprocess.DEVNULL, check=True)
        starts.append((time.perf_counter() - start) * 1000)
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0)); listener.listen(); listener.settimeout(10)
        with subprocess.Popen([exe, "127.0.0.1", str(listener.getsockname()[1])], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE) as process:
            with listener.accept()[0] as peer:
                peer.settimeout(10)
                idle_memory = memory(process)
                samples = []
                for _ in range(200):
                    start = time.perf_counter()
                    process.stdin.write(b"ping"); process.stdin.flush()
                    received = bytearray()
                    while len(received) < 4:
                        received.extend(peer.recv(4 - len(received)))
                    assert received == b"ping"
                    samples.append((time.perf_counter() - start) * 1e6)
                process.stdin.close(); process.stdin = None
                assert peer.recv(1) == b""
                peer.shutdown(socket.SHUT_WR)
                assert process.wait(timeout=10) == 0
    results = []
    with tempfile.TemporaryDirectory(prefix="rnc-bench-") as directory:
        source = Path(directory) / "input.bin"
        dest = Path(directory) / "output.bin"
        chunk = bytes(range(256)) * 4096
        with source.open("wb") as file:
            for _ in range(size_mib):
                file.write(chunk)
        expected = digest(source)
        for _ in range(rounds):
            errors = []
            with socket.socket() as listener:
                listener.bind(("127.0.0.1", 0)); listener.listen(); listener.settimeout(30)
                def echo():
                    try:
                        with listener.accept()[0] as peer:
                            peer.settimeout(30)
                            buffer = bytearray(65536)
                            view = memoryview(buffer)
                            while (n := peer.recv_into(buffer)):
                                peer.sendall(view[:n])
                            peer.shutdown(socket.SHUT_WR)
                    except BaseException as error:
                        errors.append(repr(error))
                worker = threading.Thread(target=echo, daemon=True); worker.start()
                with source.open("rb") as src, dest.open("wb") as dst:
                    start = time.perf_counter()
                    with subprocess.Popen([exe, "--raw", "-w30", "127.0.0.1", str(listener.getsockname()[1])], stdin=src, stdout=dst, stderr=subprocess.PIPE) as process:
                        try:
                            _, error = process.communicate(timeout=60)
                        except BaseException:
                            process.kill(); process.wait()
                            raise
                        elapsed = time.perf_counter() - start
                        cpu = cpu_seconds(process)
                        assert process.returncode == 0, error
                worker.join(timeout=30)
                assert not worker.is_alive() and not errors, errors
                assert digest(dest) == expected
                results.append({"seconds": round(elapsed, 4), "nc_cpu_seconds": cpu, "payload_mib_per_second": round(size_mib / elapsed, 1)})
    return {
        "platform": platform.platform(), "python": platform.python_version(),
        "rust": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "binary_bytes": Path(exe).stat().st_size,
        "startup_version_median_ms": round(statistics.median(starts), 3),
        "startup_version_p95_ms": round(sorted(starts)[28], 3),
        "idle_working_set_bytes": idle_memory,
        "pipe_to_socket_4byte_median_us": round(statistics.median(samples), 1),
        "pipe_to_socket_4byte_p95_us": round(sorted(samples)[189], 1),
        "echo_payload_mib": size_mib, "echo_rounds": results,
        "echo_sha256": expected,
        "notes": "Warm local runs; echo transfers payload in each direction simultaneously; reported rate is payload/elapsed, not sum of both directions. Includes Python peer and filesystem I/O/cache. Latency is pipe-to-socket, not RTT. Memory is one connected idle sample."
    }

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default="target/release/nc.exe" if os.name == "nt" else "target/release/nc")
    parser.add_argument("--mib", type=int, default=256)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--output")
    args = parser.parse_args()
    report = json.dumps(bench(os.path.abspath(args.exe), args.mib, args.rounds), indent=2)
    print(report)
    if args.output:
        Path(args.output).write_text(report + "\n", encoding="utf-8")
