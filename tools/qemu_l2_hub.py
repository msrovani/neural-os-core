#!/usr/bin/env python3
"""L2 hub for multi-QEMU on Windows (mcast socket is unreliable).

Each guest:
  -netdev socket,id=n0,udp=127.0.0.1:HUB,localaddr=127.0.0.1:PEER

Hub binds HUB, learns peer source addrs, fans out Ethernet frames.
"""
from __future__ import annotations

import argparse
import select
import socket
import sys


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=19000)
    ap.add_argument("--bind", default="127.0.0.1")
    args = ap.parse_args()

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind((args.bind, args.port))
    sock.setblocking(False)
    peers: set[tuple[str, int]] = set()
    print(f"[l2-hub] listening {args.bind}:{args.port}", flush=True)

    while True:
        r, _, _ = select.select([sock], [], [], 1.0)
        if not r:
            continue
        try:
            data, addr = sock.recvfrom(65535)
        except OSError:
            continue
        if len(data) < 14:
            continue
        peers.add(addr)
        for peer in list(peers):
            if peer == addr:
                continue
            try:
                sock.sendto(data, peer)
            except OSError:
                peers.discard(peer)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(0)
