# Sprint Plan v1.1.x — A Era do Silício (Continuação)
# GPU · WiFi · Rede Nativa · JARVIS v2

**Data:** 2026-07-10  
**Versão alvo:** v1.1.5  
**Lema:** *"O hardware real não perdoa. O silício obedece."*

---

## v1.1.1 — GPU Compute (~1.700 LOC)

**Foco:** Matmul ternário na GTX 1050 via PFIFO + Intel HD 530 shader

| Item | LOC | Descrição | Depois de |
|------|:---:|-----------|-----------|
| NVIDIA PFIFO PUSH_BUFFER channel | ~300 | Inicializar canal PFIFO na GTX 1050 (GP108). Register map: `0x002000` (PUSH_BUFFER), `0x002004` (size), `0x002008` (tail). Mapear BAR0 como UC. | GPU detect (ok) |
| Firmware ACR loading | ~300 | Carregar FECS + GPCCS blobs do linux-firmware para a GPU via DMA. Pipeline: WPR (Wide Payload Register) → LS ucode → signature verification → falcon boot. | NVIDIA PFIFO |
| Matmul shader ternário | ~200 | Kernel que executa `c = a * w` onde w é ternário (−1/0/+1). Embarque como blob (CUBIN ou PTX). Pipeline: CPU → DMA → VRAM → GPU executa → DMA → CPU. | Firmware loaded |
| Intel Gen9+ EU shader | ~300 | Usar ring buffer `gpu/intel.rs` para submeter batch buffer com instruções EU (send, add, mul). Compilar via intel-graphics-compiler ou assembly manual. Mais simples que NVIDIA (sem firmware signed). | GPU ring (ok) |
| Fallback automático | ~100 | `ternary_matmul_adaptive()` tenta GPU → se falhar, CPU. Pipeline: detect GPU disp., aloca VRAM, DMA weights, executa, DMA resultado. | Ambos shaders |
| Benchmark TFLOPS | ~100 | Medir TFLOPS real vs teórico da GTX 1050 (1.8 TFLOPS FP16). Relatório: "matmul 128×128: 0.3 TFLOPS (17%) vs CPU AVX2: 0.05 TFLOPS". | Tudo acima |

**Total:** ~1.700 LOC

---

## v1.1.2 — WiFi SoftMAC (~1.200 LOC)

**Foco:** Intel AX200 firmware + 802.11 scan/associate + data path

| Item | LOC | Descrição | Depois de |
|------|:---:|-----------|-----------|
| iwlwifi firmware loader | ~300 | DMA ring → CSR → ucode SRAM. CRC verification, handshake. Firmware de linux-firmware (open source). | DMA engine (ok) |
| 802.11 Probe Request | ~150 | Montar e enviar frame Probe Request. Processar Beacon Response. Parse IE: SSID, rates, channels, RSN. `wifi_protocol.rs` já tem parser. | Firmware loaded |
| 802.11 Associate | ~300 | 4-way handshake: Auth Req → Assoc Req → EAPOL-Key 1/4 → 2/4 → 3/4 → 4/4 → PTK derivation. | Scan funcional |
| smoltcp bridge | ~150 | `impl smoltcp::phy::Device for AgnosticWifiEngine`. Unificar path de pacotes: serial tunnel + WiFi + Ethernet. | Associate |
| Suporte multi-chip | ~300 | Register maps + firmware loading para Realtek RTL8822, Atheros QCA6174, Broadcom BCM4360. | Intel funcional |

**Total:** ~1.200 LOC

---

## v1.1.3 — Rede Nativa (~600 LOC)

**Foco:** VirtIO-net + E1000 funcionais sem serial tunnel

