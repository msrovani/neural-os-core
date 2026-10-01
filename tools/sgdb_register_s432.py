#!/usr/bin/env python3
"""Registra SESSION_432 + ideias #626-#629 no neural-sgdb via MCP (JSON-RPC stdio).

Contrato (AGENTS.md / Sync n-sgdb):
  scope      = project/neural-os-core
  SESSION    -> 1 memoria compacta (entities session-432 + mom/fact)
  IDEA_BANK  -> 1 memoria por ideia (entities idea-62X + mom/fact)
  fim do lote -> curate(op=commit_run) + health view=validate

Idempotente: recall por entity antes de escrever; key explicita
(session/432, idea/62X) = skip se ja existe.

Uso: PYTHONIOENCODING=utf-8 python tools/sgdb_register_s432.py [--dry-run]
"""
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MCP_BIN = Path(r"C:\DEV\neural-sgdb\target\release\examples\mcp_server.exe")
DB = ROOT / ".nsgdb" / "sgdb_memory.db"
SCOPE = "project/neural-os-core"

SESSION_432 = (
    "SESSION_432 (2026-10-01) — WHPX 6-lane hardening (log 213145) + triagem IA do HUB. "
    "Lanes: (1) hlt gate WHPX -> pause-spin sob MicrosoftHv (interrupts.rs:83-105; hlt sem wake = stall); "
    "(2) yields audio/HUB sob heap pressionado — capture caps 4ev/8fr + drains cap 8; "
    "(3) refuse-in-slice -> TALC spill como servico efetivo, sem deferred-runner (huge-2MB gate OFF, idea #626); "
    "(4) HDA LPIB-frozen estimate dt*48000 quando o LPIB congela; "
    "(5) submit-only proof + ps1 -ModelKind 1b|3b; "
    "(6) posture MIN 4->8 + piso absoluto esc>=8 (fim do FAIL com 1 escalate, 11x no log). "
    "HUB triage IA (idea #629, premissa maxima ADR-0088): hermes/hub_triage.rs — snapshot HUB\\0+JSON 1/min "
    "(3600 ticks @60Hz), triagem worst-state pura, proposta HITL via toast + USER_INTENT, dedupe FNV-1a + cooldown 10min. "
    "Validacao QEMU 8GB/8c (log 224429, 13,9min): proposta HITL T+36581 (heap critico + arena livre -> mover "
    "conversao/context-window p/ arena Cortex) -> intent -> MoE disk_diag 0.975 -> LLM invocado; 4x Observe anti-loop; "
    "dedupe por cooldown provado. Residual: 2x OOM/TALC infer_worker (size=10624/73) com span 6911MB — causa do null "
    "desconhecida, instrumentar TALC_OVERFLOW_NULL/PF_DIAG (relacionada idea #627)."
)

IDEAS = [
    ("idea/626", "idea-626",
     "#626 (agendado) huge-page flip do grow: flip HEAP_GROW_HUGE_2MB so apos validacao tok/s QEMU "
     "(gate OFF em s432, allocator.rs:160-281). Fonte: SESSION_432."),
    ("idea/627", "idea-627",
     "#627 (idea) seam per-CPU infer-stamp cortex->k_nano p/ refuse total do grow: stamp global nao cobre "
     "AP-slice x BSP-tick (residual fix-3 s432). Fonte: SESSION_432, allocator.rs."),
    ("idea/628", "idea-628",
     "#628 (idea) HOWTO 1B doc line: documentar lab 1B (skip fix-5: submit-only proof cobre via -ModelKind). "
     "Fonte: SESSION_432, tools/measure-falcon3-toks.ps1."),
    ("idea/629", "idea-629",
     "#629 (implementada) Triagem IA do HUB HEALTH (premissa maxima ADR-0088): snapshot HUB\\0+JSON 1/min "
     "alimentando o Hermes, triagem worst-state pura (heap>=90%+arena livre>=64MB -> Propose mover p/ arena; "
     "heap>=95% -> Propose reduzir carga; posture FAIL/sched lag -> Observe), proposta HITL via toast + "
     "USER_INTENT, dedupe FNV-1a + cooldown 10min (anti-loop s410d). Validada QEMU 8GB/8c log 224429 "
     "(proposta T+36581 -> MoE -> LLM; 4x Observe). Residual: proposta via LLM (hoje heuristica worst-state) "
     "+ OOM/TALC infer_worker. Fonte: SESSION_432, hermes/hub_triage.rs."),
]


class Mcp:
    def __init__(self):
        if not MCP_BIN.exists():
            raise RuntimeError(f"MCP binario ausente: {MCP_BIN}")
        DB.parent.mkdir(parents=True, exist_ok=True)
        env = dict(os.environ)
        env["NEURAL_SGDB_DB"] = str(DB)
        env["NEURAL_SGDB_DEFAULT_SCOPE"] = SCOPE
        self.child = subprocess.Popen(
            [str(MCP_BIN)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, env=env, text=True, encoding="utf-8", bufsize=1,
        )
        self.id = 0

    def send(self, method, params=None):
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

    def call(self, name, args):
        r = self.send("tools/call", {"name": name, "arguments": args})
        return "\n".join(c.get("text", "") for c in r.get("content", []) if c.get("type") == "text")

    def close(self):
        try:
            self.child.stdin.close()
        except Exception:
            pass
        self.child.wait(timeout=10)


def key_exists(mcp: Mcp, entity: str, key: str, text: str) -> bool:
    """Recall por entity e compara o PREFIXO do texto (a key logica nao
    aparece no recall JSON — so a storage key md/L3/mcp/<ts>; lição do
    1º run: check por key logica deixou gravar duplicatas)."""
    txt = mcp.call("recall", {
        "entities": [entity], "scope": SCOPE, "k": 20, "format": "json",
    })
    prefix = json.dumps(text[:80], ensure_ascii=False)[1:-1]  # escapado como no JSON
    return prefix in txt


def main() -> int:
    dry = "--dry-run" in sys.argv
    mcp = Mcp()
    ok = skip = fail = 0
    try:
        batch = [("session/432", "session-432", SESSION_432)] + IDEAS
        for key, entity, text in batch:
            if key_exists(mcp, entity, key, text):
                print(f"  [skip] {key} ja registrado")
                skip += 1
                continue
            if dry:
                print(f"  [dry] {key} ({len(text)} bytes)")
                ok += 1
                continue
            try:
                out = mcp.call("remember", {
                    "key": key,
                    "text": text,
                    "scope": SCOPE,
                    "entities": [entity, "mom/fact"],
                    "type": "text",
                })
                print(f"  [ok] {key}: {out[:120]}")
                ok += 1
            except Exception as e:
                print(f"  [FAIL] {key}: {e}")
                fail += 1

        if not dry and fail == 0:
            now = int(time.time() * 1000)
            out = mcp.call("curate", {
                "op": "commit_run",
                "scope": SCOPE,
                "scope_run": "s432-registration",
                "archive_remaining_episodic": True,
                "audit": True,
                "now": now,
            })
            print(f"  [commit_run] {out[:200]}")
            out = mcp.call("health", {"view": "validate"})
            print(f"  [health validate] {out[:200]}")
    finally:
        mcp.close()
    print(f"[register] ok={ok} skip={skip} fail={fail} db={DB}")
    return 0 if fail == 0 else 2


if __name__ == "__main__":
    sys.exit(main())
