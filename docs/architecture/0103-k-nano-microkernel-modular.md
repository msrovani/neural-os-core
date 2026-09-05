# ADR-0103: k_nano microkernel modular — Fase 1 (crates) sem virar process-OS

**Data:** 2026-09-05  
**Status:** Proposed  
**Lifecycle (INDEX):** `fazendo`  
**IDEA:** **#549**  
**Sprint / enquadramento:** Fase 1 = continuidade de **ADR-0075** (emagreçer) + **ADR-0042** (anéis de função); fatias no backlog **ADR-0100** quando houver T-* novos. Fase 2 = **pós** ADR-0102 Onda 6 aceite em **HW**.  
**Evidência:** comparação Redox-kernel v2.x (`/c/DEV/redox-kernel/`) vs `crates/k_nano` (tree 2026-09-05, ~150 `.rs`); SESSION_313/314 (USB hub→MSC em `k_hal`); dedupe `multi_user`/`hnsw` (2026-09-05).  

**Não substitui:** ADR-0041 (DeviceCap/HalOffer), ADR-0042 (K³CHJ), ADR-0075 (emagreçer bin), ADR-0077 / **0102** (Ring3 sandbox B/C), ADR-0081 (mesh R0), ADR-0088 (AIOS-first), ADR-0092 (boot log).  
**Estende:** 0075 (corte passa do bin → **dentro** de `k_nano`) + 0041/0042 (destino = anel certo, não “userspace POSIX”).  
**Corrige:** rascunho de planeamento 2026-09-05 que (a) tratava Redox como blueprint de privilégio imediato, (b) inventava duplicatas falsas (`multi_user`≠`percpu`, `hnsw` sob `smp/`, `core_pinning` em k_ai), (c) propunha mover PCI/ACPI/SMP/mesh/FAT early para fora de R0 sem critério de boot.

---

## 0. Decisão (ler primeiro)

1. **Fase 1 = modularidade de crates (anéis de função), não CPL=3.** Código sai de `k_nano` quando a verdade do produto já é (ou deve ser) `k_hal` / `k_ai` / `cortex` / `hermes` / `jarbas`, e `k_nano` fica com **primitivos R0** + hooks. Boot permanece monolítico em privilégio (quase tudo CPL=0), como hoje.
2. **Fase 2 = schemes / drivers em CPL=3 — proposta, não compromisso.** Só após **ADR-0102** §aceite HW (`register_native_ring` + Onda 6). Até lá, **não** criar trait `KernelScheme` Redox-like no kernel “para alinhar”. EventBus + HalOffer + CapGate são o IPC canônico do AIOS.
3. **Redox é referência de *tamanho* e *separação*, não de *ontologia*.** Redox empurra FS/NIC/block para schemes userspace porque o modelo é process-OS. Neural é AIOS-first (ADR-0088): agentes nativos, Hermes, Cortex, mesh, EventBus **permanecem CPL=0** (ADR-0102 §0.1). Não mapear “scheme = agent”.
4. **Invariantes R0 que NÃO migram na Fase 1:** GDT/IDT/TSS, paging/`AddressSpace`, allocator/PMM, serial/slog, TSC/time/RTC, SIMD enable, APIC/SMP wake+percpu+IST, `int 0x90`/Ring3 plumbing, limine handoff, **mesh transporte** (ADR-0081), primitivos xHCI/ATA/NVMe **MMIO/rings** (política de enum pode ir a `k_hal`).
5. **Boot path early (BOOT.LOG / USB-MSC / FAT read)** não pode “sumir” de R0 num único PR. Padrão: política em `k_hal` + hook registado cedo (`init_h1`); I/O primitivo em `k_nano`. SESSION_314 já é o protótipo (hub→MSC).
6. **Um slice = um destino + `cargo check --release` 0 erros.** QEMU = test/dev (não gate de merge). Aceite de persistência = **HW** (`E:\BOOT.LOG`). Não deletar 5 subsistemas no mesmo commit.
7. **LOC é heurística.** Alvo Fase 1 “~25k em k_nano” é direção, não KPI. Medir por *módulos sem callers R0* e *facades honestas*, não por LOC-count obsessivo.

---

## 1. Análise do rascunho Redox vs k_nano

### 1.1 O que o rascunho acertou

