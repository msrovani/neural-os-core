# SESSION_479 — #PF storm em tick=1: recursão mútua infinita affect (não era heap) + matriz de 8 modelos + HW emulado máximo

**Data:** 2026-10-17 · **Estado:** v1.9.99-s479 TEST · **Branch:** `main`
**Contexto:** s478 fechou um `#PF storm` atribuído ao "range do SLAB" em `try_fault_in_heap`. Este
sessão provou que **a causa estava errada**: era uma recursão mútua infinita em Rust puro, que
transbordava a stack do scheduler do BSP. Emenda: matriz de 8 modelos por boot + cobertura de HW emulado.

---

## 1. Correção do diagnóstico do s478 (honestidade, ADR-0113)

O s478 mae `0x0000_4000_0007_ffe4` como "endereço 60 TiB abaixo do heap" euhnou que o range
`in_slab` nunca dispara. **As duas leituras estavam erradas**: `0x4000_0000_0000` = **64 TiB**
(`4 × 16^11`), então `cr2 − HEAP_START = 0x7ffe4` está **dentro** da janela de 512 KiB, e o
`in_slab` disparava. Além disso, `sp = HEAP_START + 0x80000` = `LARGE_HEAP_START` exato.

Duas leituras erradas seguidas do mesmo campo → regra nova: **conferir aritmética hex→TiB antes
de declarar um range** (ver lição L2).

O que o s478 acertou por acidente: o gate `heap_headroom_critical()` e a máscara RSVD
(`(err & 0xB) == 0x3`) são corretos e foram mantidos.

## 2. Causa-raiz real (VERIFIED por disassembly)

**Recursão mútua infinita de 2 funções**, introduzida em `92d25242` ("s468d: typed AFFECT channel"):

```
crates/hermes/src/globals.rs:66        sync_affect_snapshot  ->  affect_channel::sync_affect_snapshot_both
crates/hermes/src/affect_channel.rs:120 sync_affect_snapshot_both -> globals::sync_affect_snapshot   (volta)
```

Prova (objdump, casando byte-a-byte com o `ip_bytes` do log):
```
ffffffff80216bc0 <hermes::affect_channel::sync_affect_snapshot_both>:
  pushq %rbx ; subq $0x30,%rsp
  movl (%rdi),%ebx / 0x4 / 0x8 / 0xc / 0x10 / 0x14 / 0x18 / 0x1c   ; AffectVector = 8 f32
  movl %ebx,0xc(%rsp) ...                                         ; AffectSnapshot em rsp+0xc
  callq 0xffffffff80240d60 <hermes::globals::sync_affect_snapshot>
  leaq  0xc(%rsp),%rdi
  callq publish_affect_snapshot                                    ; INALCANÇÁVEL
```

Cadeia completa: `ProceedWithBudget` → ciclo → stack de 8 MiB do scheduler do BSP
(`main.rs:5518-5520` + `5549-5555`, `asm!("mov rsp, {sp}")`) transborda → escreve 8 bytes por nível
descendo → `#PF` no limite do heap VA → 3 faults iguais → `park core (fail-closed)` → BSP morre
em `tick=1`. Os valores `0x3f4ccccd` (0.8f) e `0x3e99999a` (0.3f) na stack são **locals spilled**
de `AffectSnapshot::NEUTRAL`, não ponteiros flutuantes (hipótese refutada por disassembly).

**Por que o gate de heap do s476 não podia funcionar:** o caminho da recursão **não aloca nada**
antes de recursar — o único `Vec::with_capacity` está em `publish_affect_snapshot`, *depois* do
`call` recursivo. No tick 1 com heap novo, `heap_headroom_critical()` é `false`: o gate nunca
dispara.

## 3. Fix (4 arquivos, +34/−52)

| Arquivo | Mudança |
|---|---|
| `crates/hermes/src/globals.rs:65` | faz o trabalho real: monta o snapshot, publica no canal tipado, **escreve `AFFECT_SNAPSHOT`** (global que o compositor lia congelado em NEUTRAL desde s468d) |
| `crates/hermes/src/affect_channel.rs:107` | vira shim de 1 linha → **quebra o ciclo**; eliminate-se a construção duplicada do snapshot |
| `crates/k_nano/src/allocator.rs:1783` | mantém `(err & 0xB) == 0x3` (RSVD não é curável) |
| `crates/k_nano/src/allocator.rs:1811` | **reverte** `start = HEAP_START` do s478 → volta a `LARGE_HEAP_START`; cobrir o buraco de 512 KiB foi medido como péssimo (o sintoma descia 8 bytes e esmagava 512 KiB do heap VA em silêncio) |
| `crates/k_nano/src/smp/mod.rs:518` | `AP_STACK` de `"trace"` → `"info"`: alavanca de evidência (SESSION_299: se responde a uma pergunta diagnóstica, tem que ser visível) |

## 4. Prova de runtime

