#!/usr/bin/env python3
"""Servidor OTA on-demand para neural-os-core (ADR-0086 update).

Sobe HTTP :8080 com os dois endpoints que o OS consome:
  GET /UPDATE.MANIFEST  -> {"channel","version","url"} (JSON)
  GET /KERNEL.BIN       -> kernel.elf novo (blob)

Roda SOMENTE quando o usuário pede (não é daemon). Uso:
  python tools/serve_update.py                       # serve target/limine-esp-tree/kernel.elf
  python tools/serve_update.py --version 1.9.10      # anuncia versão 1.9.10
  python tools/serve_update.py --kernel target/kernel.elf
  python tools/serve_update.py --bind 0.0.0.0 --port 8080

Cenário 2 notes via cabo + ICS do Windows: o note 1 (este) roda o servidor
e o note 2 (rodando o OS) consulta via UPDATE.CFG na FAT32:
  UPDATE_URL=http://<ip-do-note-1>:8080/UPDATE.MANIFEST
No QEMU (user net) o guest alcança o host em 10.0.2.2.
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_KERNEL = ROOT / "target" / "limine-esp-tree" / "kernel.elf"
LOGS_DIR = ROOT / "target" / "logs"
# Modelos servidos (provision): qualquer .BIN/.bitnet presente em target/models/, target/ ou tools/target/
MODELS_DIRS = [ROOT / "target" / "models", ROOT / "target", ROOT / "tools" / "target"]


class Handler(BaseHTTPRequestHandler):
    manifest: bytes = b""
    kernel: bytes = b""

    def log_message(self, fmt: str, *args) -> None:
        sys.stderr.write("[OTA] " + (fmt % args) + "\n")

    def do_GET(self) -> None:  # noqa: N802
        path, _, query = self.path.partition("?")
        if path in ("/UPDATE.MANIFEST", "/update.manifest"):
            body = self.manifest
            ctype = "application/json"
        elif path in ("/KERNEL.BIN", "/kernel.bin", "/kernel.elf"):
            body = self.kernel
            ctype = "application/octet-stream"
        elif path == "/api/search":
            # Item 5 ADR-0056: lista pacotes/modelos disponíveis (skill_market).
            q = query.replace("q=", "").strip().lower()
            hits = []
            for d in MODELS_DIRS:
                if not d.is_dir():
                    continue
                for f in sorted(d.iterdir()):
                    if f.is_file() and 10240 < f.stat().st_size:
                        name = f.name
                        if q and q not in name.lower():
                            continue
                        hits.append(f"  {name} ({f.stat().st_size} bytes)")
            body = ("[MARKET] pacotes disponiveis:\n" + "\n".join(hits) + "\n").encode()
            ctype = "text/plain"
        else:
            # Modelos p/ ModelProvisioner (ADR-0086): /HWEXPRT.BIN etc. de target/models/ ou target/
            fname = path.lstrip("/")
            if fname and "/" not in fname:
                for d in MODELS_DIRS:
                    cand = d / fname
                    if cand.is_file() and 10240 < cand.stat().st_size < 2_000_000_000:
                        body = cand.read_bytes()
                        ctype = "application/octet-stream"
                        sys.stderr.write(f"[OTA] modelo servido: {cand} ({len(body)} bytes)\n")
                        break
                else:
                    self.send_error(404, "not found")
                    return
            else:
                self.send_error(404, "not found")
                return
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Accept-Ranges", "bytes")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self) -> None:  # noqa: N802
        """ADR-0086 §3.5: neural empurra o BOOT.LOG (telemetria dev↔neural)."""
        if self.path.split("?", 1)[0] != "/api/logs":
            self.send_error(404, "not found")
            return
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length) if length > 0 else b""
        LOGS_DIR.mkdir(parents=True, exist_ok=True)
        stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
        path = LOGS_DIR / f"neural-{stamp}.log"
        path.write_bytes(body)
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", "2")
        self.end_headers()
        self.wfile.write(b"ok")
        sys.stderr.write(f"[OTA] log recebido: {path} ({len(body)} bytes)\n")


def main() -> int:
    ap = argparse.ArgumentParser(description="Serve OTA update blob for neural-os-core")
    ap.add_argument("--bind", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=8080)
    ap.add_argument("--version", default="1.9.10", help="versao anunciada no manifest")
    ap.add_argument("--channel", default="stable")
    ap.add_argument("--kernel", type=Path, default=DEFAULT_KERNEL,
                    help="caminho do kernel.elf novo")
    ap.add_argument("--base-url", default=None,
                    help="URL base para o KERNEL.BIN no manifest (default: http://<bind>:<port>)")
    args = ap.parse_args()

    if not args.kernel.exists():
        print(f"[OTA] ERRO: kernel nao encontrado em {args.kernel}", file=sys.stderr)
        print(f"[OTA] Build primeiro: cargo build --release -p boot", file=sys.stderr)
        return 1

    base = args.base_url or f"http://{args.bind}:{args.port}"
    Handler.manifest = json.dumps({
        "channel": args.channel,
        "version": args.version,
        "url": f"{base}/KERNEL.BIN",
    }).encode() + b"\n"
    Handler.kernel = args.kernel.read_bytes()

    print(f"[OTA] version={args.version} kernel={args.kernel} ({len(Handler.kernel)} bytes)")
    print(f"[OTA] manifest: {Handler.manifest.decode().strip()}")
    print(f"[OTA] servindo http://{args.bind}:{args.port}  (Ctrl+C para parar)")
    print(f"[OTA] UPDATE.CFG no note 2: UPDATE_URL={base}/UPDATE.MANIFEST")
    try:
        httpd = ThreadingHTTPServer((args.bind, args.port), Handler)
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\n[OTA] stop", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
