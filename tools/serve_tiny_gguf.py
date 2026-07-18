#!/usr/bin/env python3
"""Serve a minimal fake GGUF blob for AirLLM Net smoke (QEMU guest → host).

Writes TINY.GGUF (magic "GGUF" + little version/tensors/kv = 0) and serves HTTP on :8080.

QEMU guest path (user/slirp): guest reaches host at 10.0.2.2 — no guestfwd needed.
  python tools/serve_tiny_gguf.py
  # guest: http://10.0.2.2:8080/TINY.GGUF

run-qemu-whpx.ps1 does NOT add guestfwd for :8080; serving on the host and using
10.0.2.2 from the guest is the supported path. Optional guestfwd example if you
wire it yourself:
  -netdev user,id=n0,guestfwd=tcp:10.0.2.2:8080-tcp:127.0.0.1:8080
"""

from __future__ import annotations

import argparse
import struct
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUT = ROOT / "target" / "TINY.GGUF"


def write_tiny_gguf(path: Path) -> bytes:
    """Minimal GGUF v3 header: magic + version + tensor_count + kv_count (all LE)."""
    blob = b"GGUF" + struct.pack("<IQQ", 3, 0, 0)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(blob)
    return blob


class Handler(BaseHTTPRequestHandler):
    blob: bytes = b""

    def log_message(self, fmt: str, *args) -> None:
        sys.stderr.write("[TINY-GGUF] " + (fmt % args) + "\n")

    def do_GET(self) -> None:  # noqa: N802
        path = self.path.split("?", 1)[0]
        if path in ("/", "/TINY.GGUF", "/tiny.gguf"):
            body = self.blob
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Accept-Ranges", "bytes")
            self.end_headers()
            self.wfile.write(body)
            return
        if path == "/UPDATE.MANIFEST":
            body = b'{"channel":"stable","version":"0.0.0-dev","url":"http://10.0.2.2:8080/KERNEL.BIN"}\n'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_error(404, "not found")


def main() -> int:
    ap = argparse.ArgumentParser(description="Serve tiny GGUF for QEMU AirLLM Net smoke")
    ap.add_argument("--bind", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=8080)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()

    blob = write_tiny_gguf(args.out)
    Handler.blob = blob
    httpd = ThreadingHTTPServer((args.bind, args.port), Handler)
    print(
        f"[TINY-GGUF] wrote {args.out} ({len(blob)} bytes); "
        f"serving http://{args.bind}:{args.port}/TINY.GGUF "
        f"(guest: http://10.0.2.2:{args.port}/TINY.GGUF)",
        flush=True,
    )
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\n[TINY-GGUF] stop", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
