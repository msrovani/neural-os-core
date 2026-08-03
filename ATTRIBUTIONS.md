# Atribuições — Código e Metodologia de Terceiros

Este projeto utiliza conceitos, metodologias ou trechos de código dos seguintes projetos open-source. Atribuição exigida por suas licenças.

---

## one-skill-to-rule-them-all

**Repo:** https://github.com/rebelytics/one-skill-to-rule-them-all  
**Autor:** Eoghan Henn / rebelytics.com  
**Licença:** Creative Commons Attribution 4.0 International (CC BY 4.0)  
**Uso:** Metodologia de meta-observação de skills, protocolo de logging, Pre-Flight Principle, Comprehensive Review cadence. Adaptado para Rust no_std em `skill_observer.rs` (Sprint 67).

**Atribuição:** Este trabalho incorpora material do projeto "One Skill to Rule Them All" de Eoghan Henn (https://github.com/rebelytics/one-skill-to-rule-them-all), disponível sob CC BY 4.0.

---

## agency-agents

**Repo:** https://github.com/msitarzewski/agency-agents  
**Autor:** msitarzewski  
**Licença:** MIT  
**Uso:** Catálogo de 16 divisões de agentes AI especializados, portados para o formato AgentManifest em `agency.rs` (Sprint 67). Cada agente .md foi convertido para entrada no registry.

**Atribuição:** Partes deste software são baseadas no projeto "The Agency" (https://github.com/msitarzewski/agency-agents), licenciado sob MIT.

---

## Hermes Agent

**Repo:** https://github.com/ (projeto Hermes Agent)  
**Autor:** Comunidade Hermes (207k★)  
**Licença:** Inspiração — conceitos extraídos, sem código copiado  
**Uso:** Conceitos de `/learn`, completion contracts, background fan-out, MoA (Sprint 67). Implementação própria em Rust.

---

## demais dependências

Ver `Cargo.toml` para licenças das dependências Rust (crates).

---

# Artefatos Binários de Terceiros

Este projeto distribui (no boot, via imagens de disco) artefatos binários de terceiros. **Eles NÃO são cobertos pela licença comercial** — cada um permanece sob os termos do seu proprietário. Firmware e pesos são baixados no build por script com verificação de hash (ver seções abaixo); as licenças e origens estão listadas aqui.

## Firmware (linux-firmware)

**Origem:** [linux-firmware](https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git) (espelho GitLab: `https://gitlab.com/kernel-firmware/linux-firmware.git`), arquivo `WHENCE` + `LICENSES/`.

**Download:** `python tools/download_firmware.py` — copia do clone para `firmware/` + `target/firmware/`, pinando o commit (`GIT_COMMIT=` em `FW_POLICY.txt`) e escrevendo `SHA256SUMS`. Verificação: `python tools/download_firmware.py --verify`.

Todas as famílias abaixo são **"Redistributable" (proprietárias, binário-form)**, NÃO licenças OSI. Nenhuma proíbe uso comercial diretamente, mas **nenhuma concede direito explícito de sublicenciamento** — portanto não fazem parte do que a exceção comercial licencia:

| Família | Licença (WHENCE) | Restrição-chave |
|---|---|---|
| `rtl_nic/` (Realtek Ethernet) | Redistributable, `LICENSE.r8169` | Permissão de distribuição do firmware em formato hex/equivalente com aviso de copyright; sem restrição comercial explícita |
| `rtlwifi/` (Realtek WiFi) | Redistributable, `LICENCE.rtlwifi_firmware.txt` | Redistribuição em forma binária sem modificação; proíbe engenharia reversa; patente limitada a OS licenciado open-source |
| `intel/iwlwifi/` | Redistributable, `LICENCE.iwlwifi_firmware` | Idêntico ao padrão Intel acima |
| `amdgpu/` (green_sardine, gc_10_3_6, psp_13_0_5, sdma_5_2_6, gc_11_5_0) | Redistributable, `LICENSE.amdgpu` | Única família que permite explicitamente repassar adiante ("to permit persons ... to do the same"); binário-form, sem engenharia reversa |
| `i915/` (skl, kbl, dg2) | Redistributable, `LICENSE.i915` | Padrão Intel binário-form |
| `xe/` (bmg, lnl, ptl) | Redistributable, `LICENSE.xe` | Padrão Intel binário-form (© 2024 Intel) |
| `ath10k/QCA6174/hw3.0/` | Redistributable, `LICENSE.QualcommAtheros_ath10k` | Redistribuição **somente para uso com chipset Qualcomm Atheros**; sem engenharia reversa |
| `nvidia/gp108+tu106/gr/` | Redistributable, `LICENCE.nvidia` (EULA §2.1.2) | Cópia/redistribuição permitida somente para OS sob licença aprovada pela OSI, binários não modificados, com cópia da licença; **sem sublicenciamento** |
| `nvidia/{tu102,ga102,ad102}/gsp/` (GSP 535.113.01) | Redistributable, `LICENCE.nvidia` | Mesma exceção §2.1.2; GSP vive só em `target/firmware/` (não versionado) |

**Atenção (NVIDIA):** a exceção de redistribuição está condicionada ao OS ser distribuído sob licença aprovada pela OSI. O dual-licensing AGPLv3 + exceção comercial é zona cinzenta — validar com counsel antes de oferecer a exceção comercial sobre os blobs NVIDIA.

## Pesos de modelo

| Arquivo | Origem | Licença | Notas |
|---|---|---|---|
| `models/tokenizer/PIPER_PT_BR.BIN` | [Piper TTS](https://github.com/rhasspy/piper) — voz `pt_BR-cadu-medium` ([piper-voices](https://huggingface.co/rhasspy/piper-voices)) | **MIT** (repo-level) | Modelo ONNX ~63 MB convertido; `MODEL_CARD`: dataset CC0, fine-tune de `lessac`. ⚠️ A cadeia de dados do `lessac` (Blizzard 2013) tem licença **research-only** — caveat a documentar em auditoria |
| `models/tokenizer/E5_MULTI.BIN` | [intfloat/multilingual-e5-small](https://huggingface.co/intfloat/multilingual-e5-small) | **MIT** | Atribuição requerida |
| `models/tokenizer/BGE_M3.BIN` (não versionado, gerado por `convert_bgem3.py`) | [BAAI/bge-m3](https://huggingface.co/BAAI/bge-m3) | **MIT** | — |

**Download:** `python tools/download_models.py` — manifesto com SHA-256 fail-closed; pins pendentes (None) imprimem instruções de conversão em vez de baixar.

## Bases de dados de HWIDs (derivadas)

| Arquivo | Origem | Situação |
|---|---|---|
| `models/pci_usb/pci_usb_hwids.json`, `hw_all_unified.csv` | [pci.ids](https://pci-ids.ucw.cz/) + [usb.ids](http://www.linux-usb.org/usb-ids.html) | Dual: **GPL-2+ OU BSD-3-Clause** (escolher BSD-3, manter aviso) — OK |
| `models/pci_usb/sdio_hwids.json`, `models/WDM/hwids.json` + `stats.json` | Extraído de DriverPacks SDIO + Windows DriverStore `.inf` (Microsoft/fabricantes) | ⚠️ **Sem licença limpa.** `driverpacks.net/LICENSE`: proíbe redistribuição como parte de pacote comercial sem permissão escrita de Wim Leers; `.inf` do DriverStore cobertos por EULA da Microsoft. HWIDs numéricos (`PCI\VEN_...&DEV_...`) são fatos (precedente pci.ids: "copyright cobre só agregação"), mas **o conjunto derivado de DriverPacks não pode ser vendido como está** |
| `models/pci_usb/regulatory.db` | [wireless-regdb](https://kernel.org/pub/software/network/wireless-regdb/) | Base de dados regulatória (regras de RF por país); redistribuível, ver projeto |
| `models/hw_expert/v4/dataset.json`, `models/hw_expert/*.bitnet`, `models/pci_usb/hw_expert_tf.bitnet` | Treinado pelo projeto a partir das fontes acima | Pesos de modelo treinados (saída de treino), dados de treino têm as ressalvas das linhas acima |

**Recomendação (SDIO/WDM):** para a exceção comercial, manter apenas tuplas VID/DID numéricas (fatos) citando pci.ids/usb.ids como fonte licenciada, e obter permissão escrita do DriverPacks.net (ou scrubbing de proveniência) para o restante. Ver `docs/memory/SESSION_102`/`SESSION_238` para o pipeline de extração.

## Ferramentas de extração

| Script | Propósito |
|---|---|
| `tools/download_firmware.py` | Sync firmware linux-firmware → `firmware/` + `target/firmware/` com pin de commit + SHA256SUMS |
| `tools/download_models.py` | Download de pesos de modelo de terceiros com verificação de hash (fail-closed) |
| `tools/extract_sdio_hw.py`, `tools/extract_wdm_hwids.py`, `tools/unify_hwids_v4.py` | Extração/união de HWIDs (ver ressalvas acima) |
