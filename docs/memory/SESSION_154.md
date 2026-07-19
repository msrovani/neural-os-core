# SESSION_154 — TLS #123 pesquisa (B) + WiFi firmware/plano

**Data:** 2026-07-18  
**Tipo:** pesquisa-only (sem wire Cargo / `net.rs` / drivers)  
**Check:** docs + inventário firmware em disco

---

## Escopo

| Track | Entrega | Não feito |
|-------|---------|-----------|
| TLS #123 | Opções A–D + ADR-0016 N4 | Sem dep TLS; stub `https_get` intacto |
| WiFi | Inventário API77 + novidades + plano S0–S5 | Sem Ready; sem Embassy; sem fake Connected |

Gate de rede permanece **e1000 + HTTP plain**.

---

## Parte A — TLS #123 (pesquisa B)

### Blocker canônico

| Camada | Evidência |
|--------|-----------|
| Soft-float | `.cargo/config.toml` — `-sse…-sse4.2` no `x86_64-unknown-none` |
| Stub | `net::https_get` / `parse_http_url` → `Err("tls_not_ready")` |
| Log | `[TLS] VERDICT=BLOCKED reason=softfloat_or_crate` |
| Política | **Nunca** strip `https://` → `http://:80` |
| Dep | `embedded-tls` **ausente** de todos os `Cargo.toml` |

### Candidatos externos (2026)

| Candidato | Notas | Risco soft-float |
|-----------|-------|------------------|
| **embedded-tls 0.19** (drogue, ~2026-06) | TLS 1.3; `default-features=false`; blocking `embedded-io` | Crypto deps podem puxar float/SSE |
| **noxtls** (argenox) | TLS 1.3 no_std+alloc; AES-GCM + P-256 | Novo; mesmo risco ABI |
| **RustCrypto / rscrypto** | Primitivas AES-GCM/P-256; **não** stack TLS | Úteis com hardfloat seletivo ou integer-only |
| **rustls / openssl** | Rejeitados (std) | Fora Ring0 |

Cruzamento: SESSION_147 (hardfloat seletivo VITS) — **mesmo trade-off ABI** se TLS exigir SSE/AES-NI.

### Opções ranqueadas (wire futuro — não esta sessão)

1. **A — Compile probe isolado** — scratch/`target/check-tls` com `embedded-tls` ou `noxtls` + soft-float; registrar PASS/FAIL; **sem** merge no bin.
2. **B — Hardfloat seletivo** só crate crypto (espelha SESSION_147 opção 2); PoC medido.
3. **C — Offload HTTPS** Ring3/WASM CapGate (alinha ADR N4 “WASM”); bin fica HTTP.
4. **D — Stack mínimo integer-only** — último recurso se A/B falharem; alto custo/risco.

### Trust (quando wire existir)

TOFU ou root embutido + CapGate + Trust `(token,agent,skill)`.  
Consumidores: Browser/Market/RSS/SelfUpdate/AirLLM `/model-fetch`.  
Exit: `[TLS] VERDICT=PASS` + PreFlight `tls-fetch` PASS.

### Aceite TLS desta sessão

- [x] Opções A–D documentadas (ADR-0016 N4 + esta SESSION)
- [x] IDEA #123 permanece ▶️ BLOCKED soft-float
- [x] Zero alteração em `Cargo.toml` / `net.rs`

---

## Parte B — WiFi: inventário + novidades + plano

### Inventário firmware (`firmware/intel/iwlwifi/`)

Medido em disco 2026-07-18 (API **77** em todos os nomes):

| Arquivo | Chip (docs) | Bytes |
|---------|-------------|------:|
| `iwlwifi-cc-a0-77.ucode` | AX200 | 1 368 100 |
| `iwlwifi-so-a0-gf-a0-77.ucode` | AX201 | 1 641 260 |
| `iwlwifi-so-a0-hr-b0-77.ucode` | AX210 | 1 500 532 |
| `iwlwifi-ty-a0-gf-a0-77.ucode` | AX211 | 1 594 276 |
| `iwlwifi-Qu-b0-hr-b0-77.ucode` | AX101 | 1 406 576 |
| **Total** | 5 blobs | **7 510 744** (~7,51 MB) |