| Insight | Porquê vale |
|---|---|
| `k_nano` concentra drivers, FS leitores, net, IA auxiliar, installer | Verdade: crate R0 virou “monólito de conveniência” pós-migração K³CHJ incompleta |
| Drivers de dispositivo pertencem a **k_hal** (já existe) | Alinha ADR-0041 H1 / DeviceCap; USB hub-MSC já cortou nessa linha |
| Cognitivo / multi-user / HNSW / SGDB não são microkernel | Alinha ADR-0042 anéis + 0102 (Theseus: types no confiável) |
| Fase 2 schemes só depois de Ring3 | Coerente com 0102 Onda 6 |
| Cortar duplicatas antes de moves grandes | Disciplina 0075 / `check_duplication.py` |

### 1.2 Erros factuais (corrigidos)

| Claim no rascunho | Realidade (2026-09-05) | Ação ADR |
|---|---|---|
| `k_ai/multi_user` = cópia de `k_nano/smp/percpu` | **Falsos tipos.** `multi_user` = `UserManager`; `percpu` = `PerCpu` SMP. Duplicata real: `k_nano::multi_user` ↔ `k_ai::multi_user` | Canónico **k_ai**; k_nano removido; bin `pub use k_ai` ✅ feito |
| `hnsw` em `k_nano/smp/hnsw.rs` | Path **inexistente**. Era `k_nano/src/hnsw.rs` + `cortex` + facade bin | Canónico **cortex**; k_nano removido; hermes/`vfs` → `cortex::hnsw` ✅ feito |
| `core_pinning` duplicado em k_ai | **Só** `k_nano::core_pinning` (+ política em hermes) | Sem delete; manter R0 |
| “Syscall estruturado = zero” | Existe `int 0x90`, CapGate, mailbox Ring3 (0102); falta dispatcher *tipo Redox*, não “nada” | Não inventar `syscall/process.rs` na Fase 1 |
| Mover **todo** PCI/ACPI/SMP para k_hal | Boot/`init_platform_sync` e wake AP são R0; k_hal já faz DeviceTree em cima de `k_nano::pci` | k_hal = política/descoberta; k_nano = cfg space + MADT + SIPI |
| Mover **mesh** para k_ai | ADR-0081: transporte P2P vive em **k_nano R0** | Mesh **fica** R0; só telemetria/UI em jarbas/hermes |
| Allocator 822 LOC “excessivo só por GDT/TSS” | GDT/TSS **não** estão no allocator; heap bump+TALC+budget é produto AIOS (SESSION_249b/287/290) | Simplificar só dead paths; **não** copiar linked_list 416 LOC do Redox às cegas |
| FS inteiro → k_hal | k_hal não é FS; leitores FAT/exFAT early = boot; NeuralFS/SGDB = k_nano/hermes/k_ai conforme contrato | Destinos por *papel*, não “tudo k_hal” |

### 1.3 Conflito com ADR-0102 (obrigatório)

ADR-0102 §0: sandbox CPL=3 **só** para blob nativo B/C; agentes/drivers/IRQ ficam CPL=0; **rejeita** Job/Handle/process-OS.

Portanto:

- **Fase 1** deste ADR = *emagreçer k_nano* (função), compatível com 0102.
- **Fase 2 “drivers = KernelScheme em CPL=3”** = **opcional e subordinada** a 0102. Se schemes existirem, são **drivers isolados**, não a frota de agentes. HalOffer **não** “vira IPC Redox” — pode *ganhar* um transporte Cap-gated; não se apaga o modelo EventBus.

Comparar LOC com Redox (~33k kernel sem drivers) é **inspiração de magreza**, não prova de que neural deve deletar 37k LOC amanhã.

---

## 2. Modelo alvo (Fase 1)

```text
k_nano (R0)          k_hal (R1)              k_ai / cortex / hermes / jarbas
─────────────        ────────────────        ────────────────────────────────
GDT IDT paging       DeviceTree/HalOffer     Trust, SelfHeal, Agency
PMM allocator        USB política hub→MSC    multi_user, SGDB FE, HNSW
APIC SMP percpu      NIC/GPU/HDA BE MMIO     installer FE, recipes
serial slog time     UnlockDAG / CapGate     compositor / TTS (jarbas)
xhci/ata/nvme MMIO   fat_assets / discovery  mesh *consumo* EventBus
mesh *transporte*    virtio transport        
ring3 plumbing       
hooks registáveis ←── register_*_bringup
```

