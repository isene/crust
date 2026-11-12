#!/usr/bin/env python3
"""A crust program on a terminal that goes away must end, not spin.

    python3 tests/terminal.py <program> [key that quits it]

Runs the program on a pty and checks three things:
- it uses no CPU while it waits for a key;
- it draws again when the terminal changes size;
- it ends when the terminal is gone.

The program ignores SIGHUP here, as it does under strace or nohup, so
only the dead terminal itself can tell it to stop. Before crust 0.1.74
such a program kept a full core busy until someone killed it.
"""
import fcntl
import os
import pty
import select
import shutil
import signal
import struct
import sys
import tempfile
import termios
import time

fail = 0


def check(what: str, ok: bool, got: object = "") -> None:
    global fail
    print(("ok   " if ok else "FAIL ") + what + ("" if ok else f": {got}"))
    if not ok:
        fail = 1


def ticks(pid: int) -> int | None:
    """CPU time the program has used, in hundredths of a second."""
    try:
        f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    except FileNotFoundError:
        return None
    return None if f[0] == "Z" else int(f[11]) + int(f[12])


def drain(fd: int, wait: float) -> bytes:
    out = b""
    while select.select([fd], [], [], wait)[0]:
        try:
            out += os.read(fd, 65536)
        except OSError:
            break
    return out


home = tempfile.mkdtemp(prefix="crust-test-")
pid, master = pty.fork()
if pid == 0:
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    os.environ["HOME"] = home
    os.environ["TERM"] = "xterm-256color"
    os.execv(sys.argv[1], [sys.argv[1]])
fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
drain(master, 0.8)

a = ticks(pid)
time.sleep(1.0)
b = ticks(pid)
check("no CPU while it waits for a key", a is not None and b is not None and b - a <= 1, (a, b))

fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
check("it draws again after a resize", len(drain(master, 0.5)) > 0)

if len(sys.argv) > 2:
    # A key that ends the program, to show that keys still arrive.
    os.write(master, sys.argv[2].encode())
    time.sleep(0.5)
    check("the key " + sys.argv[2] + " ends it", ticks(pid) is None)
os.close(master)
time.sleep(0.5)
alive = ticks(pid) is not None
check("it ends when the terminal is gone", not alive)
if alive:
    os.kill(pid, signal.SIGKILL)
try:
    os.waitpid(pid, 0)
except ChildProcessError:
    pass
shutil.rmtree(home, ignore_errors=True)
sys.exit(fail)