**Gaps firmware:** nenhum `.pnvm` no repo; kernel Linux usa `IWL_FW_AND_PNVM` para várias famílias AX210/GF. API 77 pode estar atrás do `linux-firmware` atual — refresh controlado via GitLab `kernel-firmware/linux-firmware` **sem** fingir `[IWL] ucode alive`.

### Estado código (ground truth)

| Camada | Estado |
|--------|--------|
| k_hal | Scaffold CSR/HBUS/`load_ucode`/`scan(0x34)` / MSI-X / WPA MMIO guess |
| WifiAgent | `VERDICT=AWAITING_REAL_HW`; demo APs; **`do_connect` ainda publica “Conectado”** (desonesto — S0) |
| HalOffer | `DeviceClass::Wifi` existe; FE ainda faz probe PCI direto |
| Gate | e1000; QEMU sem RF iwlwifi |

### Novidades externas (pesquisa 2026)

1. **Intel Wi-Fi Linux Core102 / 24.20** — foco Wi-Fi 7 (BE2xx) + AX210/211/201/101; **AX200 sem updates novos** nesse pacote (blobs legados linux-firmware).
2. **Kernel iwlwifi** — nomes FW família AX210 montados dinamicamente (MAC/RF type); vários chips exigem **`.ucode` + `.pnvm`**.
3. **Arquitetura:** iwlwifi moderno = **FW MAC** (host = transport + cmd queue + cfg), **não** SoftMAC clássico ath9k (ACK host ~10 µs).
4. **Implicação #408:** reclassificar — Embassy/async genérico; SoftMAC ACK só se medirmos path host-ACK. Priorizar **#407** ucode/transport/cmd.

### Gaps concretos

| Gap | Severidade |
|-----|------------|
| DID→blob(+pnvm) + FAT load + TLV real + ALIVE | P0 HW (#407) |
| SCAN real + RX beacons (não demo AP) | P0 HW |
| Assoc + WPA2-PSK real (não XOR PBKDF2) | P1 |
| HalOffer-only FE (sem BAR no Hermes) | P1 arch |
| `do_connect` honestidade | P0 honesty (S0) |
| smoltcp Device (#410) | P2 pós-RF |
| USB HardMAC (#409) | P3 alt |
| Embassy SoftMAC ACK (#408) | Condicional pós-S2 |

### Plano faseado (documentado; código só após OK + HW)

| Fase | Entrega | Exit |
|------|---------|------|
| **S0 Honesty** | VERDICT em connect; sem “Conectado” falso | Serial sem Ready WiFi |
| **S1 Firmware** | DID→blob(+pnvm); load; ALIVE real | `[IWL] ucode alive` em HW |
| **S2 Scan** | SSIDs do ar | WifiAgent lista RF |
| **S3 Link** | Assoc + WPA2 + DHCP; HalOffer Bound | IP via WiFi |
| **S4 Async** | #408 só se timing host-side necessário | Critério medido |
| **S5 Alt** | #409 USB CDC ECM se PCIe bloqueado | Opcional |

### Aceite WiFi desta sessão

- [x] Inventário bytes/API77
- [x] Novidades Core102 / pnvm / FW-MAC
- [x] Plano S0–S5 + #408 reclassificado
- [x] Nenhum claim Ready novo no código

---

## Docs tocados

- `docs/architecture/0016-network-strategy.md` (N4)
- `docs/memory/IDEA_BANK.md` (#123, #407–410)
- `docs/memory/SESSION_INDEX.md` / `STATE.md`
- `docs/architecture/INDEX.md`
- `TECNOLOGIAS.md` 4.3d / 4.4

## Próximo (código — fora desta sessão)

1. Compile probe TLS opção A (`target/check-tls`) **ou** S0 honesty WifiAgent.
2. Refresh firmware + pnvm sob demanda HW.
3. Wire N4 / S1 só com evidência serial.

**Ponte SESSION_159:** S0 honesty + prep S1 (DID→FAT short names, `iwl_fw.rs`) ✅ QEMU.

**Ponte SESSION_160:** pista ativa = **Note 1050 ath10k QCA6174** (`168C:003E`); iwlwifi secondary. A3 = BMI/fw_ready no Note.