**Regra de corte:** um módulo sai de `k_nano` quando:

1. Existe (ou cria-se) API na crate do anel certo; e  
2. `k_nano` só exporta primitivo + `register_*` / `pub use` fino; e  
3. Boot early ainda chama um caminho que **não** assume userspace; e  
4. Nenhum segundo espelho fica no bin (`check_duplication.py`).

---

## 3. Mapa de destino corrigido (Fase 1)

| Bloco | Fica em k_nano | Migra / canónico | Notas |
|---|---|---|---|
| GDT, IDT, paging, ring3, serial, slog, tsc, time, rtc, simd | **KEEP** | — | Microkernel mínimo |
| allocator, slab, memory, numa | **KEEP** (podar dead) | — | Não Redox-malloc |
| smp/, apic, core_pinning, cpufreq | **KEEP** (núcleo) | telemetria → hermes/k_ai | Não mover wake/SIPI |
| scheduler / runqueue | **KEEP** (simplificar depois) | agent-core já agenda fleet | Não copiar Redox context switch na F1 |
| pci cfg + acpi MADT parse | **KEEP** primitivo | inventário/receitas → k_hal/k_ai | H1 já usa pci |
| xhci rings/TRB + usb_msc BOT | **KEEP** primitivo + BOT | política enum hub → **k_hal::usb** | SESSION_314 |
| ata/ahci/nvme/virtio_blk MMIO | **KEEP** até slice estável | Cap/status → k_hal storage port | Early FAT pode depender |
| fat32/exfat **read** early | **KEEP** até path BOOT.LOG estável | assets/recipes → k_hal `fat_assets` | Depois: facade |
| neural_fs, tickv, sgdb core | avaliar | FE/API → hermes/k_ai; blob I/O pode ficar R0 | ADR-0063/0091 |
| e1000/rtl/virtio_net MMIO | **KEEP** curto prazo | oferta net → k_hal; smoltcp bridge → hermes | Net gate = e1000 |
| mesh/p2p transporte | **KEEP** (0081) | dashboard → jarbas | — |
| crypto/tpm verify | mínimo R0 p/ boot trust | políticas/contas → k_ai/hermes | Não esvaziar verify_trusted |
| multi_user | — | **k_ai** ✅ | Feito 2026-09-05 |
| hnsw | — | **cortex** ✅ | Feito 2026-09-05 |
| display/audio UI | stubs | **jarbas** / k_hal HDA BE | 0075 já cortou audio no bin |
| installer_agent / sys_installer | thin | **hermes** + ADR-0086 | — |
| limine, boot_handoff, boot_logger | **KEEP** R0 | scores/UI → 0092/jarbas | — |

---

## 4. Ordem de execução (Fase 1)

Ordem **obrigatória** (cada item = slice mergeável):

| # | Slice | Aceite |
|---|---|---|
| **S0** | Dedupe comprovado (`multi_user`, `hnsw`, facades) | `cargo check --release` ✅ (2026-09-05) |
| **S1** | USB host BE completo em `k_hal::usb` + early `probe_and_install` | Serial: hook R1 + hub/root MSC; **HW:** `E:\BOOT.LOG` deixa de ser placeholder |
| **S2** | Próximos drivers “política em k_hal, MMIO em k_nano” (padrão S1) — NIC status/offer, não mover smoltcp | Boot net gate intacto |
| **S3** | Leitores FS não-boot (ntfs/btrfs/ext2 **read-only** órfãos) → crate ou delete se 0 callers | Sem regressão FAT early |
| **S4** | Storage cognitivo (tickv FE, rollback UI) → k_ai/hermes; flash/backend R0 fino | NSGDB persiste |
| **S5** | Podar `lib.rs` exports mortos + `check_duplication.py` limpo | Codemap atualizado |
| **S6** | (Opcional) esqueleto `k_nano/src/{arch,memory,scheduler}/` **sem** mover lógica ainda | Só layout; zero comportamento |

**Proibido na Fase 1:** apagar `fat32`/`usb_msc`/`mesh`/`smp` “porque Redox não tem”; criar `scheme/` populado; multi-arch arm64/riscv.

---

## 5. Fase 2 (Proposed / gated)