- **Mesh 4×1G/4c sem modelo** (`tools/run-mesh-empty.ps1`): `tick` = **3201 / 3073 / 2944 / 2784**
  (antes: `1` em todos), `mesh TX MCH node=2 bytes=185 ok=true`, zero `#PF storm`.
- Os 8 `#PF` restantes por boot são os **demos Ring3 P6 intencionais** (`ip=0x000070000030000a`,
  `#UD 0x700000300000`), contidos por design (`ring3.rs:182`). O contador por regex é
  falso-positivo — nunca use "PF>0" como veredito.
- `cargo check --release` 0 erros; `cargo build --release` 0 erros.

## 5. Matriz de 8 modelos (nó único × 7G, 240 s cada)

O modelo vive em `0x100000000` (4 GiB) e o kernel só varre esse endereço → cada nó precisa de
>4 GiB de RAM. 4 nós não cabem num host de 15,7 GB; por isso a matriz rodou em nó único
(`tools/run-mesh-models-batch.ps1 -Single -Mem 7G`).

| Modelo | Veredito | Evidência |
|---|---|---|
| falcon3-3b | **saudável + carregado** | `LLM LOADED ... size=1012764KB`, v6 `h=3072 L=22 q_dim=3072 vocab=131072` (shapes canônicos do Falcon3-3B) |
| bitnet2a-4b | **saudável + carregado** | v7 `h=2560 L=30 tok_len=5390533`, `size=778810KB` (aceite do s476); 1 AP perdido (`#PF storm cr2=0x13b8`, endereço baixo órfão — classe diferente) |
| piper-pt-br | **saudável + carregado** | `Piper v3 → parse 61342 KB`, TTS gerou **46070 amostras** |
| hw-expert-v6 | saudável (via FAT, não loader) | `HWEXPRT4.BIN 259KB via ATA` |
| bpe-vocab | saudável (via FAT, não loader) | `BPB1 LOADED vocab_n=32002` |
| router-bitnet | saudável, blob **não** carregado | sem marcador de router |
| stt | saudável, blob **não** carregado | `skip FAT STT.BIN PIO no hypervisor` |
| **bge-m3** | **TRAVADO (PANIC)** | `[PANIC] talc.rs:836 new_heap must contain old_heap`, 6/9 phases |

**Achado transversal (aberto):** 5 de 8 logs imprimem o **mesmo tamanho fictício** `model
590680KB` (577 MB) — nenhum arquivo do repo tem esse tamanho. O probe lê header v6 em blobs
v3/v4 (Piper=3, BGE/STT=4) e fabrica um LLM fantasma. Contido em 4; **fatal no bge-m3** porque
`resize_heap` entra em panic no talc em vez de recusar fechado. Mesma classe do s476: validar o
tamanho declarado contra o comprimento real do blob **antes** de qualquer `resize_heap`.

## 6. Cobertura de HW emulado (`tools/run-qemu-fullhw.ps1`)

10 devices num boot só, todos enumerando: **video** (fb Limine 1280×800 bpp=4 + `virtio-gpu-pci`),
**NVMe**, **virtio-blk**, **AHCI/SATA**, **e1000** (RX ring 64 buffers), **xHCI** (1b36:000d, 8
portas, `os_owned`), **usb-storage/MSC** (`MSC desc cfg=1 ep_in=0x81 ep_out=0x2 mps=1024`,
instalado via k_hal), **HDA** (`sd0_capture sd1_playback`, PARTIAL/widgets residual), **ATA/IDE**.

Achados: (a) **duas NICs quebram o net bootstrap** — com e1000 + virtio-net o boot parou em
6/9 PHASE, 0 ticks, preso no smoke de DNS, com MAC default `02:00:00:00:00:fe` em vez da
configurada; o kernel não desambigua qual NIC usar (virou opt-in `-VirtioNet`). (b) o
`probe MSC` genérico dá `pass=0` mas a política `k_hal::usb::hub_msc` (hub→TT/rota) acerta 3
portas — rota por hub funciona, varredura cega não. (c) `usbstick.raw` esparsero não tem FAT32 →
BOOT.LOG fell back (honesto: só a enumeração USB foi exercitada aqui).

## 7. Ferramentas novas (4 scripts, ASCII-only, com dump de argv por ciclo)

`tools/run-mesh-model.ps1` (1..4 nós, 1 modelo, `-Single`), `tools/run-mesh-models-batch.ps1`
(matriz), `tools/run-mesh-empty.ps1` (baseline), `tools/run-qemu-fullhw.ps1` (HW máximo).
Todos gravam `.args.txt` por ciclo (lição SESSION_465) e são null-safe nas métricas.

## 8. Pendentes

1. **bge-m3** — panic `new_heap must contain old_heap` + probe de tamanho fantasma `590680KB`.
2. **mesh peers=0** — TX funciona, nenhum peer descoberto (hub L2 não recebe pacotes).
3. **Duas NICs** — net bootstrap não desambigua.
4. Metal: USB BOOT.LOG (foto PRE/POST s467), F1 1h 8c/8GB, #PF AP #3 (`cr2=0x13b8`).