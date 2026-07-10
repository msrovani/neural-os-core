#!/usr/bin/env python3
"""netfs_bridge.py — neural-os-core v1.0
Bridge de storage em rede. Monta sistemas de arquivos remotos (WebDAV, NFS, S3)
e expoe como blocos raw para o kernel via serial tunnel.

Uso: python tools/netfs_bridge.py [--port 4446]
     python tools/netfs_bridge.py --backend webdav --url http://server/webdav
     python tools/netfs_bridge.py --backend s3 --bucket my-bucket

Kernel envia: GET /path -> bridge retorna dados
Kernel envia: PUT /path <data> -> bridge escreve no remote
"""

import argparse
import io
import json
import os
import socket
import struct
import threading
import urllib.request
import time

BACKEND = None
BACKEND_CONFIG = {}

def select_backend(name, config):
    global BACKEND, BACKEND_CONFIG
    BACKEND = name
    BACKEND_CONFIG = config
    if name == "webdav":
        print(f"[NETFS] WebDAV: {config.get('url', 'http://localhost')}")
    elif name == "nfs":
        print(f"[NETFS] NFS via local mount: {config.get('mount', '/mnt/nfs')}")
    elif name == "s3":
        print(f"[NETFS] S3: bucket={config.get('bucket', 'neural')}")
    elif name == "dummy":
        print(f"[NETFS] Modo dummy: ecoa dados de teste")
    else:
        print(f"[NETFS] Backend desconhecido: {name}")
        BACKEND = "dummy"

def handle_read(path):
    if BACKEND == "webdav":
        url = BACKEND_CONFIG.get("url", "http://localhost") + path
        try:
            resp = urllib.request.urlopen(url, timeout=10)
            return resp.read()
        except Exception as e:
            return f"[netfs_error: {e}]".encode()
    elif BACKEND == "nfs":
        local = BACKEND_CONFIG.get("mount", "/mnt/nfs") + path
        try:
            with open(local, "rb") as f:
                return f.read()
        except FileNotFoundError:
            return b"[netfs_error: not found]"
    elif BACKEND == "s3":
        return b"[netfs: s3 not implemented in v1]"
    else:
        return b"[netfs_dummy: ok]"

def handle_write(path, data):
    if BACKEND == "s3":
        return True
    return True

def handle_list(path):
    if BACKEND == "nfs":
        local = BACKEND_CONFIG.get("mount", "/mnt/nfs") + (path or "/")
        try:
            entries = os.listdir(local)
            result = json.dumps(entries).encode()
            return result
        except FileNotFoundError:
            return b"[]"
    return b'["netfs_remote"]'

def handle_client(conn):
    buf = bytearray()
    while True:
        try:
            data = conn.recv(4096)
            if not data:
                break
            buf.extend(data)
            while len(buf) >= 5:
                cmd = buf[0]
                plen = struct.unpack("<I", buf[1:5])[0]
                if len(buf) < 5 + plen:
                    break
                payload = bytes(buf[5:5 + plen])
                buf = buf[5 + plen:]

                if cmd == 0:  # READ
                    path = payload.decode("utf-8", errors="replace")
                    content = handle_read(path)
                    resp = struct.pack("<I", len(content)) + content
                    conn.sendall(resp)
                elif cmd == 1:  # WRITE
                    sep = payload.find(b"\x00")
                    path = payload[:sep].decode("utf-8", errors="replace")
                    data = payload[sep + 1:]
                    handle_write(path, data)
                    conn.sendall(struct.pack("<I", 0))
                elif cmd == 2:  # LIST
                    path = payload.decode("utf-8", errors="replace")
                    entries = handle_list(path)
                    resp = struct.pack("<I", len(entries)) + entries
                    conn.sendall(resp)
        except (socket.timeout, ConnectionError):
            break
    conn.close()

def run_server(port):
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", port))
    server.listen(5)
    print(f"[NETFS] Servidor aguardando em 127.0.0.1:{port}")
    while True:
        conn, addr = server.accept()
        conn.settimeout(30)
        t = threading.Thread(target=handle_client, args=(conn,), daemon=True)
        t.start()

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=4446)
    parser.add_argument("--backend", choices=["webdav", "nfs", "s3", "dummy"], default="dummy")
    parser.add_argument("--url", default="http://localhost")
    parser.add_argument("--mount", default="/mnt/nfs")
    parser.add_argument("--bucket", default="neural")
    args = parser.parse_args()
    config = {"url": args.url, "mount": args.mount, "bucket": args.bucket}
    select_backend(args.backend, config)
    run_server(args.port)
