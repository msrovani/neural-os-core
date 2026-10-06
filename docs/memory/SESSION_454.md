# SESSION_454 — Mesh 2 instancias (6c/6GB) + triage da janela de 30 min

**Data:** 2026-10-05 · **Lane:** D5 (FREEBU: memoria/harness/QEMU/runtime) ·
**Fórum:** `FREEBU-0149` (blocker), `FREEBU-0150` (triage) ·
**Zero `.rs` tocado** (o `main.rs` esta sob outra thread).

## 0. Pedido

Rodar **2 instancias do QEMU, 6c / 6 GB, todo o HW permitido, video na tela,
rede mesh**, e **monitorar os logs por 30 min** a procura de problemas.

## 1. VERIFIED — o launcher de mesh nunca entregava o guest ao kernel

As duas instancias subiram, ficaram vivas, e **nunca passaram do OVMF**:

```
!!!! X64 Exception Type - 0D(#GP - General Protection)  CPU Apic ID - 00000001 !!!!
!!!! Find image based on IP(0x834EEE) .../PlatformPei.dll (ImageBase=...834840)
```

Serial parado em 6.795 B, um dump de registradores por CPU (1, 3, 4), sem uma
linha do Limine. Sintoma enganoso: o QEMU esta no ar, entao parece "boot lento".

### Bisect de 5 variantes (1c/1G, WHPX, `-net none`)

| variante | `-cpu` | pflash VARS | resultado |
|---|---|---|---|
| v1 | `max` | nao | **#GP loop**, 1.359 B, sem kernel |
| v2 | `max` | sim | **#GP loop** |
| v3 | `Haswell` | sim | **BOOTOU** (179.924 B) |
| v4 | `Haswell` | nao | **BOOTOU**, tick 6.514 |
| v5 | `Haswell` | sim | **BOOTOU**, tick 6.619 |

**Causa = `-cpu max`** (APX/MPX que o OVMF do QEMU 11.1.0 nao trata).
**O pflash VARS ausente nao era a causa** — v4 boota sem ele. Isso importa: a
hipotese "falta o segundo pflash" era plausivel (o launcher funcional
`run-qemu-whpx.ps1` passa code+vars) e estava errada; so o bisect separou.

**Fix (minimo, 39+/17- em `run-qemu-p2p-mesh.ps1`):** `-cpu Haswell` sob WHPX,
`max` so em TCG — mesma politica de `run-qemu-whpx.ps1:190`.
`non_ascii=0`, `SYNTAX_OK`.

### Segunda correcao no mesmo arquivo

A lista de artefatos era `BITNET2B.BIN` + experts em `0x1292_0000`/`0x1294_0000`.
Dois defeitos: (a) esses arquivos **nao existem** em `target/`, entao o modo
modelos era ligado e silenciosamente vazio; (b) se existissem, cairiam **dentro**
da janela do LLM (`0x100000000..0x13DD9BAB0`) → QEMU aborta `regions overlap`.
Trocada pela receita **provada no lab da s453**: LLM `target/FALCON3.BIN`
(1.037.071.016 B) pinado em `0x100000000`, extras empilhados depois dele
(`ROUTER.BITNET`, `PIPER_PT_BR.BIN`, `bpe_vocab.bin`, `hw_expert_v6.bitnet`),
folga de 1 MB, alinhamento de 1 MB.

## 2. O run de verdade

`run-qemu-p2p-mesh.ps1 -Cores 6 -Mem 6 -Accel whpx -Instance Both`
→ A = PID 18156 (Cloverleaf), B = PID 18600 (Hal9000), `-display gtk`
(video na tela), `-vga std`, e1000 em socket P2P (A escuta `127.0.0.1:12345`,
B conecta), artefatos + `uefi.img` IDE + `disk_qemu.raw`.

