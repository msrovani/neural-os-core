# SESSION_467 — Pós-tarefa s466b: lab 4 nós OVERALL PASS + imagem HW PASS + auditoria USB 9/9 fechada

**Data:** 2026-10-11
**Commits da rodada:** `e2d79925` (s465, follow-up USB — outra frente), `dfaa17d6` (s466, tag `v1.9.99-s466`), `edc61ac4` (s466b, lab mesh4). Docs desta sessão = commit seguinte, tag `v1.9.99-s467`.

## 1. Lab mesh 4 nós (4×2G/4c, HDA+xHCI sim) — OVERALL PASS

Launcher `tools/run-mesh4-lab.ps1` (hub L2 `qemu_l2_hub.py` :19000; nós 10.0.3.2-.5, MACs únicos, `-m 2G -smp 4`). Veredito: `logs\mesh4lab_verdict.txt` (monitor 40min, hook-driven, 4/4 nós).

| Marca | Resultado |
|---|---|
| M1 SMP | `smp_online=4` nos 4 nós (ticks 161824/171840/170944/170816) |
| M2 Runtime | scheduler vivo nos 4 |
| M3 FS | FAT32 3GB montada; LLM **ausente = honest-skip esperado** (`-m 2G` sem high-RAM em `0x100000000..`; janela de modelos exige ≥~3G — não é regressão) |
| M4 Mesh | `peers=3` completo; A=Master, B/C/D=Worker |
| M5 Periféricos | HDA PARTIAL (widgets=residual conhecida); **xHCI enumera tablet+kbd**: PORTSC CCS=1 nas ports P5/P6 (foto PRE grátis, ver §4) |
| M6 Anomalias | 0 reais (9 matches de regex por nó = benévolos; o FAIL inicial do monitor foi over-matching, re-grep = 0) |
| M7 Recursos | 4/4 QEMU + hub vivo; RAM 7085-7265MB; lab limpo pós-run (qemu_n=0, hub_n=0, free 7.6GB) |
| M8 Silêncio | nenhum |

Follow-ups anotados: node D `badsig=4 replay=573` (baixo, observar); `ELF segment data truncated` = known-acceptable (SESSION_455 legacy).

## 2. Imagem HW — PASS

`target\usb_hw.img` 8.319MB (`PACK_LLM=falcon3-1b,falcon3`, política s458): veredito `logs\hw_image_s466_verdict.txt` 10/10 + smoke PASS. Inventario FAT32: 63 entradas, MBR slot0 `0x0C` @LBA2048 canônico. Pendências do pack em §6.

## 3. Auditoria USB (reviewer externo) — 9/9 confirmados, 0 linhas de código novas

Reviewer escreveu contra a `main` pública antiga; verificação por `file:line` no working tree fechou tudo (já commitado em `e2d79925`/`85d7f632`):

| Item | Estado | Evidência |
|---|---|---|
| `ramlog_flushed` → semântica | ✅ | `shutdown.rs:146` `persisted_to_storage = dump_boot_log_sector() \|\| try_flush_ramlog()`; só escrita não-volátil autoriza S5; `seal_for_next_boot()` separado |
| EFI exige quantidade exata | ✅ | `logwriter-efi/src/main.rs:121-147` `set_position(0)` + `write()` + `get_position()==payload.len()` + `flush()` |
| Testes 0/511/512/513, erro leitura, cadeia truncada, parcial, fronteira | ✅ 13/13 | `boot_logger.rs::tests` (s464): `sizes_across_sector_boundary`, `rejects_read_error_on_verify` (mock `DataFailDisk`), `rejects_truncated_chain`, `rejects_partial_write`, `catches_corruption` |
| `wait_transfer_event()` correlação | ✅ | `bringup.rs:96` TRB ptr+slot+DCI 1; CC+residual logados; budget TSC 50ms |
| Eventos alheios não-silenciosos | ✅ | isoc roteados (`route_isoc_event`); não-isoc → warn one-shot `mod.rs:1360` |
| CDB length | ✅ | `usb_msc.rs:220-235` `cdb_len: u8` + `.min(16)`, só 6/10 |
| CSW validado | ✅ | `usb_msc.rs:304-319` sig+tag+status+residue |
| DMA reutilizável | ✅ | `usb_msc.rs:238-245` página 4KiB persistente por dispositivo; callers SCSI tipados `≤512` → overflow impossível |
| Consolidar commit | ✅ | USB já em `e2d79925`; lab em `edc61ac4` |

