# ADR-0103: k_nano microkernel modular -- Fase 1 (crates) sem virar process-OS

**Data:** 2026-09-05  
**Status:** Proposed  
**Lifecycle (INDEX):** `fazendo`  
**IDEA:** **#549**  
**Sprint / enquadramento:** Fase 1 = continuidade de **ADR-0075** (emagrecer) + **ADR-0042** (aneis de funcao); fatias no backlog **ADR-0100** quando houver T-* novos. Fase 2 = **pos** ADR-0102 Onda 6 aceite em **HW**.  
**Evidencia:** comparacao Redox-kernel v2.x (`/c/DEV/redox-kernel/`) vs `crates/k_nano` (tree 2026-09-05, ~150 `.rs`); SESSION_313/314 (USB hub->MSC em `k_hal`); dedupe `multi_user`/`hnsw` (2026-09-05).  

**Nao substitui:** ADR-0041 (DeviceCap/HalOffer), ADR-0042 (K3CHJ), ADR-0075 (emagrecer bin), ADR-0077 / **0102** (Ring3 sandbox B/C), ADR-0081 (mesh R0), ADR-0088 (AIOS-first), ADR-0092 (boot log).  
**Estende:** 0075 (corte passa do bin -> **dentro** de `k_nano`) + 0041/0042 (destino = anel certo, nao "userspace POSIX").  
**Corrige:** rascunho de planeamento 2026-09-05 que (a) tratava Redox como blueprint de privilegio imediato, (b) inventava duplicatas falsas (`multi_user`???`percpu`, `hnsw` sob `smp/`, `core_pinning` em k_ai), (c) propunha mover PCI/ACPI/SMP/mesh/FAT early para fora de R0 sem criterio de boot.

---

## 0. Decisao (ler primeiro)

1. **Fase 1 = modularidade de crates (aneis de funcao), nao CPL=3.** Codigo sai de `k_nano` quando a verdade do produto ja e (ou deve ser) `k_hal` / `k_ai` / `cortex` / `hermes` / `jarbas`, e `k_nano` fica com **primitivos R0** + hooks. Boot permanece monolitico em privilegio (quase tudo CPL=0), como hoje.
2. **Fase 2 = schemes / drivers em CPL=3 -- proposta, nao compromisso.** So apos **ADR-0102** sectionaceite HW (`register_native_ring` + Onda 6). Ate la, **nao** criar trait `KernelScheme` Redox-like no kernel "para alinhar". EventBus + HalOffer + CapGate sao o IPC canonico do AIOS.
3. **Redox e referencia de *tamanho* e *separacao*, nao de *ontologia*.** Redox empurra FS/NIC/block para schemes userspace porque o modelo e process-OS. Neural e AIOS-first (ADR-0088): agentes nativos, Hermes, Cortex, mesh, EventBus **permanecem CPL=0** (ADR-0102 section0.1). Nao mapear "scheme = agent".
4. **Invariantes R0 que NAO migram na Fase 1:** GDT/IDT/TSS, paging/`AddressSpace`, allocator/PMM, serial/slog, TSC/time/RTC, SIMD enable, APIC/SMP wake+percpu+IST, `int 0x90`/Ring3 plumbing, limine handoff, **mesh transporte** (ADR-0081), primitivos xHCI/ATA/NVMe **MMIO/rings** (politica de enum pode ir a `k_hal`).
5. **Boot path early (BOOT.LOG / USB-MSC / FAT read)** nao pode "sumir" de R0 num unico PR. Padrao: politica em `k_hal` + hook registado cedo (`init_h1`); I/O primitivo em `k_nano`. SESSION_314 ja e o prototipo (hub->MSC).
6. **Um slice = um destino + `cargo check --release` 0 erros.** QEMU = test/dev (nao gate de merge). Aceite de persistencia = **HW** (`E:\BOOT.LOG`). Nao deletar 5 subsistemas no mesmo commit.
7. **LOC e heuristica.** Alvo Fase 1 "~25k em k_nano" e direcao, nao KPI. Medir por *modulos sem callers R0* e *facades honestas*, nao por LOC-count obsessivo.

---

## 1. Analise do rascunho Redox vs k_nano

### 1.1 O que o rascunho acertou