**Pré-condição dura:** ADR-0102 aceite metal + `isolation_ring_available()` refletindo HW gate.

**Escopo máximo permitido:**

1. Um **driver** não-crítico (ex.: serio/debug) como blob B/C Cap-gated — prova de vida.  
2. Trait interno tipo “scheme” **só se** mapear 1:1 para CapGate + EventBus topics existentes.  
3. FS userspace **não** é default; boot stick continua R0/`k_hal`.

**Fora de escopo Fase 2 neste ADR:** reimplementar Redox `SchemeList` 65k slots; mover Hermes/Cortex para CPL=3; substituir mesh Ed25519 por scheme.

Se Fase 2 nunca compensar o custo, este ADR **permanece válido só com Fase 1** (magreza modular).

---

## 6. Relação com backlog e emagreçer

| ADR | Papel |
|---|---|
| **0075** | Emagreçer **bin** → crates |
| **0103** | Emagreçer **k_nano** → anel certo (Fase 1) |
| **0041/0042** | Destinos semânticos (HalOffer, K³CHJ) |
| **0102** | Teto de privilégio (não virar process-OS) |
| **0100** | Se surgirem T-* novos para S2–S5, numerar sob Onda adequada (provável Onda 1/5 HW), sem sprint paralelo órfão |

---

## 7. Riscos

| Risco | Mitigação |
|---|---|
| Cortar FS/USB cedo → BOOT.LOG morto no Alienware | S1 primeiro; aceite = ficheiro em `E:` |
| Dependência cíclica k_nano↔k_hal | Só hooks `register_*` R0; k_hal chama k_nano, nunca o inverso em deps Cargo |
| “Facade” que esconde segundo monólito | `tools/check_duplication.py` no CI mental de cada slice |
| LOC vanity | Preferir remoção de **módulos sem caller** |
| Fase 2 cedo demais | Gate explícito §5 + 0102 |

---

## 8. Critérios de aceite

### Fase 1 (este ADR vira `Accepted` parcial quando)

- [x] S0 dedupe multi_user/hnsw  
- [~] S1: hub→MSC em k_hal **wired** (SESSION_314); evidência HW BOOT.LOG **AWAITING_OPERATOR** (`HW_FLASH_s314.md`)  
- [ ] ≥2 slices S2–S5 merged com boot check verde — **FREEZE** até 2 boots com log  
- [x] INDEX lifecycle → `fazendo`; `completa` (Fase 1) só após S1 PASS metal; Fase 2 pode ficar `pesquisa`  

### Fase 2

- [ ] Pré-condição 0102 HW  
- [ ] ADR adenda ou ADR nova se o desenho schemes divergir deste §5  

---

## 9. Checks,ляется no PR (rodar antes de marcar slice feito)

| Nº | Check | Comando / evidência |
|---|---|---|
| C0 | Build release limpo | `cargo clean -p neural-kernel && cargo check --release` → 0 erros |
| C1 | Sem novo excesso modular | `tools/check_duplication.py` → exit 0; lista de duplicatas vazia (ou intencional + justificada em SESSION) |
| C2 | Sem shadowing de singleton | `grep -Rn "lazy_static!.*SKILL_REGISTRY\|register_builtin_skills" crates/` → única fonte canónica; sem duplicatas bin↔crate |
| C3 | Hooks k_hal registados antes da fase do boot dependente | Boot log deve conter `H1 register_*_bringup` antes do stage onde o hook é consumido; se não, o slice regressa |
| C4 | Fat early não degradado | Após cada slice, boot QEMU lean e TCG: `E0 boot log` e FAT32 mount seguem em `k_nano`/hooks, não partida dead |
| C5 | Aceite metal S1 (USB/FS) | BOOT.LOG real em `E:\BOOT.LOG` (ou LBA correto via USB-MSC/AHCI); ver `docs/evidence/HW_FLASH_s314.md` |

---

## 10. TODO e referências ao Redox (para revisão em merge)

### 10.1 Referências ao tree Redox (clone em `/c/DEV/redox-kernel`, 2026-09-05)