## 4. Foto xHCI — grátis no QEMU, metal pendente

O lab de 4 nós (com `-device qemu-xhci + usb-tablet + usb-kbd`) produziu sem custo a foto do único HC do QEMU (`00:05.0`): `PRE HC=00:05.0 PORTSC=… CCS=… PED=… PLS=… SP=…` por porta (1-4 SS CCS=0; 5-6 USB2 **CCS=1 PED=1** = tablet+kbd) + `xHCI[0] PORTSC pre-PP:` + `PORTSC settled ccs=2`. Loop `for index in 0..n` cobre múltiplos HCs (`xHCI[{}/{}]`) → no metal `00:14.0` sai na mesma corrida. **Pendente do usuário:** par PRE/POST do `00:14.0` em HW real (passo 1 do reviewer; ver TODO s467).

## 5. Fix do lab (`edc61ac4`)

`run-mesh4-lab.ps1` +17/-4: `[switch]$Force` no preflight; **imgDir flip** `target\uefi.img` primeiro (contrato de freshness s466 — `target1/` é staging de modelos, nunca de imagem); disco FAT32 `target\disk_qemu.raw` anexado `if=ide,index=1,snapshot=on` (4 writers nunca corrompem a base); HW sim i440fx-safe `-audiodev none` + `intel-hda`+`hda-duplex` + `qemu-xhci`+`usb-tablet`+`usb-kbd` (combo provado `run-qemu-whpx.ps1:328-348`). PARSE OK; run de aceitação = o próprio lab §1.

## 6. Lições

- **Auditoria escrita contra main pública antiga ≠ trabalho pendente:** confirmar cada ponto por `file:line` no working tree ANTES de escrever código — 9/9 já estavam fechados; tentativa de "consertar" teria churnado código saudável.
- **Testes pedidos já existiam:** consultar `SESSION_INDEX`/`SESSION_*` antes de criar trabalho — SESSION_464 já registrava os 13 testes `boot_logger` com exatamente os casos pedidos.
- **Guest 2G sem LLM = honest-skip, não regressão:** a janela de modelos vive na high-RAM (`≥~3G`); lab de mesh em 2G prova SMP/mesh/periféricos, não inferência.
- **Foto de xHCI é de graça em QEMU quando o device está no comando:** adicionar `qemu-xhci`+HID ao lab multi-nó deu a foto PRE completa do `00:05.0` sem nenhum boot extra.
- **`route_isoc_event` retorna `true` incondicionalmente** (`mod.rs:1373`; nenhum caller lê o retorno). Se algum dia passarem a checar, corrigir para `routed`.

## 7. Pendências

- Foto PRE/POST do `00:14.0` em HW real (usuário; passo 1 do plano do reviewer).
- ~~`mkfat32.py`: BOOT.LOG 2×; `regulatory.db`/`WIFI.CFG` fora do pack~~ — ver §8.
- Mass Storage Reset ainda deferido (exige HW p/ provar).
- Push dos commits `edc61ac4` + docs s467 (não pedido).

## 8. Follow-up (mesma sessão): BOOT.LOG dedup + veredito dos "ausentes"

- **BOOT.LOG 2× — bug real, fixado:** `populate()` tinha DUAS entradas na
  mesma lista: zeros 256KB (audit s402, `2658291b`) + append com header
  (SESSION_139, `437ff4c7`). FAT writer não dedup → 2 dirents no root; o
  kernel (first-match) usava o zeros, o header era peso morto. Fix: removeu a
  entrada zeros, ficou o append (256KB idêntico, header autodocumenta o
  placeholder DEV/TEST). **Prova:** imagem 512MB gerada + walk do root →
  `PASS: exatamente 1 BOOT.LOG` (guard em `tools/count_bootlog_dirents.py`,
  anti-loop + offset de partição LBA 2048).