| Insight | Porque vale |
|---|---|
| `k_nano` concentra drivers, FS leitores, net, IA auxiliar, installer | Verdade: crate R0 virou "monolito de conveniencia" pos-migracao K3CHJ incompleta |
| Drivers de dispositivo pertencem a **k_hal** (ja existe) | Alinha ADR-0041 H1 / DeviceCap; USB hub-MSC ja cortou nessa linha |
| Cognitivo / multi-user / HNSW / SGDB nao sao microkernel | Alinha ADR-0042 aneis + 0102 (Theseus: types no confiavel) |
| Fase 2 schemes so depois de Ring3 | Coerente com 0102 Onda 6 |
| Cortar duplicatas antes de moves grandes | Disciplina 0075 / `check_duplication.py` |

### 1.2 Erros factuais (corrigidos)

| Claim no rascunho | Realidade (2026-09-05) | Acao ADR |
|---|---|---|
| `k_ai/multi_user` = copia de `k_nano/smp/percpu` | **Falsos tipos.** `multi_user` = `UserManager`; `percpu` = `PerCpu` SMP. Duplicata real: `k_nano::multi_user` ??? `k_ai::multi_user` | Canonico **k_ai**; k_nano removido; bin `pub use k_ai` ??? feito |
| `hnsw` em `k_nano/smp/hnsw.rs` | Path **inexistente**. Era `k_nano/src/hnsw.rs` + `cortex` + facade bin | Canonico **cortex**; k_nano removido; hermes/`vfs` -> `cortex::hnsw` ??? feito |
| `core_pinning` duplicado em k_ai | **So** `k_nano::core_pinning` (+ politica em hermes) | Sem delete; manter R0 |
| "Syscall estruturado = zero" | Existe `int 0x90`, CapGate, mailbox Ring3 (0102); falta dispatcher *tipo Redox*, nao "nada" | Nao inventar `syscall/process.rs` na Fase 1 |
| Mover **todo** PCI/ACPI/SMP para k_hal | Boot/`init_platform_sync` e wake AP sao R0; k_hal ja faz DeviceTree em cima de `k_nano::pci` | k_hal = politica/descoberta; k_nano = cfg space + MADT + SIPI |
| Mover **mesh** para k_ai | ADR-0081: transporte P2P vive em **k_nano R0** | Mesh **fica** R0; so telemetria/UI em jarbas/hermes |
| Allocator 822 LOC "excessivo so por GDT/TSS" | GDT/TSS **nao** estao no allocator; heap bump+TALC+budget e produto AIOS (SESSION_249b/287/290) | Simplificar so dead paths; **nao** copiar linked_list 416 LOC do Redox as cegas |
| FS inteiro -> k_hal | k_hal nao e FS; leitores FAT/exFAT early = boot; NeuralFS/SGDB = k_nano/hermes/k_ai conforme contrato | Destinos por *papel*, nao "tudo k_hal" |

### 1.3 Conflito com ADR-0102 (obrigatorio)

ADR-0102 section0: sandbox CPL=3 **so** para blob nativo B/C; agentes/drivers/IRQ ficam CPL=0; **rejeita** Job/Handle/process-OS.

Portanto:

- **Fase 1** deste ADR = *emagrecer k_nano* (funcao), compativel com 0102.
- **Fase 2 "drivers = KernelScheme em CPL=3"** = **opcional e subordinada** a 0102. Se schemes existirem, sao **drivers isolados**, nao a frota de agentes. HalOffer **nao** "vira IPC Redox" -- pode *ganhar* um transporte Cap-gated; nao se apaga o modelo EventBus.

Comparar LOC com Redox (~33k kernel sem drivers) e **inspiracao de magreza**, nao prova de que neural deve deletar 37k LOC amanha.

---

## 2. Modelo alvo (Fase 1)

```text
k_nano (R0)          k_hal (R1)              k_ai / cortex / hermes / jarbas
???????????????????????????????????????        ????????????????????????????????????????????????        ????????????????????????????????????????????????????????????????????????????????????????????????
GDT IDT paging       DeviceTree/HalOffer     Trust, SelfHeal, Agency
PMM allocator        USB politica hub->MSC    multi_user, SGDB FE, HNSW
APIC SMP percpu      NIC/GPU/HDA BE MMIO     installer FE, recipes
serial slog time     UnlockDAG / CapGate     compositor / TTS (jarbas)
xhci/ata/nvme MMIO   fat_assets / discovery  mesh *consumo* EventBus
mesh *transporte*    virtio transport        
ring3 plumbing       
hooks registaveis ????????? register_*_bringup
```