Monitor: `tools/mesh_watch_30min.py 30` (read-only; amostra a cada 30 s →
`logs/mesh_watch.csv`, `logs/mesh_watch_status.txt`, `..._summary.txt`).
Acrescentei a coluna `qemu_n` e normalizei `free_gb` (o PowerShell devolve
`"8,02"` com virgula decimal, que no CSV vira string ambigua).

### Linha do tempo (medida)

```
00:22:40  boot comeca, 45.780 B
00:24:11  A tick 3675 · B tick 3838 · peers=1 nos dois
00:24:42  A tick 6018 · B tick 6019   <-- ULTIMO crescimento dos dois logs
00:24:42..00:33:21   14 amostras com logs BYTE-IDENTICOS
00:33:51  qemu_n=0 (morta por fora) · free_gb 1.8 -> 8.97
```

O congelamento **precede** em ~9 min o kill externo: o `Stop-Process` de outra
thread nao causou o stall (e o que a coluna `qemu_n`existiu para separar).

## 3. VERIFIED — o que FUNCIONOU

- **Mesh convergiu**: `MESH_HEALTH peers=1` nos dois; `node_id=2` e `node_id=3`;
  MACs `52:54:00:aa:00:01` / `bb:00:02`; IP estatico `10.0.3.2` / `10.0.3.3`
  lido do netmode flag (armadilha do s411 — ids unicos, sem colisao).
- **Skill marketplace**: `broadcast skill 'micropython' v1.0 from node 3 sent=true`
  (e mais 6). **CRDT**: `publish v=29 sent=true`.
- **Seguranca**: `unsigned=0 badsig=0 replay=0` nas duas.
- **matmul mesh na forma 64x64**: A responde `matmul resposta node=3 sent=true
  bytes=16448`. Compute distribuido **funciona** nessa forma.
- `ram=7168MB`, `frag_budget=64000` impressos pelo proprio kernel.

## 4. Triagem ranqueada (o que deu errado)

### P1 — OOM → NULL+0x18 → `#PF storm` → park (VERIFIED)

A (`crash_mesh_a_T6018.txt`):
```
[PF_DBG] heap-fail cr2=0x...0018 kphys=0x96c97000
[PF_DBG] diag ... alloc_f=0x0 ...
[EXC] #PF ip=0xffffffff80a9a4f8 ...
[EXC] #PF storm ip=0xffffffff80a9a4f8 streak=3+ park core (fail-closed)
```
B: `O[OOM/TALC] sem memoria Tier 1. size=2359307 align=1 agente=infer_worker`.

Os dois no **mesmo tick** (6018/6019) — mesma carga deterministica nos dois guests.

**Simbolizacao:** o `kernel.elf` da arvore **DIVERGE** da imagem rodada
(`target/limine-esp-tree` sha `6f18e21e` vs ESP de `target1/uefi.img` sha
`463f7490`) — simbolizar com a arvore teria dado atribuicao falsa. Extrai o ELF
de dentro da ESP (FAT32, reusando `tools/verify_usb_esp.py`) e refiz:

| endereco | simbolo |
|---|---|
| `0xffffffff80a9a4f8` | `core::sync::atomic::atomic_load::<usize>` |
| `0xffffffff808b940c` | `<Atomic<bool>>::compare_exchange` |
| `0xffffffff808bef26` | `<RawVecInner>::grow_exact` |
| `0xffffffff808cf92e` | `cortex::cellular::CellNetwork::sleep_cycle` |

Ou seja: **grow de `Vec` no CellNetwork com o heap esgotado**, e o ponteiro NULL
lido como base de um atomico (`cr2=0x18` = NULL + offset de campo).
A linha `TTS Piper: "lose" (5060 samples)` imediatamente antes e o **ultimo log**,
nao o culpado (licao s415-417).

### P2 — o park fail-closed vira pause-spin sob WHPX (VERIFIED)