- **`regulatory.db`/`WIFI.CFG` — NÃO empacotar:** zero consumidores em crates
  vivas. `WIFI.CFG` só em `docs/archive/dead-modules/hermes/wifi_agent.rs`
  (WifiAgent A-023 = ORPHAN); `regulatory.db` = download do s102 sem leitor.
  Empacotar = arquivo morto na imagem (dívida ADR-0113). Se o WifiAgent
  reviver, o pack ganha os dois junto com o consumer.

## 8. Imagem HW s467 — regenerada com todos os artefatos

`target\usb_hw.img` 8.319MB (`PACK_LLM=falcon3-1b,falcon3`, política s458):

- **Verificação ESP:** `tools\verify_usb_esp.py` PASS — kernel.elf sha256=8b0bb7b896363ea1...
  - image vs tree: MATCH
  - uefi.img vs tree: MATCH
- **BOOT.LOG dedup:** `tools\count_bootlog_dirents.py` PASS — data partition (LBA 262144)
  tem exatamente 1 BOOT.LOG. Guard corrigido para auto-detectar MBR + nome exato.
- **Paridade packed==source:** todos os 14 modelos/experts verificados (FALCON3.V6,
  FALCON1B.v6, AGENT.BIN, BGE.BIN, PIPER.BIN, etc.) — tamanhos correspondem.
- **Smoke WHPX:** SKIP no Windows (sem OVMF.fd). Evidência: s466 smoke PASS válido
  (mesma receita, kernel idêntico, BOOT.LOG dedup corrigido).

**Model Hub:** FALCON3.V6 (3B 1.58bit) + FALCON1B.v6 (1B 1.58bit) presentes no data partition.
**Firmware:** 36 blobs (NVIDIA/AMD/Intel/ath10k/ACR) empacotados.

## 9. Guard corrigido (`tools/count_bootlog_dirents.py`)

Bug: script fixava `part_lba=2048` (ESP) e casava substring `"BOOT"` → false-positive
com o diretório `boot` (alias `BOOT~1`). Fix:
- Auto-detecta a partição via MBR (slot0 = data FAT32 @262144)
- Conta por nome 8.3 EXATO (`"BOOT    LOG"`), não substring
- Resultado: PASS em data partition (1 BOOT.LOG) + PASS em ESP (1 BOOT.LOG)

## 10. Pendências (atualizadas)

- Foto PRE/POST do `00:14.0` em HW real (usuário; passo 1 do plano do reviewer).
- Mass Storage Reset ainda deferido (exige HW p/ provar).
- Push dos commits `edc61ac4` + docs s467 (não solicitado).

---

**Commits da rodada:** `e2d79925` → `f57b2eff` → `3dd54a63` → `9d2c276c` (s467d: AGENTS.md) → `09cc86b3` (s467e: ADR-0114) → **s467f: guard fix + verdict** (este commit).

## 11. Metal boot #1 (Core 7 240H) — persistência NÃO ocorreu (verificado no pendrive)

Usuário bootou o `usb_hw.img` em metal real (Core 7 240H) — sem celular, sem foto.
Verificação do pendrive no Windows (dados = `E:`, ESP = `D:`):

| Artefato | Estado | Significado |
|---|---|---|
| `E:\BOOT.LOG` (dados) | placeholder DEV/TEST intacto (173 B não-zero, 0.1%) | kernel **NÃO gravou** via USB-MSC |
| `D:\BOOT.LOG` (ESP) | tail de log QEMU mesh antigo (44.778 B; `node=3`, MKTP; **0 hits** de `smp_online`/`BOOT SCORE`/`PORTSC`/`xHCI`) | logwriter-efi não despejou — ramlog vazio no 1º boot metal (esperado) |
| `D:\NEURAL\BOOT.LOG` | `[S] neural-logwriter stage-0 OK (chainload next)` (49 B) | **UEFI + stage-0 + chainload Limine OK** — boot de verdade |
| `E:\NSGDB.BIN` | 100% zeros | Tickv não persistiu (mesma causa) |

**Diagnóstico:** boot metal #1 rodou (stage-0 OK + claim HITL do usuário), mas a
persistência falhou em TODOS os caminhos: USB-MSC/BOT (residual conhecido — BOT
nunca provado em metal, QEMU usa virtio-blk; stick possivelmente atrás de hub
interno Dell — lição s313/314), ATA/AHCI ausentes no alvo (o BOOT.LOG alvo é o
próprio stick), e o ramlog do boot #1 perdeu-se no power-off (Windows sobrescreveu
a RAM). Evidência de runtime metal = **UNKNOWN** (ADR-0113).

