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

## 7. Caca do writer — tentativas (s452, continuação)

**gdb watchpoint: não confiável sob WHPX.** 3 variantes: `break` aos 25 s = texto do
kernel não mapeado; `hbreak` = WHPX não expõe debug registers ("too many"); `break` aos
60 s (software) = inserido, mas ao hit o guest **resetou** (PC→`0xfffcc3d4`) e gdb não
leu `BEI_STATE` (hot-path + 4 CPUs + text virando RO). Harness em
`tools/watch_bei_write.ps1` (pronto, mas não usável aqui).

**Watchpoint NO KERNEL (robusto, sem gdb):** `k_nano::interrupts::arm_write_watchpoint`
(DR0/DR7 = `0xD0001`, 8 bytes, write) + `arm_stored_watchpoint` (APs em `ap_entry`),
armado no `main.rs` após `init_bei` no campo `affect_regulator` (`hermes::bei::
affect_regulator_addr`). O `#DB` cai em `debug_handler` → `fatal_exception`, que loga
o `ip=` do writer antes de parkar. **Armado e funcionando** (log: `watchpoint DR0
@0xffffffff811c9c00 armed`) — mas a corrupção é **rara**: 6 runs limpos seguidos.

**2ª vítima (disco fresco, `logs/boot_whpx_20261004_182537.txt`):** crash **durante o
prefill do `a2_proof`** (`chunk=0/10`) — `#PF cr2=0x18 err=0x0` (READ de ponteiro
corrompido) → storm → park. RIP `core::sync::atomic::atomic_load::<usize>`; stack
`MpmcQueue<CellMessage>::len` ← `cortex::cellular::CellNetwork::round_robin`. Ou seja:
**outra estrutura BEI (early heap)** — mesmo padrão (ponteiro corrompido, cr2 pequeno).

**Conclusão:** o **prefill do LLM (a2_proof) corrompe o heap**; as vítimas são as
estruturas BEI alocadas no **início do heap** (`BeiState` @ `HEAP_BUFFER`, `CellNetwork`
logo depois). O `#PF` é sempre a vítima (lição s438) — o writer segue não capturado
(a 2ª vítima não está em `BeiState+0x38`, então o watchpoint atual não a pega).

**Próximo:** watchpoint num alvo mais largo (o próprio **início do heap**/redzone do
allocator, ou o `CellNetwork` MPMC) para pegar o writer; ou revisar o path de alocação
do prefill (`apply_one_layer`/tensor alloc) por overflow. `ponytail:` o diag (DR0 +
`corrupt diag`) é temporário — reverter quando o culpado for capturado.

**Repro com diag (20:55, `logs/boot_whpx_20261004_205521.txt`):** `a2_proof done
id=2` → `done id=2 len=1 in_flight=0` → `[BEI] corrupt diag self=0xffffffff811c9c08
… cn=0x18 pc=0x830 …`. Offsets dos Arc (8 B, ordem do struct, `ar`@+0x38): `cmq`@+0x00,
`bm`@+0x08, `el`@+0x10, **`cn`@+0x18**, **`pc`@+0x20**, `dm`@+0x28, `ms`@+0x30,
`ar`@+0x38, `es`@+0x40, `ct`@+0x48. Vítimas desta vez = `cn`/`pc` (+0x18/+0x20);
`el`/`dm` também suspeitos (`0xffffffffce62e3b8/e3e8`, fora do range `811b…/811c…`).
O watchpoint (DR0..3 = +0x00/+0x18/+0x20/+0x38) está armado, mas a corrupção é **rara**
(~1 em 11 runs) — ainda não capturada. **Teardown suspeito:** `finish_job`
(`logits_recycle`, `heap_aios::verify_job`) roda imediatamente após `done`.