**Regra de corte:** um modulo sai de `k_nano` quando:

1. Existe (ou cria-se) API na crate do anel certo; e  
2. `k_nano` so exporta primitivo + `register_*` / `pub use` fino; e  
3. Boot early ainda chama um caminho que **nao** assume userspace; e  
4. Nenhum segundo espelho fica no bin (`check_duplication.py`).

---

## 3. Mapa de destino corrigido (Fase 1)

| Bloco | Fica em k_nano | Migra / canonico | Notas |
|---|---|---|---|
| GDT, IDT, paging, ring3, serial, slog, tsc, time, rtc, simd | **KEEP** | -- | Microkernel minimo |
| allocator, slab, memory, numa | **KEEP** (podar dead) | -- | Nao Redox-malloc |
| smp/, apic, core_pinning, cpufreq | **KEEP** (nucleo) | telemetria -> hermes/k_ai | Nao mover wake/SIPI |
| scheduler / runqueue | **KEEP** (simplificar depois) | agent-core ja agenda fleet | Nao copiar Redox context switch na F1 |
| pci cfg + acpi MADT parse | **KEEP** primitivo | inventario/receitas -> k_hal/k_ai | H1 ja usa pci |
| xhci rings/TRB + usb_msc BOT | **KEEP** primitivo + BOT | politica enum hub -> **k_hal::usb** | SESSION_314 |
| ata/ahci/nvme/virtio_blk MMIO | **KEEP** ate slice estavel | Cap/status -> k_hal storage port | Early FAT pode depender |
| fat32/exfat **read** early | **KEEP** ate path BOOT.LOG estavel | assets/recipes -> k_hal `fat_assets` | Depois: facade |
| neural_fs, tickv, sgdb core | avaliar | FE/API -> hermes/k_ai; blob I/O pode ficar R0 | ADR-0063/0091 |
| ext2/btrfs/ntfs readers | ??? DELETADO | 0 callers (commit `6e789960`) | FASE A completa |
| firewall, verify, rollback, self_check | ??? DELETADO | stubs/mortos (commit `6e789960`) | FASE A completa |
| e1000/rtl/virtio_net MMIO | **KEEP** curto prazo | oferta net -> k_hal; smoltcp bridge -> hermes | Net gate = e1000 |
| mesh/p2p transporte | **KEEP** (0081) | dashboard -> jarbas | -- |
| crypto/tpm verify | minimo R0 p/ boot trust | politicas/contas -> k_ai/hermes | Nao esvaziar verify_trusted |
| multi_user | -- | **k_ai** ??? | Feito 2026-09-05 |
| hnsw | -- | **cortex** ??? | Feito 2026-09-05 |
| display/audio UI | stubs | **jarbas** / k_hal HDA BE | 0075 ja cortou audio no bin |
| installer_agent / sys_installer | thin | **hermes** + ADR-0086 | -- |
| limine, boot_handoff, boot_logger | **KEEP** R0 | scores/UI -> 0092/jarbas | -- |

---

## 4. Ordem de execucao (Fase 1)

Ordem **obrigatoria** (cada item = slice mergeavel):

| # | Slice | Aceite |
|---|---|---|
| **S0** | Dedupe comprovado (`multi_user`, `hnsw`, facades) | `cargo check --release` ??? (2026-09-05) |
| **S1** | USB host BE completo em `k_hal::usb` + early `probe_and_install` | Serial: hook R1 + hub/root MSC; **HW:** `E:\BOOT.LOG` deixa de ser placeholder |
| **S2** | Proximos drivers "politica em k_hal, MMIO em k_nano" (padrao S1) -- NIC status/offer, nao mover smoltcp | Boot net gate intacto |
| **S3** | Leitores FS nao-boot (ntfs/btrfs/ext2 **read-only** orfaos) -> crate ou delete se 0 callers | Sem regressao FAT early |
| **S4** | Storage cognitivo (tickv FE, rollback UI) -> k_ai/hermes; flash/backend R0 fino | NSGDB persiste |
| **S5** | Podar `lib.rs` exports mortos + `check_duplication.py` limpo | Codemap atualizado |
| **S6** | (Opcional) esqueleto `k_nano/src/{arch,memory,scheduler}/` **sem** mover logica ainda | So layout; zero comportamento |