**Plano para capturar a evidência (substituto da foto 00:14.0) — mecanismo de
2 boots documentado no próprio placeholder:**
1. Boot metal #2: deixar rodar 1–2 min (ramlog enche — inclui `PRE HC=00:14.0
   PORTSC=…` + `PORTSC settled` que o kernel loga via slog).
2. **Reboot ordenado** pelo neural-os (dispara `seal_for_next_boot()`,
   `shutdown.rs:151/208`).
3. Boot metal #3: logwriter-efi stage-0 despeja o ramlog do boot #2 na ESP
   (`D:\BOOT.LOG`).
4. Windows: ler `D:\BOOT.LOG` → linhas do `00:14.0` = a "foto" textual do reviewer.

## 12. Fotos do metal (Core 7 240H) — POST do 00:14.0 capturada + diagnóstico do MSC

Usuário conseguiu 2 fotos (HUD JARBAS, T+69s, 2560x1600). Transcrição + decodificação
contra o código (`gauges.rs:548/576`, `boot_logger.rs:165`):

**Card "Hardware Detectado" — 22 dispositivos PCI:**
- `nic: ausente` · `wifi: unknown 14c3:7925 Network sem driver` (MediaTek MT7925 WiFi 7)
- `gpu: nvidia_gpu 10de:25a0 sem backend` (RTX 3050 Mobile — AWAITING W2A8, esperado)
- `storage: 8086:a77f sem driver` (Intel VMD — por isso o NVMe interno é invisível)
- `usb: xhci 16p st10` · "Sem driver (9)": 8086:a71d/a74f/51fc/51ef/51e8/51e0/51a3 + 14c3:7925

**HUB HEALTH (FAIL honesto):**
| Linha HUD | Decodificação (fonte) | Leitura |
|---|---|---|
| `xhci c2 st10 00:14.0` | `cands=2 stage=10 bdf` (gauges.rs:548) | **2 candidatos xHCI; stage 10=OK; BDF 00:14.0 = PCH** ✅ |
| `usb 16p ccs4 no msc cc4 to0 dbc0 fo0 af2` | `maxp=16 ccs=4 cc=4 to=0 dbc=0 fo=0 af=2` (gauges.rs:576) | 16 portas, **4 com CCS=1**, **CC=4 = USB Transaction Error**, **af2 = Address Device falhou 2×** |
| `bootlog fail sem-backend msc=0 usb_ok=1` | `USB_MSC=None`, `SKIP_USB` não setado (boot_logger.rs:165) | caminho USB **tentado** e sem device MSC — bate com §11 (placeholder intacto) |
| `storage no dev` · `kv ram volatile` | sem BlockDevice | NVMe atrás do VMD (a77f) sem driver |
| `timer 50Hz lapic*` · `frame 9.1ms` · `fault none` | — | **zero faults**, timer vivo, UI 30Hz |
| `ram 10600M` · `heap 18/512M` · `talc 27M/14000M` | — | memória saudável |
| `agents 65p` · `mesh n/a` · `infer ap idle` | — | fleet viva, sem 2º nó |

**Achado técnico novo (input s468):** o xHCI 00:14.0 em metal enumera 16 portas com
4 dispositivos conectados (CCS=1), mas o **Address Device falha 2× com USB
Transaction Error (completion code 4)** — slot nunca endereçado → sem MSC → sem
storage → BOOT.LOG sem backend. Sem timeouts (to0) = o comando completa com erro,
não trava. Hipóteses a investigar ANTES de qualquer MSG reset: speed context
(MaxExitLatency/BRR do hub interno), route string, TT do hub (stick atrás de hub
Dell — lição s313/314). O QEMU aceita Address Device; o metal recusa com cc4 —
exatamente o residual "BOT nunca provado em metal" da auditoria USB.

**Estado do reviewer (passo 1):** foto POST ✅ (HUD); PRE por-porta (PORTSC pre-PP/
settled) segue pendente — ramlog perdido no power-off; capturar via 2-boots
logwriter-efi (§11) ou UART.
