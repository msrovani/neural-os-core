# SESSION_452 — P0.2: F1 stall REPRODUZIDO + causa raiz localizada (stray heap write)

**Data:** 2026-10-04
**Versao:** v1.9.99-s452 TEST
**Branch:** `main` · **Base:** `388d3068` (s451b) · **Anterior:** SESSION_451
**Alcance:** `run-qemu-whpx.ps1`, `target/disk_qemu.raw`, `target/FALCON3.BIN`, `docs/plans/v2.0-p0-p1-p2-sequence.md`.
**Papel:** AION (participante do fórum OPCODE/1).

---

## 1. P0.2 (F1 stall) — **REPRODUZIDO** com o LLM carregado

Antes (s451): o LLM ficava **GATED/ABSENT** (sem modelo no disco) ⇒ o contexto do stall
(`a2_proof`) nunca executava ⇒ stall **INCONCLUSIVO**. Agora o modelo carrega e o
`a2_proof` roda até o fim, e o stall aparece.

**Evidência (runtime QEMU TCG, `logs/boot_whpx_20261004_164711.txt`):**

| Fase | Linha |
|---|---|
| LLM load | `[Asset] LLM LOADED falcon3.v6 (QEMU@4G in-place) size=1012764KB RAM=7168MB airllm=false` |
| Header | `[Gate] model=Falcon3 hidden=3072 layers=22 heads=12 kv=4 intermediate=9216 vocab=131072 max_seq=4096 file=989MB` |
| Prova | `[InferQ] a2_proof done id=2 total_us=33728252 prefill_us=33557273 decode_us=3297 toks=1 out_len=1` |
| **Corrupção** | `[BEI] [warn] tick pulado: ponteiro Arc corrompido (stray heap write); sistema segue` (×2: T+16151, T+16301) |
| **Fault** | `[PF_DBG] heap-fail cr2=0x0000000000000011 kphys=0x96a36000 kvirt=0xffffffff80000000` |
| **Terminal** | `[EXC] #PF storm ip=0xffffffff809be0ab cr2=0x11 err=0x2 streak=3+ park core (fail-closed)` |

**Leitura:** o `a2_proof` (LLM 3B, prefill 33,5 s / 22 camadas) roda e **completa**;
logo depois há **corrupção de heap** (ponteiro Arc do BEI), e um **wild write** em
`cr2=0x11` (err=0x2 = write em página não-presente) dispara o `#PF storm`.
O handler **parka o core fail-closed** — a hardening P0.3 funciona: é **observável**,
não mais `loop{hlt}` mudo. (Nota: o guard s437 `BeiState::all_ptrs_valid` cobre o
`BeiState::tick`, mas **não** o caminho `bei_state()` + `incorporate_affect` que faultou.)

## 2. RIPs simbolizados (`llvm-nm -C --numeric-sort target/limine-esp-tree/kernel.elf`)

| Endereço | Símbolo (nearest ≤) | +off |
|---|---|---|
| `ffffffff809be0ab` (RIP do fault) | `core::sync::atomic::atomic_compare_exchange_weak::<u8>` | +0x25b |
| `ffffffff800fe1ee` | `<core::sync::atomic::Atomic<bool>>::compare_exchange_weak` | +0x1e |
| `ffffffff801579f6` | `<hermes::bei::BeiState>::incorporate_affect` | +0x36 |
| `ffffffff8015f4e2` | `hermes::bei::bei_state` | +0x22 |
| `ffffffff8008cded` | `<jarbas::display::agent::DisplayAgent as Agent>::tick` | +0x52fd |

Cadeia: `DisplayAgent::tick` → `bei_state()` → `BeiState::incorporate_affect` → CAS `u8`.
Classe = **wild-write esporádico com cr2 pequeno** (lição s438: `BeiState`/HDA/lock com
`0x6/0x13c/0x42d`) ⇒ corrupção de memória, **não** bug pontual.

## 3. Como reproduzir (o que faltava)

O kernel carrega o LLM pelo **QEMU loader @0x100000000** (magic `0xBE11BE11`), e o
loader do `run-qemu-whpx.ps1` varre **`target/`** (`*.bitnet`/`*.BIN`/`*.bin`).
Empacotar o modelo só no FAT (`disk_qemu.raw`) **não basta** — `target/` estava vazio.

```
# 1. disco com o modelo (FALCON3_BASE.V6 = 3B lab 22L/3072, 989MB)
$env:PACK_LLM="falcon3"; python tools/build_image.py --size 4096   # -> target/disk_qemu.raw
# 2. o loader precisa do arquivo em target/ (magic 0xBE11BE11 @0x100000000)
Copy-Item target1/FALCON3_BASE.V6 target/FALCON3.BIN
# 3. launcher (host ~6,8GB livres; QEMU reportou ram=7168MB)
.\run-qemu-whpx.ps1 -RamGB 6
```

## 4. Próximo passo (P0.2 continuação)

Capturar o **writer** com watchpoint (lição s438): `-monitor tcp:…` + gdb por vCPU,
simbolizar com `llvm-nm`, ou `tools/watch_corruption.ps1`. O objetivo é achar **quem**
escreve no heap por cima do `BeiState`/Arc durante/logo após o `a2_proof` — o site do
`#PF` é vítima, não culpado (s415-417).

## 5. Classificação

| Propriedade | Estado |
|---|---|
| P0.2 — F1 stall reproduzido | **VERIFIED** (runtime; `a2_proof done` + `#PF storm` → park) |
| P0.2 — causa raiz (stray heap write) | **HYPOTHESIS** (Arc do BEI corrompido; writer não capturado) |
| P0.3 — park observável (não `loop{hlt}`) | **VERIFIED** (runtime: `park core (fail-closed)`) |
| P0.1 — launcher serial | **VERIFIED** (2 runs, 130–371 KB) |
| P1.1/P1.2 | VERIFIED (s451b, commit `388d3068`) |

## 6. Residual

- `bpe=ABSENT` no `a2_proof` (`bpe_absent fallback=char99`, `prompt_len=2`) — o BPE não
  carrega no caminho loader-only; follow-up.
- Serial lossy sob SMP (linhas entrelaçadas `[NESTED]`/`[SILENCE]` no meio) — evidência
  canônica é o BOOT.LOG, não o serial (lição s411).
- Nada commitado nesta sessão até o commit pós-task.