**Proibido na Fase 1:** apagar `fat32`/`usb_msc`/`mesh`/`smp` "porque Redox nao tem"; criar `scheme/` populado; multi-arch arm64/riscv.

---

## 5. Fase 2 (Proposed / gated)

**Pre-condicao dura:** ADR-0102 aceite metal + `isolation_ring_available()` refletindo HW gate.

**Escopo maximo permitido:**

1. Um **driver** nao-critico (ex.: serio/debug) como blob B/C Cap-gated -- prova de vida.  
2. Trait interno tipo "scheme" **so se** mapear 1:1 para CapGate + EventBus topics existentes.  
3. FS userspace **nao** e default; boot stick continua R0/`k_hal`.

**Fora de escopo Fase 2 neste ADR:** reimplementar Redox `SchemeList` 65k slots; mover Hermes/Cortex para CPL=3; substituir mesh Ed25519 por scheme.

Se Fase 2 nunca compensar o custo, este ADR **permanece valido so com Fase 1** (magreza modular).

---

## 6. Relacao com backlog e emagrecer

| ADR | Papel |
|---|---|
| **0075** | Emagrecer **bin** -> crates |
| **0103** | Emagrecer **k_nano** -> anel certo (Fase 1) |
| **0041/0042** | Destinos semanticos (HalOffer, K3CHJ) |
| **0102** | Teto de privilegio (nao virar process-OS) |
| **0100** | Se surgirem T-* novos para S2-S5, numerar sob Onda adequada (provavel Onda 1/5 HW), sem sprint paralelo orfao |

---

## 7. Riscos

| Risco | Mitigacao |
|---|---|
| Cortar FS/USB cedo -> BOOT.LOG morto no Alienware | S1 primeiro; aceite = ficheiro em `E:` |
| Dependencia ciclica k_nano???k_hal | So hooks `register_*` R0; k_hal chama k_nano, nunca o inverso em deps Cargo |
| "Facade" que esconde segundo monolito | `tools/check_duplication.py` no CI mental de cada slice |
| LOC vanity | Preferir remocao de **modulos sem caller** |
| Fase 2 cedo demais | Gate explicito section5 + 0102 |

---

## 8. Criterios de aceite

### Fase 1 (este ADR vira `Accepted` parcial quando)

- [x] S0 dedupe multi_user/hnsw  
- [~] S1: hub->MSC em k_hal **wired** (SESSION_314); evidencia HW BOOT.LOG **AWAITING_OPERATOR** (`HW_FLASH_s314.md`)  
- [x] S0.5: Dead code deletion -- 13 modulos mortos removidos (~1800 LOC); k_nano 88->87 modulos, ~27k->25.5k LOC (commit `6e789960`)
- [x] S0.6: NIC drivers -> k_hal::net (fasade pattern, 4 drivers: e1000/rtl8139/i225/virtio_net); k_nano mantem nic_globals statics (commit `030e4bf0`)
- [ ] ???2 slices S2-S5 merged com boot check verde -- **FREEZE** ate 2 boots com log  
- [x] INDEX lifecycle -> `fazendo`; `completa` (Fase 1) so apos S1 PASS metal; Fase 2 pode ficar `pesquisa`  

### Fase 2

- [ ] Pre-condicao 0102 HW  
- [ ] ADR adenda ou ADR nova se o desenho schemes divergir deste section5  

---

## 9. Checks,???????????? no PR (rodar antes de marcar slice feito)

