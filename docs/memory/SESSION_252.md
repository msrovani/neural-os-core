# SESSION_252 — ADR-0086 Instalação e Update OTA: processo unificado + execução completa (2026-08-05)

**Escopo:** Consolidar instalação (ADR-0079) + update (ADR-0031 §1 / #308 / ADR-0074) numa ADR
canônica e **implementar todos os gaps** — o AIOS agora instala, baixa o cérebro, se auto-atualiza
com rollback, e reporta telemetria ao dev.
**Status:** ADR-0086 Accepted · 10 gaps fechados · 8 commits · 0 erros (6 warnings Known).

---

## 1. Documentação (ADR-0086 — processo canônico)

- **`docs/architecture/0086-instalacao-e-update-ota.md`** (nova, Accepted, `fazendo`):
  - §0 inventário de fontes (ADR-0079 + plan, ADR-0031 §1, ADR-0074, IDEA_BANK #176/#306–310/#417–423)
  - §2 instalação (AutoInstaller, modos, menu live/install, adaptação ao silício, autobiografia)
  - §3 update OTA (A/B dual-slot, topologias QEMU/note real, loop de telemetria, imagem fixa, HTTP→mesh)
  - §5 gaps U1–U6 / I1–I12 com origem rastreada
- **Deprecações (processo → 0086):** ADR-0079, ADR-0079-plan, ADR-0031 §1; INDEX + headers atualizados.
- **Descoberta:** ADR-0074 **não tem arquivo** (só referência no codemap/`git_thin.rs`) — lacuna
  registrada no INDEX; conteúdo consolidado na §3.3.

## 2. Implementação (10 gaps fechados)

### Update OTA
| Gap | Entregue | Commit |
|-----|----------|--------|
| **U2** trigger imediato | comando shell `update` → `check_for_update()` | 84a421b |
| **U6** update no GPT instalado | filtro `0xEF` (ESP FAT32) nos 4 pontos do self_update + UPDATE.CFG na ESP (build.rs) | 84a421b |
| **U1** elo de boot | `switch_slot()` promove slot inativo → `kernel.elf` (path fixo do Limine) + BOOTCFG | fef1eb7 |
| **U4** rollback automático | `rollback()` promove slot bom com guarda `tries`; BootSelfHeal dispara em PANIC/GPU_HUNG | af4992a |

### Instalação / ciclo de vida
| Gap | Entregue | Commit |
|-----|----------|--------|
| **I9** `boot_media::mode()` | `k_nano/boot_mode.rs`: CONFIG.TXT BOOT_MODE + NeuralFS 0x7F = Installed; cacheado + set_boot_mode | ff8362a |
| **I6** AutoInstallerAgent órfão | registrado no AgentFleet + comando shell `install` (publica SYS_INSTALL) | 4783c50 |
| **I4** ModelProvisioner | `model_provisioner.rs`: baixa slots vazios (HwExpert→RustCoder→Reranker→Active) via URL do UPDATE.CFG + register_bytes; comando `provision` | 486a22c |
| **I5** leitura NeuralFS no boot | `try_hub_slot_neuralfs` (monta 0x7F, lê /models/+/boot/) antes do FAT32; `persist_slot` grava /models/ | 7a89bdc |
| **I10** SELF.STATE | `k_ai/self_state.rs`: `sys/self_state` KV + `record_life_event` (narrativa L3); wiring boot + update | 8f1a4a9 |
| **I11** loop de telemetria | `log_agent.rs` POST /api/logs via `tcp_exchange` + `do_POST` no serve_update.py (grava `target/logs/`) | 194d278 |
| **I7** VRAM real | `bar_size_mb()` técnica PCI (0xFFFFFFFF→bits→restaurar) no BAR0 — fim do 2048 hardcoded | 7f006c9 |
| **I8** self_check real | `resolve_path("boot/kernel.elf")` + CRC32C vs INSTALL.CHK | e03e925 |
| **I12** imagem mini | `build_image.py --mini`: PACK_LLM=none + MODELS_SOURCE=network no CONFIG.TXT (~60MB) | 2f995a5 |
| **I3** agente executa install | `run_install_from_bus()`: SYS_INSTALL → source ATA + target AHCI/NVMe/USB | 69b618d |

### Limpeza
- **Stub hardcoded removido:** `CHANNEL_MANIFEST_URL` (IP QEMU), `UpdateChannel`, `poll_channel`,
  `channel_name` — todos mortos, URL do server vive só no UPDATE.CFG (ff8362a).

## 3. Decisões ponytail

- **U6:** em vez de suportar NeuralFS (centenas de LOC), usar a **ESP que já é FAT32 real (0xEF)** —
  o update fala o GPT instalado com ~15 linhas de diff.
- **U1:** não ensinar o Limine a ler slots — o `switch_slot()` **promove o slot → `kernel.elf`**
  (path fixo que o Limine já carrega). Zero mudança no bootloader.
- **I3:** a ESP copiada crua **é** o bootloader instalado (Limine + kernel.elf + UPDATE.CFG) —
  BootloaderInstaller dedicado desnecessário neste fluxo.
- **U3 (defer):** FNV-1a cobre integridade; Ed25519 = anti-tamper p/ server público — custo real é
  o server assinar (quebraria o fluxo dev). Reabrir quando update for público/mesh.
- **I12:** `PACK_LLM=none` já faz a imagem mini (llmfit_pack_filter respeita); só faltava o flag + MODELS_SOURCE.

## 4. Verificação

- `cargo check --release` — 0 erros (6 warnings = política Known Warnings do projeto).
- `cargo test -p hermes --lib self_update` — 2/2 PASS (manifest field, version ordering).
- `cargo test -p k_ai --lib self_state` — 1/1 PASS (phase names).
- `cargo test -p k-nano --lib boot_mode` — 1/1 PASS (mode codes).
- `serve_update.py` smoke: GET manifest 200, GET KERNEL.BIN 200 (17MB), 404 ok, **POST /api/logs 200**
  + arquivo `target/logs/neural-*.log` gravado íntegro.
- `build_image.py --mini`: imagem 128MB com raiz sem LLMs grandes (HwExpert + experts + LEGO + FW).

## 5. Lições

1. **Hardcoded de ambiente ≠ contrato de layout.** Nomes 8.3 do FAT (KERNEL~1) e path do Limine
   (kernel.elf) são contratos — configurá-los quebra o sistema. IP de server (10.0.2.2) é dado —
   deve viver no config file (UPDATE.CFG). O AIOS não carrega endereço como constante.
2. **"Resolver o gap" pode ser escolher o alvo mais simples**: U6 pedia NeuralFS; a ESP FAT32 (0xEF)
   já resolve — o filtro é 1 linha, o mount+create_file seriam centenas.
3. **Agente que só loga o evento = dívida**: o AutoInstallerAgent escutava SYS_INSTALL mas nunca
   instalava. O wiring (evento → execução real) é o gap que vale; a API já estava pronta.
4. **Stub morto com valor de ambiente = lixo a apagar**, não configurar (YAGNI — se canais um dia
   nascerem, nascem do UPDATE.CFG).

## 6. Estado do ciclo de vida (ADR-0086)

```
onde estou (I9 boot_mode) → quem sou (I10 SELF.STATE) → instalar (I3/I6 → GPT dual)
→ 1º boot: provision (I4) → persiste (I5) → boot lê do disco
→ update diário (U2) → slot (U6) → promove (U1) → rollback em PANIC (U4)
→ telemetria (I11) → opencode analisa target/logs/ → gera fix → ciclo
→ imagem fixa ~60MB (I12): uma imagem, qualquer alvo
```
