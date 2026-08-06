#!/usr/bin/env python3
"""Servidor OTA on-demand para neural-os-core (ADR-0086 update).

Sobe HTTP :8080 com os endpoints que o OS consome:
  GET /UPDATE.MANIFEST  -> {"channel","version","url","sha256"} (JSON)
  GET /KERNEL.BIN       -> kernel.elf novo (blob)
  GET /api/search?q=    -> lista modelos disponíveis (skill_market)
  GET /<MODEL>.BIN      -> modelos p/ ModelProvisioner
  POST /api/logs        -> telemetria (BOOT.LOG do neural)

Segurança (S1/S2/S8/S9/S10 oracle):
  - manifest carrega sha256 do kernel (o kernel verifica ANTES de gravar o slot)
  - token de sessão gerado no start (--token ou aleatório); clientes usam
    Authorization: Bearer <token> (S2)
  - --base-url obrigatório quando --bind=0.0.0.0 (senão o KERNEL.BIN anunciado
    é http://0.0.0.0:8080 e o update nunca baixa) (S8)
  - path traversal bloqueado (resolve + parent check + \\ e .. rejeitados) (S9)
  - POST /api/logs com cap de tamanho (1MB) (S10)
  - DEV-ONLY: sem TLS, sem auth criptográfica — nunca expor em rede não-confiável.

Roda SOMENTE quando o usuário pede. Uso:
  python tools/serve_update.py --version 1.9.10
  python tools/serve_update.py --bind 192.168.137.1 --token meu-token
"""

from __future__ import annotations

import argparse
import hashlib
import json
import secrets
import sys
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_KERNEL = ROOT / "target" / "limine-esp-tree" / "kernel.elf"
LOGS_DIR = ROOT / "target" / "logs"
# Modelos servidos (provision): qualquer .BIN/.bitnet presente em target/models/, target/ ou tools/target/
MODELS_DIRS = [ROOT / "target" / "models", ROOT / "target", ROOT / "tools" / "target"]
MAX_LOG_BYTES = 1_048_576  # S10: cap de 1MB no POST /api/logs


class Handler(BaseHTTPRequestHandler):
    manifest: bytes = b""
    kernel: bytes = b""
    token: str = ""

    def log_message(self, fmt: str, *args) -> None:
        sys.stderr.write("[OTA] " + (fmt % args) + "\n")

    def _auth_ok(self) -> bool:
        # S2: se um token foi definido, exige Authorization: Bearer <token>.
        if not self.token:
            return True
        auth = self.headers.get("Authorization", "")
        return auth == f"Bearer {self.token}"

    def _reject(self, code: int, msg: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(msg)))
        self.end_headers()
        self.wfile.write(msg.encode())
        self.close_connection = True

    def do_GET(self) -> None:  # noqa: N802
        if not self._auth_ok():
            self._reject(401, "unauthorized")
            return
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
            # Modelos p/ ModelProvisioner: /HWEXPRT.BIN etc. S9: path traversal bloqueado.
            fname = path.lstrip("/")
            if fname and "/" not in fname and "\\" not in fname and ".." not in fname:
                for d in MODELS_DIRS:
                    cand = (d / fname).resolve()
                    # S9: o resolve() precisa continuar DENTRO do dir raiz
                    if not cand.is_relative_to(d.resolve()):
                        continue
                    if cand.is_file() and 10240 < cand.stat().st_size < 2_000_000_000:
                        body = cand.read_bytes()
                        ctype = "application/octet-stream"
                        sys.stderr.write(f"[OTA] modelo servido: {cand} ({len(body)} bytes)\n")
                        break
                else:
                    self._reject(404, "not found")
                    return
            else:
                self._reject(404, "not found")
                return
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Accept-Ranges", "bytes")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self) -> None:  # noqa: N802
        """ADR-0086 §3.5: neural empurra o BOOT.LOG (telemetria dev↔neural)."""
        if not self._auth_ok():
            self._reject(401, "unauthorized")
            return
        if self.path.split("?", 1)[0] != "/api/logs":
            self._reject(404, "not found")
            return
        try:
            length = int(self.headers.get("Content-Length", 0))
        except ValueError:
            self._reject(400, "bad content-length")
            return
        if length <= 0 or length > MAX_LOG_BYTES:
            # S10: cap de tamanho — evita DoS de memória/disco no host dev.
            self._reject(413, "payload too large")
            return
        body = self.rfile.read(length)
        LOGS_DIR.mkdir(parents=True, exist_ok=True)
        stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
        # S10: sufixo único evita colisão de POSTs no mesmo segundo
        seq = secrets.token_hex(2)
        path = LOGS_DIR / f"neural-{stamp}-{seq}.log"
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
                    help="URL base para o KERNEL.BIN no manifest (obrigatorio com --bind 0.0.0.0)")
    ap.add_argument("--token", default=None,
                    help="token de sessao (Authorization: Bearer). Default: aleatorio")
    args = ap.parse_args()

    if not args.kernel.exists():
        print(f"[OTA] ERRO: kernel nao encontrado em {args.kernel}", file=sys.stderr)
        print(f"[OTA] Build primeiro: cargo build --release -p boot", file=sys.stderr)
        return 1

    # S8: com bind 0.0.0.0 o manifest nao pode anunciar 0.0.0.0 (inalcancavel).
    if not args.base_url and args.bind == "0.0.0.0":
        print("[OTA] ERRO: --base-url obrigatorio com --bind 0.0.0.0 "
              "(ex: http://192.168.137.1:8080 — IP da placa ethernet/ICS)", file=sys.stderr)
        return 2

    base = args.base_url or f"http://{args.bind}:{args.port}"
    Handler.kernel = args.kernel.read_bytes()
    # S1: sha256 do kernel no manifest — o kernel verifica ANTES de gravar o slot.
    sha = hashlib.sha256(Handler.kernel).hexdigest()
    Handler.manifest = json.dumps({
        "channel": args.channel,
        "version": args.version,
        "url": f"{base}/KERNEL.BIN",
        "sha256": sha,
    }).encode() + b"\n"
    Handler.token = args.token or secrets.token_hex(8)

    print(f"[OTA] version={args.version} kernel={args.kernel} ({len(Handler.kernel)} bytes)")
    print(f"[OTA] sha256={sha}")
    print(f"[OTA] manifest: {Handler.manifest.decode().strip()}")
    print(f"[OTA] token (Authorization: Bearer {Handler.token})")
    print(f"[OTA] servindo http://{args.bind}:{args.port}  (Ctrl+C para parar)")
    print(f"[OTA] UPDATE.CFG no note 2: UPDATE_URL={base}/UPDATE.MANIFEST")
    print("[OTA] DEV-ONLY: sem TLS/auth criptografica — nao expor em rede nao-conciavel")
    try:
        httpd = ThreadingHTTPServer((args.bind, args.port), Handler)
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\n[OTA] stop", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