| N?? | Check | Comando / evidencia |
|---|---|---|
| C0 | Build release limpo | `cargo clean -p neural-kernel && cargo check --release` -> 0 erros |
| C1 | Sem novo excesso modular | `tools/check_duplication.py` -> exit 0; lista de duplicatas vazia (ou intencional + justificada em SESSION) |
| C2 | Sem shadowing de singleton | `grep -Rn "lazy_static!.*SKILL_REGISTRY\|register_builtin_skills" crates/` -> unica fonte canonica; sem duplicatas bin???crate |
| C3 | Hooks k_hal registados antes da fase do boot dependente | Boot log deve conter `H1 register_*_bringup` antes do stage onde o hook e consumido; se nao, o slice regressa |
| C4 | Fat early nao degradado | Apos cada slice, boot QEMU lean e TCG: `E0 boot log` e FAT32 mount seguem em `k_nano`/hooks, nao partida dead |
| C5 | Aceite metal S1 (USB/FS) | BOOT.LOG real em `E:\BOOT.LOG` (ou LBA correto via USB-MSC/AHCI); ver `docs/evidence/HW_FLASH_s314.md` |

---

## 10. TODO e referencias ao Redox (para revisao em merge)

### 10.1 Referencias ao tree Redox (clone em `/c/DEV/redox-kernel`, 2026-09-05)

- **Commit-base do comparativo:** consultar `git -C /c/DEV/redox-kernel log --oneline -1` no momento do PR; snapshot usado = commit printado no SESSION_313/314. O Redox e referencia, nao upstream -- nao fazer `git merge`/
- **locais de leitura (nao alterar):** `src/scheme/` (trait `KernelScheme`), `src/scheme/sys/{block,context,fdstat,irq,proc,memory,pipe,proc,serio,sys,debug,event,memory,proc,serio,sys,time}.rs`, `src/scheme/user.rs`, `src/start.rs`, `src/arch/x86_64/interrupt/syscall.rs`, `src/arch/x86_64/interrupt/vectors.rs`, `src/context/`, `src/syscalls/mod.rs`, `src/syscall/debug.rs`, `src/allocator/linked_list.rs`, `src/allocator/mod.rs`, `src/scheme/irq.rs` -- invariantes quando nao ha `.git` disponivel: leitura somente.
- **Como enquadrar no PR:** cada slice pode incluir um SESSION_NNN.md com um paragrafo "o que o Redox faz, o que o AIOS faz, o que migrou" -- evita repensar depois; deve ser breve e citar os arquivos Redox acima, nao copia-los.

### 10.2 TODO de Fase 1

| ID | O que | Owner | Blocker |
|---|---|---|---|
| TODO-0103-0 | S0.5: Dead code deletion (verify, disk_power, io_scheduler, fw_cfg, ext2, btrfs, ntfs, user_accounts, luks_open, self_check, suspend_resume, rollback, firewall) -- ??? FEITO commit `6e789960` | maint. | -- |
| TODO-0103-1 | S1: wiring completa em k_hal::usb + early hook registado no boot (SESSION_314) | squada (USB) | aceite metal BOOT.LOG |
| TODO-0103-2 | S0.6: NIC drivers -> k_hal::net (fasade pattern, 4 drivers) | maint. | ??? FEITO commit `030e4bf0` |
| TODO-0103-3 | S0.7: FS primitivos -> k_hal::fat_assets canonico (partition scanning API + compat re-exports) | maint. | ??? FEITO commit `4ea9e956` |
| TODO-0103-2 | S2: pick one NIC/device com politica em k_hal + stub MMIO em k_nano; provar com boot + log | squada (net) | manter e1000 como canonico (no stdout QEMU) |
| TODO-0103-3 | S0.7: FS primitivos -> k_hal::fat_assets canonico (partition scanning API + compat re-exports) | maint. | [X] FEITO commit `4ea9e956` |
| TODO-0103-4 | S0.8: storage_bus -> k_hal::storage_port (facade: device_count, publish_all_storage, register_probe, bus_report) | maint. | [X] FEITO commit `f206e637` |
| TODO-0103-5 | S3: triage FS readers orfaos (ntfs/btrfs/ext2) -- mover para crate k_ai ou deletar se 0 callers | squada (fs) | contagem de callers reais |
| TODO-0103-4 | S4: storage cognitivo (tickv FE/UI) -> libs proprias; backend raw em k_nano fino | squada (sgdb) | NSGDB persiste OK |
| TODO-0103-5 | S5: podar lib.rs morto + codemap; rodar check_duplication.py como gate | maint. | sem regredir boot |
TODO-0103-7 | FASE F: seguranca -> k_ai (firewall deletado FASE A; usb_trust manter em k_nano) | maint. | FEITO analise -- usb_trust dominio USB hardware policy, nao IA trust |
TODO-0103-8 | FASE G: agentes -> hermes (installer_agent 275 LOC + sys_installer 344 LOC) | squada (installer) | NAO MIGRADO -- dependencias k_nano (hw_profiler, sys_installer, neural_fs) + Agent trait |
TODO-0103-9 | FASE H: display/audio -> jarbas | jarbas (display/audio) | NAO MIGRADO -- display.rs (boot_ckpt) usado por k_hal::usb e jarbas::display::agent; audio/ ja existe como jarbas::audio, k_nano audio/ = HDA stub 996 LOC |
TODO-0103-9 | FASE H: display/audio -> jarbas (display.rs 50 LOC + audio/) | jarbas (display/audio) | NAO MIGRADO -- display.rs usado por k_hal boot_ckpt; audio/ ja que existe em jarbas
TODO-0103-7 | FASE F: seguranca -> k_ai (firewall deletado FASE A; usb_trust manter em k_nano) | maint. | FEITO analise -- usb_trust dominio USB hardware policy, nao IA trust |
TODO-0103-8 | FASE G: agentes -> hermes (installer_agent 275 LOC + sys_installer 344 LOC) | squada (installer) | NAO MIGRADO -- dependencias k_nano (hw_profiler, sys_installer, neural_fs) + Agent trait |
TODO-0103-7 | FASE F: segurana -> k_ai (firewall deletado FASE A; usb_trust manter em k_nano) | maint. | FEITO analise -- usb_trust dominio USB hardware policy, nao IA trust |
| TODO-0103-6 | S6: esqueleto `k_nano/src/{arch,memory,scheduler}/` sem mover logica -- so documentar layout proposto | maint. | opcional; nao obrigatorio |

