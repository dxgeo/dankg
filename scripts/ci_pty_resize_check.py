#!/usr/bin/env python3
"""Throwaway CI check: does the TUI redraw on a pty resize with no keypress?

Not part of the permanent test suite -- this drives a real pty (pty.fork)
so it can exercise the actual SIGWINCH + poll(2) path term.rs declares by
hand, on the real Linux runner rather than a developer's Mac. Deleted
once the Linux path is confirmed; see architecture.md's TUI section for
why this can't be a normal `cargo test`.
"""
import os, pty, fcntl, termios, struct, time, select, re, sys


def set_winsize(fd, rows, cols):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


def drain(fd, timeout=0.5):
    buf = b""
    while True:
        r, _, _ = select.select([fd], [], [], timeout)
        if not r:
            break
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        buf += chunk
    return buf


def main():
    pid, master_fd = pty.fork()
    if pid == 0:
        os.execv("./target/release/dankg", ["dankg", "tui", "."])
        os._exit(1)

    set_winsize(master_fd, 24, 80)
    time.sleep(0.5)
    first = drain(master_fd, 0.5)
    print(f"initial frame: {len(first)} bytes", file=sys.stderr)
    if b"\x1b[H" not in first:
        print("FAIL: no initial frame drawn at all", file=sys.stderr)
        return 1

    ok = True
    for rows, cols in [(40, 120), (15, 50)]:
        set_winsize(master_fd, rows, cols)
        time.sleep(0.5)
        frame = drain(master_fd, 0.5)
        redrawn = b"\x1b[H" in frame
        plain = re.sub(rb"\x1b\[[0-9;?]*[a-zA-Z]", b"", frame)
        lines = [l for l in plain.split(b"\r\n") if l]
        print(
            f"resize to {rows}x{cols}: redrawn={redrawn} bytes={len(frame)} "
            f"lines={len(lines)} (want <= {rows})",
            file=sys.stderr,
        )
        if not redrawn:
            print(f"FAIL: no redraw at all after resize to {rows}x{cols}", file=sys.stderr)
            ok = False
        elif not (rows - 2 <= len(lines) <= rows):
            print(f"FAIL: line count {len(lines)} does not track requested {rows} rows", file=sys.stderr)
            ok = False

    os.write(master_fd, b"q")
    time.sleep(0.3)
    try:
        os.kill(pid, 9)
    except ProcessLookupError:
        pass
    os.waitpid(pid, 0)

    print("PASS" if ok else "FAIL", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
