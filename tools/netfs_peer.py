#!/usr/bin/env python3
"""NetFs peer (#418) — raw TCP protocol for neural-os-core guest.

Protocol (request):
  cmd(1) + len(4 LE) + payload
  cmd 0 READ  — payload = path UTF-8; response = file bytes
  cmd 1 WRITE — payload = path + NUL + data; response = b"OK"
  cmd 2 LIST  — payload = path UTF-8; response = newline-separated names (dirs end with /)

Listen: 0.0.0.0:4446 (guest connects to QEMU gateway 10.0.2.2:4446).

Note: run-qemu-whpx.ps1 may hostfwd host:4446→guest:4446. If that binds first,
stop hostfwd or run this peer on another host port and point guest via guestfwd.
Default slirp: guest→10.0.2.2:4446 reaches a process listening on the host :4446
when hostfwd is not occupying that port.

  python tools/netfs_peer.py --root target/netfs_root
"""

from __future__ import annotations

import argparse
import socket
import struct
import sys
import threading
from pathlib import Path

CMD_READ = 0
CMD_WRITE = 1
CMD_LIST = 2


def recv_exact(conn: socket.socket, n: int) -> bytes | None:
    buf = bytearray()
    while len(buf) < n:
        chunk = conn.recv(n - len(buf))
        if not chunk:
            return None
        buf.extend(chunk)
    return bytes(buf)


def handle(conn: socket.socket, root: Path) -> None:
    try:
        hdr = recv_exact(conn, 5)
        if hdr is None:
            return
        cmd = hdr[0]
        (plen,) = struct.unpack("<I", hdr[1:5])
        if plen > 16 * 1024 * 1024:
            conn.sendall(b"ERR:too_large")
            return
        payload = recv_exact(conn, plen) if plen else b""
        if payload is None:
            return
        if cmd == CMD_READ:
            path = payload.decode("utf-8", errors="replace").lstrip("/")
            fp = (root / path).resolve()
            if not str(fp).startswith(str(root.resolve())) or not fp.is_file():
                conn.sendall(b"")
                return
            conn.sendall(fp.read_bytes())
        elif cmd == CMD_WRITE:
            nul = payload.find(b"\x00")
            if nul < 0:
                conn.sendall(b"ERR:bad_write")
                return
            path = payload[:nul].decode("utf-8", errors="replace").lstrip("/")
            data = payload[nul + 1 :]
            fp = (root / path).resolve()
            if not str(fp).startswith(str(root.resolve())):
                conn.sendall(b"ERR:path")
                return
            fp.parent.mkdir(parents=True, exist_ok=True)
            fp.write_bytes(data)
            conn.sendall(b"OK")
        elif cmd == CMD_LIST:
            path = payload.decode("utf-8", errors="replace").lstrip("/") or "."
            dp = (root / path).resolve()
            if not str(dp).startswith(str(root.resolve())) or not dp.is_dir():
                conn.sendall(b"")
                return
            lines: list[str] = []
            for p in sorted(dp.iterdir(), key=lambda x: x.name.lower()):
                name = p.name + ("/" if p.is_dir() else "")
                lines.append(name)
            conn.sendall("\n".join(lines).encode("utf-8"))
        else:
            conn.sendall(b"ERR:cmd")
    finally:
        try:
            conn.close()
        except OSError:
            pass


def main() -> int:
    ap = argparse.ArgumentParser(description="NetFs TCP peer :4446")
    ap.add_argument("--bind", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=4446)
    ap.add_argument("--root", type=Path, default=Path("target/netfs_root"))
    args = ap.parse_args()
    args.root.mkdir(parents=True, exist_ok=True)
    # seed so LIST/READ smoke has something
    sample = args.root / "hello.txt"
    if not sample.exists():
        sample.write_text("netfs-ok\n", encoding="utf-8")
    (args.root / "subdir").mkdir(exist_ok=True)

    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((args.bind, args.port))
    srv.listen(8)
    print(f"[NETFS-PEER] listen {args.bind}:{args.port} root={args.root.resolve()}", flush=True)

    while True:
        conn, addr = srv.accept()
        print(f"[NETFS-PEER] accept {addr}", flush=True)
        threading.Thread(target=handle, args=(conn, args.root), daemon=True).start()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("\n[NETFS-PEER] stop", file=sys.stderr)
        raise SystemExit(0)