| Item | LOC | Descrição | Depois de |
|------|:---:|-----------|-----------|
| VirtIO-net RX debug | ~200 | Depurar por que RX não entrega. Verificar: MMIO BAR UC? buffer alignment? descriptor ring inicializado? IRQ? Testar com WHPX + QEMU. | — |
| E1000 RX enable | ~100 | Verificar Receiver Enable bits (análogo ao RTL8139 CR_RE). `dump_e1000_status()` já existe. | — |
| Loopback test | ~100 | ping 127.0.0.1 ou entre duas portas VirtIO. Não depende de rede externa. | VirtIO funcional |
| Fallback automático | ~100 | Se NIC falhar → serial tunnel. Já temos `SystemEnv`. | Ambos |

**Total:** ~600 LOC

---

## v1.1.4 — JARVIS v2 (~1.200 LOC)

**Foco:** Memória persistente, reconhecimento, personalidade viva

| Item | LOC | Descrição | Depois de |
|------|:---:|-----------|-----------|
| Memória episódica persistente | ~300 | Salvar `EpisodicMemory` no NeuralFS. Carregar no boot. A memória não morre no reboot. | NeuralFS (ok) |
| SOUL.md loader | ~100 | Personalidade lida de `SOUL.md` no NeuralFS. Editável pelo usuário. Hoje hardcoded. | NeuralFS (ok) |
| Reconhecimento facial | ~400 | Câmera USB/UVC → frame → detector facial (Haar cascade simples ou MLP). JARVIS "vê" você e cumprimenta. | UVC driver (ok) |
| TTS streaming real | ~200 | Saída de som pelo HDA audio driver. Hoje o TTS gera PCM mas o HDA não reproduz. | HDA driver (ok) |
| Proatividade | ~200 | JARVIS sugere ações sem comando: "Disco 90% — quer limpar?", "WiFi disponível — conectar?". | Todas as skills |

**Total:** ~1.200 LOC

---

## v1.1.5 — Integração + Testes HW Real (~1.000 LOC)

**Foco:** Validar tudo em HW real (i5-6400 + GTX 1050)

| Item | LOC | Descrição |
|------|:---:|-----------|
| Boot HW real | — | Boot pelo pendrive. Validar: framebuffer, PCI scan, AHCI, FAT32/exFAT, NeuralFS |
| GPU compute HW | — | Rodar matmul ternário na GTX 1050 real. Medir TFLOPS |
| WiFi HW | — | Conectar a rede WiFi real. scan → associate → ping → HTTP get |
| Rede nativa HW | — | Ethernet real sem serial tunnel |
| JARVIS HW | — | Voz, câmera, memória persistente entre reboots |
| Bateria de testes | ~500 | Testes automatizados com MemoryDisk para todos os módulos NeuralFS |
| Release v1.1.5 | — | Tag + CHANGELOG + release notes |

---

## Cronograma

| Versão | Início | Término | Entrega |
|:------:|:------:|:-------:|---------|
| **v1.1.0** | ✅ Pronto | — | Sprint FS-v2 |
| **v1.1.1** | Imediato | N+3 sem | GPU Compute (GTX 1050 matmul) |
| **v1.1.2** | N+4 sem | N+8 sem | WiFi Intel AX200 |
| **v1.1.3** | N+8 sem | N+10 sem | Rede nativa |
| **v1.1.4** | N+10 sem | N+14 sem | JARVIS v2 |
| **v1.1.5** | N+14 sem | N+16 sem | Integração + HW real |

---

## Resumo

| Versão | Foco | LOC | Risco |
|:------:|------|:---:|:-----:|
| v1.1.0 | ✅ FS-v2 | ~3.884 | Concluído |
| v1.1.1 | GPU Compute | ~1.700 | 🔴 Firmware NVIDIA signed |
| v1.1.2 | WiFi | ~1.200 | 🟡 Firmware loading frágil |
| v1.1.3 | Rede nativa | ~600 | 🟡 RX debug |
| v1.1.4 | JARVIS v2 | ~1.200 | 🔵 Depende de HDA + UVC |
| v1.1.5 | Integração HW | ~500 | 🔴 HW real imprevisível |
| **Total** | | **~9.084** | |