Com os logs congelados, o CPU do host continuava subindo: **+18,3 s de CPU por
8 s de parede**, medido em 3 amostras. Nao e travamento de guest — e o park
fail-closed virando spin sob MicrosoftHv (licao s432). Custo: 1 core perdido para
sempre, mais a pressao de pagina do host (`free_gb` 8,62 → 1,29).

### P3 — matmul mesh e IMPOSSIVEL acima de 64.000 B, e a mensagem culpa o lugar errado (VERIFIED)

Payloads observados: `884755`, `2457619`, `7176211`, `11356000` B.
Teto real = `FRAG_MAX_PARTS(64) x FRAG_MAX_CHUNK(1000) = 64.000` B, porque o
bitmask de fragmentos e `[u8; 8]` (64 bits) —
`crates/k_nano/src/net/udp_broadcast.rs:460-462`. O RX faz
`total_len > max_legit_len -> continue`: **drop silencioso**.

O slog diz `payload=N > frag budget (DEGRADED)` e
`frag_reassembly_budget_bytes()` (RAM) tambem satura em 64.000 — **coincidencia**
que faz o leitor culpar a RAM. Mexer so na tabela de RAM **nao muda nada**.
Para 1 MB real exige mudar o formato do wire. (IDEA #654)

### P4 — o gate vem DEPOIS do serialize (VERIFIED, com efeito HYPOTHESIS)

`cortex::compute::mesh_matmul_worker` chama `serialize_mesh_request` (que copia
`w.packed_data` + `x.data` para um `Vec` novo) e **so entao** testa
`can_afford_frag`. B recusou 75 matmuls, cada um alocando e descartando ate
11,3 MB. Que esse churn|esgote o Tier 1 e **HYPOTHESIS** — falsificador: rodar
com o mesh desligado e ver se o `OOM/TALC` some.

### P5 — dual-truth de peer (OBSERVED)

`MESH_HEALTH peers=1` (UI) e, no mesmo instante,
`peer failure sem peer online — skip circuit breaker` (compute; `online_nodes()`
vazio). O circuit breaker nunca abre e B insiste nas mesmas 75 recusas sem backoff.

### P6/P7 — conhecidos (OBSERVED)

`[ELF] [fail] load fail: ELF: segment data truncated` (1/boot, nos dois) ·
`[EXC]` x8 pre-Runtime (self-test P6/P7, conhecido) · `MOUSE stream F4 TIMEOUT`
(usb-hid) · **host**: 2x6 GB em 15,7 GB — o commit limit (31,2 GB) comportou,
a RAM fisica nao; T-047 se mantem.

## 5. Honestidade do veredito

`VERIFIED`: o fix do `-cpu`, a convergencia do mesh, o OOM+park, o teto de
64.000 B, a ordem serialize->gate, a dupla verdade de peer, o kill externo.
`HYPOTHESIS`: o churn do P4 como causa do OOM.
`UNKNOWN`: por que a alocacao falha **nesse** workload e nao no lab da s453
(6 GB / 8c, tick 26.206 limpo) — 6c x 8c e mesh on/off sao as duas variaveis
candidatas, nenhuma isolada.
`BLOCKED`: nada implementado no kernel (crates sob outra thread; `main.rs`
explicitamente intocado).
Nada marcado done/accepted: o run morreu em T+6018 e nao ha segunda janela.

## 6. Artefatos

- `run-qemu-p2p-mesh.ps1` (39+/17-) — `-cpu` + receita de artefatos.
- `tools/mesh_watch_30min.py` (novo) + coluna `qemu_n` + `free_gb` normalizado.
- `logs/crash_mesh_a_T6018.txt` / `logs/crash_mesh_b_T6019.txt` (preservados).
- `logs/mesh_watch.csv` / `_status.txt` / `_summary.txt`, `logs/mesh_launch_2.txt`.
- Rascunho em `target/meshbisect/`: `bisect.ps1`, `bisect2.ps1`,
  `extract_kernel.py`, `forum_s454.py` (gitignored).