"""Real Win32 console tests. Python is a test dependency only; nc is standalone."""
import argparse
import ctypes as C
from ctypes import wintypes as W
import os
import socket
import subprocess
import threading
import time

K = C.WinDLL("kernel32", use_last_error=True)

def api(name, restype, *argtypes):
    fn = getattr(K, name)
    fn.restype, fn.argtypes = restype, argtypes
    return fn

class COORD(C.Structure):
    _fields_ = [("X", W.SHORT), ("Y", W.SHORT)]
class RECT(C.Structure):
    _fields_ = [("Left", W.SHORT), ("Top", W.SHORT), ("Right", W.SHORT), ("Bottom", W.SHORT)]
class CHAR(C.Union):
    _fields_ = [("UnicodeChar", W.WCHAR), ("AsciiChar", C.c_char)]
class CHAR_INFO(C.Structure):
    _fields_ = [("Char", CHAR), ("Attributes", W.WORD)]
class KEY(C.Structure):
    _fields_ = [("down", W.BOOL), ("repeat", W.WORD), ("vk", W.WORD), ("scan", W.WORD), ("char", CHAR), ("control", W.DWORD)]
class EVENT(C.Union):
    _fields_ = [("key", KEY), ("padding", C.c_byte * 16)]
class INPUT(C.Structure):
    _fields_ = [("type", W.WORD), ("event", EVENT)]

free = api("FreeConsole", W.BOOL)
attach = api("AttachConsole", W.BOOL, W.DWORD)
close = api("CloseHandle", W.BOOL, W.HANDLE)
create = api("CreateFileW", W.HANDLE, W.LPCWSTR, W.DWORD, W.DWORD, W.LPVOID, W.DWORD, W.DWORD, W.HANDLE)
read_screen = api("ReadConsoleOutputW", W.BOOL, W.HANDLE, C.POINTER(CHAR_INFO), COORD, COORD, C.POINTER(RECT))
write_input = api("WriteConsoleInputW", W.BOOL, W.HANDLE, C.POINTER(INPUT), W.DWORD, C.POINTER(W.DWORD))
set_cp = api("SetConsoleCP", W.BOOL, W.UINT)
set_output_cp = api("SetConsoleOutputCP", W.BOOL, W.UINT)
get_cp = api("GetConsoleCP", W.UINT)
get_output_cp = api("GetConsoleOutputCP", W.UINT)
get_mode = api("GetConsoleMode", W.BOOL, W.HANDLE, C.POINTER(W.DWORD))
set_mode = api("SetConsoleMode", W.BOOL, W.HANDLE, W.DWORD)
ctrl = api("GenerateConsoleCtrlEvent", W.BOOL, W.DWORD, W.DWORD)
handler = api("SetConsoleCtrlHandler", W.BOOL, W.LPVOID, W.BOOL)

def check(result):
    if not result:
        raise C.WinError(C.get_last_error())
    return result

def screen(handle):
    chars = (CHAR_INFO * 800)()
    rect = RECT(0, 0, 79, 9)
    check(read_screen(handle, chars, COORD(80, 10), COORD(0, 0), C.byref(rect)))
    return "".join(ch.Char.UnicodeChar for ch in chars if not ch.Attributes & 0x200)

def wait_text(handle, expected, process):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        text = screen(handle)
        if expected in text:
            return
        assert process.poll() is None, (process.returncode, ascii(text))
        time.sleep(0.01)
    raise AssertionError((ascii(expected), ascii(screen(handle))))