- **Commit-base do comparativo:** consultar `git -C /c/DEV/redox-kernel log --oneline -1` no momento do PR; snapshot usado = commit printado no SESSION_313/314. O Redox é referência, não upstream — não fazer `git merge`/
- **locais de leitura (não alterar):** `src/scheme/` (trait `KernelScheme`), `src/scheme/sys/{block,context,fdstat,irq,proc,memory,pipe,proc,serio,sys,debug,event,memory,proc,serio,sys,time}.rs`, `src/scheme/user.rs`, `src/start.rs`, `src/arch/x86_64/interrupt/syscall.rs`, `src/arch/x86_64/interrupt/vectors.rs`, `src/context/`, `src/syscalls/mod.rs`, `src/syscall/debug.rs`, `src/allocator/linked_list.rs`, `src/allocator/mod.rs`, `src/scheme/irq.rs` — invariantes quando não há `.git` disponível: leitura somente.
- **Como enquadrar no PR:** cada slice pode incluir um SESSION_NNN.md com um parágrafo “o que o Redox faz, o que o AIOS faz, o que migrou” — evita repensar depois; deve ser breve e citar os arquivos Redox acima, não copiá-los.

### 10.2 TODO de Fase 1

| ID | O quê | Owner | Blocker |
|---|---|---|---|
| TODO-0103-1 | S1: wiring completa em k_hal::usb + early hook registado no boot (SESSION_314) | squada (USB) | aceite metal BOOT.LOG |
| TODO-0103-2 | S2: pick one NIC/device com política em k_hal + stub MMIO em k_nano; provar com boot + log | squada (net) | manter e1000 como canónico (no stdout QEMU) |
| TODO-0103-3 | S3: triage FS readers órfãos (ntfs/btrfs/ext2) — mover para crate k_ai ou deletar se 0 callers | squada (fs) | contagem de callers reais |
| TODO-0103-4 | S4: storage cognitivo (tickv FE/UI) → libs próprias; backend raw em k_nano fino | squada (sgdb) | NSGDB persiste OK |
| TODO-0103-5 | S5: podar lib.rs morto + codemap; rodar check_duplication.py como gate | maint. | sem regredir boot |
| TODO-0103-6 | S6: esqueleto `k_nano/src/{arch,memory,scheduler}/` sem mover lógica — só documentar layout proposto | maint. | opcional; não obrigatório |

### 10.3 TODO de Fase 2 (gated, não iniciar antes 0102 HW)

| ID | O quê | Owner | Pre-cond |
|---|---|---|---|
| TODO-0103-10 | 1 blob driver B/C Cap-gated rodando em CPL=3 (ex.: serio/debug) | squada (ring3) | 0102 aceite metal |
| TODO-0103-11 | Trait `KernelScheme`-like **só** se mapear 1:1 para CapGate + EventBus topic existente | maint. | não reinvent scheme list |
| TODO-0103-12 | FS userspace não como padrão; boot continua R0/k_hal | squada (fs) | gate de privilégio |

---

## 11. Referências diretas (para o revisor ler junto com o PR)

- `docs/architecture/0102-ring3-isolation-migration-cpl03.md` — teto de privilégio (Fase 2 gated).  
- `docs/architecture/INDEX.md` — lifecycle + tabela de substituições/coflitos.  
- `docs/architecture/0075-emagrecer-neural-kernel.md` — emagrecer bin → crates (prévio a este).  
- `docs/architecture/0041-k2chj-capability-rings.md` e `docs/architecture/0042-k2chj-adequacao-boot.md` — destinos semânticos K³CHJ.  
- `docs/architecture/0100-k3chj-backlog-custo-anel.md` — backlog de T-* quando surgem novos slices, numerar sob Onda adequada; não abrir sprint paralelo órfão.  
- `tools/check_duplication.py` — guarda de segundo monólito (calla no CI mental de cada slice).  
- Redox local: `/c/DEV/redox-kernel/` — snapshot de commit no SESSION; read-only para comparativo.

## 9. Conclusão

O rascunho Redox acerta o **diagnóstico** (k_nano inchado) e erra o **remédio imediato** (process-OS + mapa de duplicatas falso + mover SMP/mesh/FAT wholesale).

**ADR-0103 decide:** magreza por **anel K³CHJ** (Fase 1), protótipo USB já no caminho certo; privilégio estilo Redox schemes **só** depois do sandbox Ring3 real — e mesmo assim sem trair ADR-0102/0088.

**Próximo passo operacional:** fechar **S1** (coleta `BOOT.LOG` no metal), não redesenhar `arch/` ainda.
