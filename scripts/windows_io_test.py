"""Exercise actual cmd.exe TYPE pipelines and file redirection with all byte values."""
import argparse
import hashlib
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading


def main(exe):
    with tempfile.TemporaryDirectory(prefix="rnc io ") as directory:
        source = Path(directory) / "input bytes.bin"
        output = Path(directory) / "output bytes.bin"
        data = bytes(range(256)) * 16384
        source.write_bytes(data)
        for kind in ["pipe", "file"]:
            errors = []
            with socket.socket() as listener:
                listener.bind(("127.0.0.1", 0)); listener.listen(); listener.settimeout(10)
                def echo():
                    try:
                        with listener.accept()[0] as peer:
                            peer.settimeout(10)
                            received = bytearray()
                            while chunk := peer.recv(65536):
                                received.extend(chunk)
                            assert received == data
                            peer.sendall(received)
                            peer.shutdown(socket.SHUT_WR)
                    except BaseException as error:
                        errors.append(repr(error))
                worker = threading.Thread(target=echo, daemon=True); worker.start()
                nc = f'"{exe}" --encoding gbk -w10 127.0.0.1 {listener.getsockname()[1]}'
                command = f'type "{source}" | {nc} > "{output}"' if kind == "pipe" else f'{nc} < "{source}" > "{output}"'
                # All paths are created locally, quoted, and passed only to cmd.
                result = subprocess.run('cmd.exe /d /s /c "' + command + '"', capture_output=True, timeout=20)
                assert result.returncode == 0, (result.returncode, result.stderr)
                worker.join(timeout=10)
                assert not worker.is_alive() and not errors, errors
                assert result.returncode == 0, result.stderr
                assert output.read_bytes() == data
                print("PASS cmd.exe", kind, "4 MiB all-byte-values", hashlib.sha256(data).hexdigest())

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default="target/release/nc.exe")
    main(os.path.abspath(parser.parse_args().exe))