### 10.3 TODO de Fase 2 (gated, nao iniciar antes 0102 HW)

| ID | O que | Owner | Pre-cond |
|---|---|---|---|
| TODO-0103-10 | 1 blob driver B/C Cap-gated rodando em CPL=3 (ex.: serio/debug) | squada (ring3) | 0102 aceite metal |
| TODO-0103-11 | Trait `KernelScheme`-like **so** se mapear 1:1 para CapGate + EventBus topic existente | maint. | nao reinvent scheme list |
| TODO-0103-12 | FS userspace nao como padrao; boot continua R0/k_hal | squada (fs) | gate de privilegio |

---

## 11. Referencias diretas (para o revisor ler junto com o PR)

- `docs/architecture/0102-ring3-isolation-migration-cpl03.md` -- teto de privilegio (Fase 2 gated).  
- `docs/architecture/INDEX.md` -- lifecycle + tabela de substituic??es/coflitos.  
- `docs/architecture/0075-emagrecer-neural-kernel.md` -- emagrecer bin -> crates (previo a este).  
- `docs/architecture/0041-k2chj-capability-rings.md` e `docs/architecture/0042-k2chj-adequacao-boot.md` -- destinos semanticos K3CHJ.  
- `docs/architecture/0100-k3chj-backlog-custo-anel.md` -- backlog de T-* quando surgem novos slices, numerar sob Onda adequada; nao abrir sprint paralelo orfao.  
- `tools/check_duplication.py` -- guarda de segundo monolito (calla no CI mental de cada slice).  
- Redox local: `/c/DEV/redox-kernel/` -- snapshot de commit no SESSION; read-only para comparativo.

## 9. Conclusao

O rascunho Redox acerta o **diagnostico** (k_nano inchado) e erra o **remedio imediato** (process-OS + mapa de duplicatas falso + mover SMP/mesh/FAT wholesale).

**ADR-0103 decide:** magreza por **anel K3CHJ** (Fase 1), prototipo USB ja no caminho certo; privilegio estilo Redox schemes **so** depois do sandbox Ring3 real -- e mesmo assim sem trair ADR-0102/0088.

**Proximo passo operacional:** fechar **S1** (coleta `BOOT.LOG` no metal), nao redesenhar `arch/` ainda.
