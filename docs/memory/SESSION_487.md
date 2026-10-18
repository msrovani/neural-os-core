# SESSION_487 — LOO destravado: seam registrado no sandbox + Pro preferido (fim do ovo-galinha)

**Data:** 2026-10-18
**Continua:** s483 (Passo 2b, seam de load-on-first-use), s485 (evict), s486 (poda).
**Natureza:** o diff já estava no working tree sem commit nem doc; esta sessão **verificou**,
documentou e fechou — não reescreveu.

## 1. Problema: o seam existia, mas dois gates o anulavam

O s483 2b criou `SlotLoader`/`register_slot_loader`/`ensure_slot_loaded` e removeu as 5
leituras eager do boot. Em QEMU nada disso era exercitável, e o registro do s483 chamou isso
de "fail-safe". Eram **três** bloqueios empilhados:

| # | Bloqueio | Efeito |
|---|---|---|
| 1 | `qemu_dev_skip_models` (`hypervisor().is_sandbox()`) envolvia **todo** o bloco do hub, inclusive `register_slot_loader` | `SLOT_LOADER = 0` em sandbox → `try_load_on_demand` sempre `false` → caminho antigo. O seam nunca era registrado, então **nunca podia ser exercitado em dev** |
| 2 | `select_generator_slot` exigia `hub_has_blob(GeneratorPro)` para escolher Pro | Pro sem blob nunca era escolhido — e LOO existe exatamente para carregar o blob que falta. **Ovo-galinha**: precisa do blob para escolher o slot, precisa do slot para carregar o blob |
| 3 | Early-return em `try_hub_slot_fat`: `qemu_loader_2b && model_is_loaded() && slot ∈ {Vision, Pro}` → `return` | Mesmo que o seam disparasse, o load era recusado com log `info` ("Active ja carregado via QEMU-loader") |

Os três são da mesma classe: **um gate que protege trabalho também mata o caminho que
deveria exercitá-lo**.

## 2. Fix (3 peças + 1 remoção)

1. **P0 — gate separado** (`main.rs`): sandbox passa a pular só o *eager* NeuralFS;
   `register_slot_loader(hub_load_on_demand)` roda **sempre**. Log novo:
   `HUB ok sandbox: LOO seam only (sem eager NeuralFS) — first-use + pio_cap`.
2. **Decisão por intenção** (`model_hub.rs::select_generator_slot`): `slot_real` no lugar de
   `hub_has_blob`; prompt complexo com Pro ausente devolve `GeneratorPro` **de propósito**
   (dispara LOO). O fallback já existia e é honesto: `dispatch_expert` → `generate_from_slot`
   → `None` → `pro miss → CURRENT_MODEL (Active)` com warn (cortex.rs:5848-5853).
3. **P1 — adapter** (`hub_load_on_demand`): NeuralFS (disco instalado) **antes** de FAT
   (pendrive/live); o bool sai de `slot_real` depois da tentativa.
4. **Removido o early-return** do `qemu_loader_2b`. O que ele protegia (PIO gigante do chat no
   boot) continua protegido pelo `pio_cap` (`PIO_QEMU = 8MB` sob `qemu_hv`, `PIO_FAST = 400MB`
   em metal) — era guard duplicado com o cap, e a duplicação era o que matava o LOO.

Teste novo: `select_complex_prefers_pro_even_absent` — precondição `!slot_real(Pro)` e
assert de que prompt complexo devolve `GeneratorPro` (não `Active`).

## 3. Prova

- `cargo check --release` → **exit 0** (0 erros; warnings pré-existentes).
- `cargo test -p cortex --lib model_hub` → **2/2**
  (`select_complex_prefers_pro_even_absent`, `evict_slot_libera_blob_e_limpa_estado`).
- Sem mudança de assinatura pública; `qemu_loader_2b` continua usado no cálculo de
  `pio_cap` — a remoção foi só do early-return.
- **Runtime WHPX VERIFIED (seam)** — log
  `logs/boot_hub_nodisk_s487_20261018_130517.txt` (NoDisk + Falcon3-3B loader
  `models/FALCON3.BIN` @ `0x100000000`, `-m 6G -smp 2` Haswell):
  - `HUB ok sandbox: LOO seam only (sem eager NeuralFS) — first-use + pio_cap`
  - `HUB ok loader registrado (load-on-first-use; 0 leituras PIO no boot)`
  - `MODEL ok ModelHub: active=1 vision=0 generator_pro=alias reranker=0 …`
  - `Asset ok LLM LOADED … size=1012764KB` + `LOADED — model active in ModelHub`
- Confundidor: boot **com** `disk_qemu.raw` FAT 512MB (VISION+RERANKER) estola em
  `K33[26] tickv` após `MBR type=0x0c` (pré-existente; hub fica depois do tickv).
  Aceite do seam usou NoDisk; niches no FAT = metal ou tickv sem hang.

## 4. Honestidade: o que é VERIFIED vs UNKNOWN

| Claim | Status |
|---|---|
| Seam registrado em sandbox | **VERIFIED** (log acima) |
| Active via QEMU-loader | **VERIFIED** (1012764KB) |
| Select complexo → Pro ausente | **VERIFIED** host test |
| Load real de Pro/Vision/Reranker via LOO | **UNKNOWN** — `pio_cap` 8MB sob `qemu_hv` rejeita blobs >8MB; aceite = metal ou cap liberado |
| Validate REJECT / evict sob pressão | **UNKNOWN** (exige LOO que passa do cap) |

## 5. Lições

- **Seam atrás de gate de sandbox nasce morto em dev** — "fail-safe" (seam não registrado =
  caminho antigo) também significa "nunca exercitado". O gate deve pular o **trabalho**, não
  o **registro**. Um hook que não pode ser ligado em dev não é verificável por construção.
- **Ovo-galinha de presença** — decisão que exige o artefato para escolher quem carrega o
  artefato é circular. A saída é decidir por intenção (prompt complexo) e deixar o loader +
  fallback resolver; o fallback já era honesto (warn + Active), faltava permitir a tentativa.
- **Antes de manter um guard "anti-duplicação", ver o que ele duplica** — aqui ele era
  redundante com o `pio_cap` e, por ser mais restritivo, virava o bloqueio. Guard que repete
  outro guard só adiciona um ponto de morte.
- **Diff no working tree é hipótese até rodar** — o s487 estava sem commit/doc; a verificação
  (check + teste) e a leitura do fallback em `dispatch_expert` foram o que separou "parece
  certo" de "fecha o arco".

## 6. Pendentes

- Aceite do LOO em metal (o cap de 8MB impede o load em QEMU).
- Aceite de runtime do hub: validador s484 + evict s485.
- `#PF storm` pós-PostRuntime / sob falcon3 — pré-existente, IP migrante → watchpoint no writer.
- `target/usb_hw.img` **não existe** — regenerar de HEAD antes do teste metal (fix EP0 s477 é
  runtime-VERIFIED em QEMU; metal = confirmação).
- Mesh re-run com kernel atual (CURAIX-0060); F1 aceite 1h 8c/8GB.
