#!/usr/bin/env python3
"""Registra SESSION_433/434/435 + ideia #630 (residual s435) no neural-sgdb via MCP (JSON-RPC stdio).

Contrato (AGENTS.md / Sync n-sgdb):
  scope      = project/neural-os-core
  SESSION    -> 1 memoria compacta (entities session-NNN + mom/fact)
  IDEA_BANK  -> 1 memoria por ideia (entities idea-NNN + mom/fact)
  fim do lote -> curate(op=commit_run) + health view=validate

Idempotente: recall por entity antes de escrever; comparacao por PREFIXO
do texto (key logica nao aparece no recall JSON).

Uso: PYTHONIOENCODING=utf-8 python tools/sgdb_register_s435.py [--dry-run]
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

SESSION_433 = (
    "SESSION_433 (2026-10-01) — HUB triage lane LLM: Propose submete o snapshot HUB\\0+JSON ao LLM via "
    "InferQueue (reply HUB_TRIAGE_LLM, prompt com instrucao deterministica: responda {\"title\":..,\"action\":..} "
    "ou {}), parse sem serde (scanner JSON com escapes + UTF-8 lossy, CAP 256B/campo), publica a acao GERADA "
    "via HITL. Modelo ausente/recusa/[heap escalate]/timeout 60s -> fallback heuristico (s432); {} = declinio "
    "honesto (sem toast). Anti-loop: fp da heuristica reservado NO SUBMIT (PendingProposal), fp da acao gerada "
    "anotado na publicacao; dedupe cooldown 10min provado. Gates de headroom em toda publicacao (publish_proposal_hitl) "
    "+ gate duplo antes do submit (should_try_llm = modelo carregado + !heap_headroom_low). Agente drena replies "
    "em qualquer tick (llm_receiver no has_pending — lost-wakeup s411). QEMU 8GB/8c log 132152: submitted id=4 "
    "(T+29433) -> fallback-timeout honesto (T+33118) -> HITL -> MoE -> LLM (T+33860); dedupe T+35110. "
    "Residual: stall silencioso pos-OOM/TALC infer_worker (3a sessao)."
)

SESSION_434 = (
    "SESSION_434 (2026-10-01) — Causa-raiz do OOM/TALC infer_worker (idea #630, 3 sessoes) RESOLVIDA: era realloc "
    "de chunk BUMP-residente pelo default GlobalAlloc::realloc (alloc novo do bump puro SEM overflow do TALC -> NULL "
    "com janela ~2030MB cheia, 6911MB de TALC intocados -> oom() cego). Fix (s434c): realloc bump-residente -> "
    "alloc pelo hibrido (TALC da o espaco) + copy manual. Gap 2 (s434b): Talck::realloc/alloc_zeroed chamam o malloc "
    "INTERNO do talc — NULL sem counter/snapshot -> instrumentado (snapshot bins + counter + oom()). Fail-closed de "
    "classe: oom() saiu do loop{hlt} -> spin + heartbeat [OOM-HALT] 10s (quebrou stall silencioso; revelou N cores "
    "parkados, padrao 146/10624 alternando ~140 ticks). Instrumentacao: TALC_PF_OUTSIDE_SPAN, TALC_NULL_BINS_NONEMPTY/"
    "_EMPTY, snapshot one-shot availability (layout Talc 4.4.3: avail_low@0 avail_high@8 bins@16), "
    "PF_DIAG_PT_ALLOC_FAIL, pmm_free_frames/allocated_count/total_frames, [OOM-DIAG] zero-alloc, stop-the-world "
    "bounded no claim. Validacao QEMU 8GB/8c log 144435: 0x OOM/TALC em ~16min (T+57771 recorde; morria em T+34k), "
    "bump no teto com sistema vivo. Licoes: counter no path errado = pior que nenhum; GlobalAlloc::realloc default = "
    "armadilha em allocator hibrido; Talck::realloc/alloc_zeroed nao passam pelo alloc do usuario."
)

SESSION_435 = (
    "SESSION_435 (2026-10-01) — Telemetria de uso REAL do TALC (idea #630 residual: monitorar fragmentacao em "
    "runtime de horas). talc_walk_bins percorre os gap-nodes dos 128 bins do talc 4.4.3 — layout confirmado no FONTE "
    "(bins = array de sentinelas Option<NonNull<LlistNode>> @16; gap-node next@0/size@16; terminacao None; Tag nunca "
    "nos bins); free = soma dos gaps, used = span-free, largest_free = maior gap contiguo, gaps = n fragmentos; "
    "CAP 4096 + bounds-check no span -> partial=1 honesto em vez de pendar/mintir; zero alloc. Cache 2 Hz "
    "(talc_refresh_usage/talc_usage/talc_usage_samples) sob o lock do Talck + seed pos-claim; NUNCA no caminho de "
    "alloc. Headroom honesto: heap_headroom_bytes/heap_observe somam o FREE medido (estimativa 'span inteiro' "
    "aposentada); HeapObserve +5 campos talc_*. HUB HEALTH linha 'talc' (21a, HUB_ROWS 20->21): u{}/{}M lg{}M g{} "
    "com Warn de fragmentacao (free>=256MB e largest*4<free), 'partial g{}', 'n/a' sem amostra. hub_triage: snapshot "
    "JSON com talc_used/free/largest/gaps/partial + vereditos Observe ('talc fragmentado (largest/free baixo)' e "
    "'talc metadata parcial') + slog 'ok talc u..f..lg..g..' (regra 419). Validacao: hermes 275/275 (2 novos), "
    "k-nano 244/244, jarbas 134/134 (-t1); QEMU 8GB/8c log 182016: 'ok talc u0M f6911M lg6911M g1' telemetria viva "
    "1/min, zero OOM, sistema vivo. Licoes: lista de gaps termina em None (ler layout do FONTE, nao por analogia); "
    "dado lido sob o lock do proprio escritor = consistencia sem barreira."
)

IDEA_630 = (
    "#630 (implementada) Instrumentar o path de overflow do TALC + telemetria real. s434: causa-raiz do OOM/TALC "
    "infer_worker = 2 gaps de realloc (default realloc bump-residente sem overflow; Talck::realloc malloc interno "
    "sem counter) + fail-closed de classe (oom() hlt -> spin + heartbeat [OOM-HALT] 10s); 0x OOM em ~16min "
    "(T+57771 recorde). s435 (residual fragmentacao): telemetria de uso REAL em runtime — talc_walk_bins (gap-nodes "
    "dos 128 bins, free/used/largest/gaps/partial), cache 2 Hz + seed pos-claim, headroom = FREE medido (span inteiro "
    "aposentado), linha 'talc' 21a no HUB HEALTH com Warn de fragmentacao (free>=256MB e largest*4<free), hub_triage "
    "snapshot JSON + vereditos Observe + slog 'ok talc u..f..lg..g..'. QEMU log 182016: u0M f6911M lg6911M g1 viva, "
    "zero OOM. Residual pendente (agendado): calibrar limiar largest*4<free em runtime de horas. "
    "Fonte: SESSION_434/435, k_nano/allocator.rs, jarbas/display/gauges.rs, hermes/hub_triage.rs."
)


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
    aparece no recall JSON — so a storage key md/L3/mcp/<ts>)."""
    txt = mcp.call("recall", {
        "entities": [entity], "scope": SCOPE, "k": 20, "format": "json",
    })
    prefix = json.dumps(text[:80], ensure_ascii=False)[1:-1]
    return prefix in txt


def main() -> int:
    dry = "--dry-run" in sys.argv
    mcp = Mcp()
    ok = skip = fail = 0
    try:
        batch = [
            ("session/433", "session-433", SESSION_433),
            ("session/434", "session-434", SESSION_434),
            ("session/435", "session-435", SESSION_435),
            ("idea/630", "idea-630", IDEA_630),
        ]
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
                "scope_run": "s433-s435-registration",
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
