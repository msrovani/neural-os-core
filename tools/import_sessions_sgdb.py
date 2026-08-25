#!/usr/bin/env python3
"""Importa docs/memory/SESSION_*.md para o neural-sgdb via MCP (JSON-RPC stdio).

Cada SESSION_NNN.md vira 1 doc L3 lexical (ADR-0008, sem embedding) com:
  scope = project/neural-os-core
  entities = [session/NNN]
  text = cabecalho + corpo verbatim

Idempotente: se a key md/L3/session/NNN ja existe (recall por entity), pula.
Uso:
  python tools/import_sessions_sgdb.py [--limit N] [--dry-run] [--fresh]
"""
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MCP_BIN = Path(r"C:\DEV\neural-sgdb\target\release\examples\mcp_server.exe")
DB = ROOT / ".nsgdb" / "sgdb_memory.db"
SCOPE = "project/neural-os-core"
SESSIONS_DIR = ROOT / "docs" / "memory"

PROTO = "2025-11-25"


class Mcp:
    def __init__(self, fresh: bool):
        if fresh and DB.exists():
            DB.unlink()
        DB.parent.mkdir(parents=True, exist_ok=True)
        env = dict(os.environ)
        env["NEURAL_SGDB_DB"] = str(DB)
        env["NEURAL_SGDB_DEFAULT_SCOPE"] = SCOPE
        self.child = subprocess.Popen(
            [str(MCP_BIN)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=env,
            text=True,
            encoding="utf-8",
            bufsize=1,
        )
        self.id = 0

    def send(self, method: str, params: dict | None = None) -> dict:
        self.id += 1
        msg = {"jsonrpc": "2.0", "id": self.id, "method": method}
        if params is not None:
            msg["params"] = params
        self.child.stdin.write(json.dumps(msg) + "\n")
        self.child.stdin.flush()
        while True:
            line = self.child.stdout.readline()
            if not line:
                raise RuntimeError("MCP server fechou stdout")
            resp = json.loads(line)
            if resp.get("id") == self.id:
                if "error" in resp:
                    raise RuntimeError(f"MCP error: {resp['error']}")
                return resp.get("result", {})

    def call(self, name: str, args: dict) -> dict:
        r = self.send("tools/call", {"name": name, "arguments": args})
        content = r.get("content", [])
        txt = "\n".join(c.get("text", "") for c in content if c.get("type") == "text")
        return {"raw": txt}

    def close(self):
        try:
            self.child.stdin.close()
        except Exception:
            pass
        self.child.wait(timeout=10)


def parse_session(path: Path) -> tuple[str, str]:
    """Retorna (tag da sessao, texto para remember)."""
    m = re.match(r"SESSION_(\d+)(?:_(.+))?\.md", path.name)
    if m:
        num, suffix = m.group(1), m.group(2)
        tag = f"session/{num}" + (f"-{suffix.lower()}" if suffix else "")
    else:
        tag = f"session/{path.stem}"
    text = path.read_text(encoding="utf-8", errors="replace")
    return tag, text


def existing_keys(mcp: Mcp) -> set[str]:
    """Chaves de sessions ja importadas (via scan de recall por entity)."""
    have = set()
    # health view=validate nao lista; usamos curate op=explain? Nao ha list tool.
    # Estrategia: tentativa de leitura por entity em lote e' cara; simplesmente
    # confiamos no idempotente do remember (nova key por now monotonico) e
    # marcamos importados num doc indice local.
    marker = DB.parent / "imported_sessions.txt"
    if marker.exists():
        have = set(marker.read_text(encoding="utf-8").split())
    return have


def main() -> int:
    dry = "--dry-run" in sys.argv
    fresh = "--fresh" in sys.argv
    limit = None
    if "--limit" in sys.argv:
        limit = int(sys.argv[sys.argv.index("--limit") + 1])

    files = sorted(SESSIONS_DIR.glob("SESSION_*.md"))
    if limit:
        files = files[:limit]

    print(f"[import] {len(files)} sessions encontradas em {SESSIONS_DIR}")
    if not MCP_BIN.exists():
        print(f"[import] ERRO: MCP binario ausente: {MCP_BIN}")
        return 1

    mcp = Mcp(fresh=fresh)
    imported = existing_keys(mcp)
    marker = DB.parent / "imported_sessions.txt"
    ok = skip = fail = 0
    try:
        for f in files:
            tag, text = parse_session(f)
            if tag in imported:
                skip += 1
                continue
            if len(text) > 60_000:
                # FileStorage aguenta, mas capamos por sanidade (BM25 indexa igual)
                text = text[:60_000] + "\n\n[... truncado no import ...]"
            if dry:
                print(f"  [dry] {f.name} ({len(text)} bytes)")
                ok += 1
                continue
            try:
                mcp.call("remember", {
                    "text": text,
                    "scope": SCOPE,
                    "entities": [tag],
                    "type": "text",
                })
                ok += 1
                imported.add(tag)
                marker.write_text("\n".join(sorted(imported)), encoding="utf-8")
                print(f"  [ok] {f.name} -> {tag}")
            except Exception as e:
                fail += 1
                print(f"  [FAIL] {f.name}: {e}")
    finally:
        mcp.close()

    print(f"[import] ok={ok} skip={skip} fail={fail} db={DB}")
    return 0 if fail == 0 else 2


if __name__ == "__main__":
    sys.exit(main())