def keys(handle, text):
    units = text.encode("utf-16-le")
    records = (INPUT * (len(units) // 2))()
    for i, record in enumerate(records):
        code = int.from_bytes(units[2*i:2*i+2], "little")
        record.type = 1
        record.event.key = KEY(True, 1, 13 if code == 13 else 0, 0, CHAR(chr(code)), 0)
    written = W.DWORD()
    check(write_input(handle, records, len(records), C.byref(written)))
    assert written.value == len(records)

def launch(exe, args, pipes=False):
    # A private hidden console prevents tests from touching the user's terminal.
    startup = subprocess.STARTUPINFO()
    startup.dwFlags = subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    # The ignored-Ctrl+C flag is inherited, unlike registered handler functions.
    handler(None, False)
    redirects = dict(stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE) if pipes else {}
    process = subprocess.Popen([exe, *args], creationflags=subprocess.CREATE_NEW_CONSOLE, startupinfo=startup, **redirects)
    free()
    deadline = time.monotonic() + 5
    while not attach(process.pid):
        if process.poll() is not None or time.monotonic() > deadline:
            process.kill()
            raise AssertionError("could not attach private test console")
        time.sleep(0.01)
    check(handler(None, True))  # Only this test driver ignores Ctrl+C.
    handles = [create(name, 0xC0000000, 3, None, 3, 0, None) for name in ["CONIN$", "CONOUT$"]]
    assert all(h != W.HANDLE(-1).value for h in handles)
    check(set_cp(437)); check(set_output_cp(437))
    return process, handles

def cleanup(process, handles):
    for h in handles:
        close(h)
    free()
    if process.poll() is None:
        process.kill()
    process.wait(timeout=5)
    for stream in [process.stdin, process.stdout, process.stderr]:
        if stream:
            stream.close()

def console_roundtrip(exe, mode):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0)); listener.listen(); listener.settimeout(5)
        process, handles = launch(exe, ["--encoding", mode, "-w10", "127.0.0.1", str(listener.getsockname()[1])])
        try:
            with listener.accept()[0] as peer:
                peer.settimeout(5)
                wire = "gb18030" if mode == "auto" else mode
                peer.sendall(b"banner> ")
                wait_text(handles[1], "banner>", process)
                # ASCII must not lock auto to UTF-8; send legacy Chinese afterwards.
                for byte in "你好世界".encode(wire):
                    peer.sendall(bytes([byte])); time.sleep(0.005)
                wait_text(handles[1], "你好世界", process)
                text = "输入中文" + ("🙂𠀀" if wire != "gbk" else "")
                keys(handles[0], text + "\r")
                received = b""
                while not received.endswith(b"\n"):
                    received += peer.recv(1024)
                assert received == (text + "\n").encode(wire), (mode, received.hex())
                if mode == "utf-8":
                    saved = W.DWORD()
                    check(get_mode(handles[0], C.byref(saved)))
                    check(set_mode(handles[0], saved.value & ~4))  # Avoid scrolling the test screen.
                    # ReadConsoleW's 4096-unit buffer splits this line's CRLF.
                    long_text = "x" * 4095
                    keys(handles[0], long_text + "\r")
                    received = b""
                    while not received.endswith(b"\n"):
                        received += peer.recv(8192)
                    assert received == (long_text + "\n").encode()
                    check(set_mode(handles[0], saved.value))
                assert get_cp() == 437 and get_output_cp() == 437
                keys(handles[0], "\x1a\r")
                assert peer.recv(1) == b"", "Ctrl+Z must half-close TCP"
                peer.sendall(b" AFTER-EOF")
                wait_text(handles[1], "AFTER-EOF", process)
                peer.shutdown(socket.SHUT_WR)
                assert process.wait(timeout=5) == 0
            print("PASS native console", mode, "CP437 / Unicode input / split output / Ctrl+Z")
        finally:
            cleanup(process, handles)

def interruption(exe, listening):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
        if listening:
            listener.close()
            args = ["-l", "127.0.0.1", str(port)]
        else:
            listener.listen(); listener.settimeout(5)
            args = ["127.0.0.1", str(port)]
        process, handles = launch(exe, args)
        peer = None
        try:
            if not listening:
                peer = listener.accept()[0]
                peer.sendall(b"READY")
                wait_text(handles[1], "READY", process)
            else:
                time.sleep(0.2)
            check(ctrl(0, 0))
            assert process.wait(timeout=5) == 130, process.returncode
            if peer:
                peer.settimeout(5)
                assert peer.recv(1) == b""
            print("PASS Ctrl+C", "blocked accept" if listening else "blocked native stdin/socket")
        finally:
            if peer:
                peer.close()
            cleanup(process, handles)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default="target/release/nc.exe")
    args = parser.parse_args()
    exe = os.path.abspath(args.exe)
    for mode in ["utf-8", "gbk", "gb18030", "auto"]:
        console_roundtrip(exe, mode)
    for listening in [False, True]:
        interruption(exe, listening)
    interruption_pipes(exe)
    raw_console(exe)

def raw_console(exe):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0)); listener.listen(); listener.settimeout(5)
        process, handles = launch(exe, ["--raw", "--encoding", "gbk", "127.0.0.1", str(listener.getsockname()[1])])
        try:
            with listener.accept()[0] as peer:
                peer.settimeout(5)
                peer.sendall(b"\xe9")  # Invalid standalone UTF-8; WriteFile uses CP437.
                wait_text(handles[1], b"\xe9".decode("cp437"), process)
                keys(handles[0], "raw\r")
                received = b""
                while not received.endswith(b"\n"):
                    received += peer.recv(1024)
                assert received == b"raw\r\n", received
                check(ctrl(0, 0))
                assert process.wait(timeout=5) == 130
            print("PASS raw native console bypass / unchanged CRLF")
        finally:
            cleanup(process, handles)

def interruption_pipes(exe):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0)); listener.listen(); listener.settimeout(5)
        process, handles = launch(exe, ["127.0.0.1", str(listener.getsockname()[1])], pipes=True)
        try:
            with listener.accept()[0] as peer:
                peer.settimeout(5)
                peer.sendall(b"READY")
                assert process.stdout.read(5) == b"READY"
                def flood():
                    try:
                        peer.sendall(b"x" * (16 * 1024 * 1024))
                    except OSError:
                        pass  # Intentional interruption while bytes remain pending.
                writer = threading.Thread(target=flood, daemon=True)
                writer.start(); time.sleep(0.2)
                check(ctrl(0, 0))
                assert process.wait(timeout=5) == 130, process.returncode
                writer.join(timeout=5)
                assert not writer.is_alive()
            print("PASS Ctrl+C blocked stdin pipe and stdout pipe")
        finally:
            cleanup(process, handles)

if __name__ == "__main__":
    main()
