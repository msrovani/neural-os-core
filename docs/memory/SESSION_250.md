# SESSION_250 — AIOS na veia: RAM física → HMI → auto-adaptação + Boot do 2B (2026-08-05)

**Escopo:** Preceptiva do dono — o AIOS deve **ler o tamanho da memória física disponível, elencá-lo no HMI e se auto-adaptar** (heap sob demanda; AirLLM/layer-streaming se necessário). Junto: destravar o boot do 2B v6 (download, conversão, diagnóstico de wrap 2⁶⁴).

**Status:** Premissa AIOS implementada (heap self-adapting + gate AirLLM + HMI) · boot 2B: conversão OK + scan autodescritivo + wrap 2⁶⁴ diagnosticado (revertido por segurança) · 0 erros cargo check --release · boot QEMU com reboot loop após resize (wrap pendente de fix de allocator).

---

## 1. Premissa AIOS (premissa 4 do dono)

### 1.1 Heap self-adapting (auto-adaptação à RAM física)
- **Antes:** `resize_bump_heap(2048)` hardcoded no boot (main.rs:1437) — heap fixo independente da máquina.
- **Agora:** `heap_initial_mb = clamp(75% da RAM detectada, 512..1536)` no boot + **`grow_bump_auto`** (novo, em `k_nano::allocator`): o bump cresce **sob demanda** quando a alocação atinge `HEAP_LIMIT` — mapeia +256MB/passo, **verifica `heap_pte_present`** pós-mapeamento e retorna true só se `need` ficou coberto (senão re-tenta). Eliminou o OOM que o resize hardcoded causava.
- Evidência no log: `[HEAP] [AIOS] - heap auto-alvo=1536MB inicial (RAM detectada=9216MB; grow sob demanda)`.
- **Lição:** NÃO mapear eager 6GB em TCG (exaure frames → reboot loop). Piso inicial modesto + crescimento preguiçoso.

### 1.2 Gate AirLLM (layer-streaming quando o modelo não cabe)
- `cortex::model_fit::needs_airllm(params, model_file_mb)`: true quando `modelo + heap estimado > 75% da RAM física` — decide residente vs AirLLM honestamente.
- `estimate_heap_mb` clamp agora derivado da RAM (não hardcoded 128..2048).
- Logado no load do modelo (main.rs): `LLM LOADED ... RAM={}MB airllm={}`.

### 1.3 HMI (elencar a RAM)
- Já existia: SysInfoAgent (card 9001) lê `TOTAL_RAM_MB`/`CURRENT_HEAP_MB`/`heap_used_bytes`/frame usage — a leitura da RAM física no HMI estava pronta; a auto-adaptação é que faltava.

## 2. Boot do 2B v6 (download + conversão + diagnóstico)

### 2.1 Download e conversão
- `config.json` + `model.safetensors` (1.1GB) do `microsoft/bitnet-b1.58-2B-4T` → `target1/` (decisão do dono: modelos novos em target1).
- `convert_bitnet.py` apontado para `target1/`; **encoder Q6_K vetorizado** (numpy) — era loops Python por elemento sobre 328M elementos (horas) → 0.012s.
- **2B v6 canônico:** 792MB, magic 0xBE11BE11, ver=6, act_type=1 (RELU2), embed_type=1 (Q6_K), feat=0x07, theta=500000, tie=TIED. Header validado byte a byte.

### 2.2 Scan autodescritivo (anti-hardcode)
- O scan do QEMU-loader usava `BITNET_2B_V4_BYTES = 604_856_373` (tamanho do 2B **v4**) → truncava o 2B v6 (792MB) → parse lixo.
- Fix: `cortex::model::v6_file_size(data)` deriva o tamanho total do arquivo a partir do header v6 (autodescritivo). Log: `magic OK @0x100000000 exact=773545KB` (correto).

### 2.3 🔴 Wrap 2⁶⁴ no bump heap (diagnóstico do oracle)
- **Sintoma:** após o auto-grow (2048→2304MB), `#PF ip=memcpy CR2=0` — escrevendo em VA 0.
- **Causa (oracle, disassembly):** `HEAP_BUFFER` @ 0xffffffff809c59d8; `heap_start + offset` envolve 2⁶⁴ em offset ≈2044MB. O 2B (cópia 755MB + embed Q6_K 257MB → offset ~2158MB) cruza o wrap → `rep movsq` escreve em VA 0 (não-mapeada).
- **Tentativa de fix:** `bump_virt` → `HEAP_EXT_BASE` (p4[508]). **Falhou**: o boot-time `resize_bump_heap` com HEAP_EXT_BASE causa reboot loop (map_page_direct sem check de HUGE_PAGE em certo nível → lê P2 garbage → early-return → páginas não-mapeadas).
- **Decisão:** **revertido** para `heap_start + offset` — o boot-time resize (512→1536MB) não cruza o wrap (só ~2044MB+ cruza); o grow runtime do 2B ainda cruza. Documentado como **known-issue**: `map_page_direct` precisa de check HUGE_PAGE em TODOS os níveis antes de confiar no mapeamento (fix real futuro).

### 2.4 CUDA / GPU (descoberta da SESSION_249b, confirmada)
- GTX 1050 funciona com torch 2.13+cu126 (arch list inclui sm_61; o drop era do cu130). `CUDA_VISIBLE_DEVICES=0` destrava. Retreino RustCoder concluído (commit 3eb6c51).

## 3. Bugs corrigidos na rodada

1. **Encoder Q6_K vetorizado** — 328M elementos com loops Python (horas) → numpy (0.012s).
2. **Scan truncava o 2B v6** — const v4 hardcoded → `v6_file_size` autodescritivo.
3. **OOM no heap** — `resize_heap_to_mb` estimado estendia o TALC (que não é o global allocator!) → removido; `grow_bump_auto` no bump resolve.
4. **Wrap 2⁶⁴** — diagnosticado (oracle) e revertido por segurança (known-issue).

## 4. Pendentes (conhecidos)

- **Boot 2B em TCG:** reboot loop após resize (wrap 2⁶⁴ no grow runtime). Fix real: check HUGE_PAGE em todos os níveis do `map_page_direct` + HEAP_EXT_BASE com walk correto — sessão de allocator dedicada.
- **AirLLM real (layer streaming):** o gate `needs_airllm` existe; a implementação de streaming por layer do storage (FAT) é o próximo passo se o 2B não couber em máquinas pequenas.
- **W2A8 gated** (ADR-0084 F4) — já implementado (commit 3f15a4a), aguarda WHPX/HW real.

## 5. Evidência

- `cargo check --release --workspace`: 0 erros.
- Log do boot 2B: `heap auto-alvo=1536MB (RAM detectada=9216MB)`, `v6 LLM h=2560 L=30`, auto-grow AIOS OK, depois wrap #PF (conhecido).
- `v6_file_size`: `exact=773545KB` (792MB correto).
