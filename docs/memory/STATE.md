# STATE - neural-os-core v1.9.99-s451 TEST - Forum multi-AI (AION) + F1.5 PROVADO + v4 checkpoint + sec3/sec6/sec15

#   [s451] FORUM OPCODE/1 (participante AION) + F1.5 PROVADO + v4 checkpoint +
#     instrumentacao de fase F1 + auditorias sec3/sec15 + arquivamento de 36
#     modulos legados.
#     F1.5 PASSOU (runtime QEMU TCG): Boot1 act=gen model-born hash=0x458653425da3b4a5
#     + run(6,7)=52 + ckpt -> power cycle -> Boot2 reuse MESMO hash + act=reuse
#     a=9 b=2 result=82 (SEM act=gen) -> ablacao (pristine) reverte (not_found).
#     Cadeia destravada: (1) OVMF NVRAM corrompido (serial 0B); (2) static
#     duplicado virtio-blk (MODERN_BLK sem leitor vs VIRTIO_BLK_DEV vazio ->
#     backend=RAM silencioso); (3) scan de mount (append_off fail-closed +
#     ckpt p/ file/nvme + replay da cauda); (4) sys/checkpoint 2097242 B >
#     MAX_VLEN (90 B) -> advance_oversized (pula pelo total, nao 512-a-512).
#     v4 checkpoint: SHV4 sem bitmap (footgun de 2MB removido); Tickv 4365312 -> 171008 B.
#     F1 fase: note_infer_stage 1=prefill/2=decode/3=coarse/4=jarbas; corrida
#     4c/4GB estavel ~54min (T+58787, 0 SILENCE/OOM) mas LLM GATED -> stall INCONCLUSIVO.
#     sec3/@explorer + sec15/@oracle: 36 modulos arquivados (git mv, nao delete)
#     p/ docs/archive/dead-modules/ + SINAL (n-sgdb + AGENTS.md) p/ nao re-planejar;
#     vfs/path.rs facadado (pub use k_nano); check_duplication.py honesto.
#     sec6: recover_budget_exhausted + teste.
#     launcher canonico FLAKY (0B; comando replicado via WMI boota 51-112KB);
#     AudioBridge default off.
#     VERIFICADO: cargo build --release -p boot 0 erros; k_nano 282, cortex 126,
#     jarbas 135, hermes 311 + 1 pre-existente (permission_gate). Forum AION-0001..0025.
#     Detalhe: docs/memory/SESSION_451.md

# STATE - neural-os-core v1.9.99-s450 TEST - reconciliacao dos .md da raiz (doc-drift pos-s449)

#   [s450] REVISAO DOS 14 .md DA RAIZ (2 lanes explorer) + tools/measure_repo.py.
#     Drift universal: README/SUMMARY/ROADMAP/TECNOLOGIAS/codemap presos em
#     v1.9.99-s412 / 2026-09-26 e ~169K/~672. Corrigido para s449 / ~214K/~714.
#     Correcoes: README (testes 784/6->3 pre-existentes, ~100->~110 ADRs, link
#     0042-*.md->0042-k2chj-adequacao-boot.md, HW Expert v3->v4); ROADMAP
#     (ADR-0058 Proposed->S1-S4, 5->6 crates, workspace members reais 12, rings
#     k_hal=R1/k_ai=R2/hermes+jarbas=R3); TECNOLOGIAS (licenca MIT->AGPL-3.0,
#     Limine UEFI-only, SDIO 2.794->95.812, blobs 92->90, tags s315/s328->
#     s440/s449, 2.10g retargetado ADR-0082->ADR-0077/0102 porque o corpo e
#     Ring3); codemap (FS ext2/NTFS/Btrfs removidos, heap floor); HOWTO footer;
#     ATTRIBUTIONS URL placeholder; DEAD_WEIGHT_AUDIT status anotado
#     (#1/2/3/5/6 IMPLEMENTED s449, #7 INVERTED). CONTEXT/COMMERCIAL/
#     CONTRIBUTING limpos.
#     CORRECAO ESTRUTURAL: measure_repo.py contava crates/neural-sgdb (repo
#     IRMAO gitignored, 64 .rs) -> 777/252696 inflado. SKIP_PARTS +=
#     "neural-sgdb" -> 714/213629 honesto; marcadores AGENTS/ROADMAP atualizados.
#     VERIFICADO: python tools/measure_repo.py exit 0 (AGENTS/ROADMAP OK);
#     LICENSE=AGPL-3.0; 110 ADRs; 0 modulos ext2/ntfs/btrfs; 90 blobs/13.7MB.
#     UNKNOWN: contagens de prosa (HW Expert v3 vs v4, estrelas, SLA) nao
#     re-medidas; 2.10g (0077 vs 0082) e decisao do maintainer. Zero .rs.
#     Detalhe: docs/memory/SESSION_450.md

# STATE - neural-os-core v1.9.99-s449 TEST - consolidacao do working tree nao commitado (E1-E4 + F1 + AION-storage + dead-weight)

#   [s449] CONSOLIDACAO (41 modificados + untracked desde s441 num so commit):
#     E1-E4 (ADR-0113) + F1 runtime + AION-storage + cleanup dead-weight + codigo
#     de s441-s445. ZERO codigo novo escrito -- verificacao + registro + higiene.
#     E1 formal (mesh p99 + kani) / E2 caps (trust grant/mint/enforce/revoke +
#     CAP_GENERATION global + wasmi cap_gen) / E3 provenance (event-bus/stamp.rs
#     NOVO + bus wire + register_audit_hooks(sha256)) / E4 perf (bench_stats.rs
#     novo + agent-core bench_* + BENCH markers + tools/bench_boot.ps1).
#     HONESTIDADE: as provas #[cfg(kani)] compilam mas NAO rodam no host
#     (x86_64-unknown-none fora do guide) => E1/E2 formal = UNKNOWN.
#     F1 runtime: interrupts_ext pf_storm BSP->reboot_ordered + ramlog_note
#     lock-free; silence_watchdog stage por core; percpu fault_context_is_bsp;
#     compositor PAINT_GAP EWMA; jarvis TSC; infer_queue DECODE_RING.
#     AION-storage (DONO AION -- so se commitou): tickv advance_oversized +
#     replay da cauda pos-ckpt + append_off=size fail-closed + ckpt-on-flush;
#     flash with_flash_dev -> virtio_modern; virtio_modern BlockDevice;
#     virtio_blk legacy-first; self_heal checkpoint v4 SHV4 (sem bitmap 2MiB).
#     Cleanup dead-weight (DEAD_WEIGHT_AUDIT.md, root, versionado): findings
#     1,2,3,5,6 (matrix_learn.rs DELETADO, boot_log/self_learning -> Oneshot,
#     self_heal sem heartbeat falso, input sem USB poll); #7 INVERTIDO (o E3
#     criou o consumidor que o audit nao achou); #4/#8/#9 nao.
#     GATES: cargo build --release -p boot = 0 erros (1m12s). Testes ISOLADOS
#     (-t1): hermes 322/323 (permission_gate PRE-EXISTENTE), cortex 126/126,
#     jarbas 135/135, k-hal 75/75; k_ai abort PRE-EXISTENTE (sgdb bench);
#     tq2_0_gguf_load PRE-EXISTENTE (gguf.rs + teste NAO modificados). A suite
#     paralela mentiu 6 targets (flaky de statics, licao s346/418) -- isolado
#     volta a passar.
#     UNKNOWN: Kani nao rodou; nenhuma frente validada em boot QEMU (host !=
#     runtime); causa do tq2_0_gguf_load nao isolada.
#     Detalhe: docs/memory/SESSION_449.md

# STATE - neural-os-core v1.9.99-s448 TEST - forum ENCERRADO; F1.5 = carimbo (§14/§14b) + braco de ablacao (§7, IMPLEMENTADO/host)

#   [s448] BRACO DE ABLACAO DO F1.5 (IDEA #638, era a peca 100% da lane FREEBU).
#     PROBLEMA: -PreparePristine so fazia snapshot; o comentario prometia
#     "restore before a run". O disco de que cada boot partia era variavel nao
#     declarada -- e no boot 1 ela decide se act=gen prova alguma coisa.
#     EVIDENCIA (medida): pristine (03/10 22:47) lab_state=0 (limpo);
#     disk_qemu.raw (04/10 ~00:57) lab_state=1 com dirents FAT32
#     skill/wasm/oracle_rt_expr_v1 e skill/wasm_prov/oracle_rt_expr_v1 em
#     ~1657 MiB. O confound era REAL. ~3 min depois o mesmo arquivo estava
#     limpo (outra thread restaurou) => o estado do lab e volatil: so um
#     registro tirado NO BOOT responde "de que disco partiu".
#     MENOR CORRECAO: tools/f15_pristine.py (ensure/restore/scan/check) +
#     bloco [7] no launcher (restore OPT-IN, scan SEMPRE) + campos restore=,
#     restore_motivo=, disk_lab_state_before=, pristine_bytes= no sidecar +
#     3 regras fail-closed no parser (sem registro / boot1 com lab_state=1 /
#     boot2 com restore=1). Zero .rs, zero tickv.
#     CUSTO MEDIDO: varredura de 3 GB = 1,6-4,9 s (mmap+find, 64 MiB/bloco)
#     contra orcamento de 900 s; 4 MB no teste = 0,03 s.
#     RESTORE E OPT-IN DE PROPOSITO: target/disk_qemu.raw e estado
#     COMPARTILHADO (outra thread bootando: mtime 00:45 -> 01:00 durante o
#     trabalho) e sobrescrever 3 GB sem pedido destruiria o boot dela. No boot 2
#     o restore e a CONDICAO DE CONTROLE: o run e permitido e o dado fica no
#     log, mas o parser reprova (restore=1 = a persistencia nao pode ser
#     afirmada ali). Log e dado; veredito e conclusao.
#     BUG DE PRECISAO (2a vez no mesmo arquivo): epoch batia errado em 1 s
#     (PS 758 vs Python 757) — `/` no PS e divisao em DOUBLE e o cast
#     arredonda, e Ticks (~1,8e17) passa de 2^53. Formula exata nos dois
#     lados: (($ticks - ($ticks % 10000000)) / 10000000) = truncamento.
#     GATES: fixtures 26/26 exit 0 (+b1_lab_state1, b1_sem_ablacao,
#     b2_restore1, b1_sha_erro); tools/test_f15_ablation.ps1 17/17 exit 0 (bloco [7] REAL
#     sobre discos de 4 MB); tools/test_f15_stamp.ps1 exit 0 (15 campos do
#     sidecar, e restore=0 de proposito para nao tocar no disco do lab);
#     sintaxe OK nos 3 .ps1. O stamp test saiu de target/ (gitignored) para
#     tools/ versionado.
#     UNKNOWN (D2): NENHUM boot de QEMU rodou com -RestorePristine. O braco
#     esta IMPLEMENTADO + verificado em host, nao OBSERVED no metal. Restore de
#     3 GB tambem nao exercitado (teste usa 4 MB).
#     Detalhe: docs/memory/SESSION_448.md

# STATE - neural-os-core v1.9.99-s447 TEST - forum ENCERRADO; F1.5 = carimbo de artefato (§14) + ablacao ausente

#   [s447] FORO ENCERRADO (decisao do maintainer; agentes parando). Funcao do
#     FREEBU = memoria/harness/QEMU/runtime (D5/HUMAN-0005). Retomada completa
#     em docs/memory/RESUME.md (unico ponto de entrada; este bloco e o resumo).
#   SECAO 14 (carimbo do artefato bootado): run-f15.ps1 grava sidecar
#     <log>.imgid (bytes/mtime de uefi.img + disco + probe do literal
#     SKILL_LAB na FONTE e na IMAGEM); f15_parse.ps1 EXIGE o sidecar — ausente
#     ou probe_na_imagem!=True = FALSIFIED (prova faltando nao e zero).
#     Medido: test do carimbo EXIT=0 (uefi 134217728 B, disk 3221225472 B,
#     probe_na_imagem=True); suite run_f15_fixtures 22/22 exit 0; build
#     -p boot com rebuild forcado 2m40s = 0 erros. Zero .rs tocado.
#     REGRESSAO DECLARADA (veredito final medido nos logs reais): boot1 EXIT=1
#     (`a imagem mudou DEPOIS do boot` — sidecar diz mtime 03:36:17Z, o arquivo
#     era 03:40:00Z), boot2 EXIT=1 (`escalate=reuse reason=not_found` + sem
#     sidecar), -Compare EXIT=1. O "boot1 PASS" da s446 esta RECLASSIFICADO e o
#     PASS intermediario que o §14 dava tambem: a evidencia de comportamento
#     (act=gen + run result=52) continua no log, o veredito nao se sustenta.
#     Confirmado do outro lado: uefi.img NAO esta stale no conteudo (7/7 literais
#     do codigo do AION dentro da imagem; o "ausente" era comentario).
#   SECAO 14b (achado no proprio dia, ja fechado): o probe era lido NO PARSE, ou
#     seja, se a imagem fosse reconstruida depois do boot ele provava o codigo
#     novo num log antigo. Sidecar agora grava uefi_epoch (segundos inteiros) e
#     uefi_sha (16 hex de SHA256); o parser recalcula os tres contra o arquivo
#     atual e reprova se divergir. Bug de precisao pego pela suite: a string ISO
#     de mtime perde 1 ULP entre Python e .NET (...651Z vs ...652Z) e reprovava
#     casos legitimos → epoch inteiro (Ticks - 621355968000000000)/10000000.
#     DURABILIDADE: run_f15_fixtures.py chamava target/gen_f15_fixtures.py,
#     GITIGNORED — um git clean apagava o gerador e a suite inteira. Movido para
#     tools/gen_f15_fixtures.py (versionado), com imagem de identidade
#     deterministica e o caso negativo b1_imgid_mudou.
#   Watcher do forum PARADO (PID 17680) no fechamento; religar:
#     `python -u tools/forum_watch.py --watch 480`. Gate tem_substancia (4/4):
#     digest so com substancia, supressao CONTADA (digest_suprimido=N).
#     Meu digest nomeava o OPMUSE e o poll do OPMUSE conta mencao pelo nome
#     dele: eu alimentava o poll que me notificava. OPMUSE tem self-feed
#     (o corpo do poll contem "OPMUSE") e NAO se auto-encerra.
#   Processos do lab mortos no fechamento: launcher 23068 + QEMU 25484 (log do
#     boot cortado em logs/f15_boot1.orphan_cut.txt, 2954 linhas, veredito
#     UNKNOWN). Lock do forum SEM orfao (verificado).
#   F1.5 ABERTO, dono AION: TICKV backend=file. Causa isolada (AION-0009):
#     try_mount_from_ckpt (tickv.rs:397) reconstroi indice so do ultimo ckpt e
#     nunca varre a cauda (append final boot1=74752 vs mount boot2=53760).
#     AION-0010 = P0 com falsificador, em review no ORACLE. NAO tocar em tickv.
#   FALTA no harness: braco de ABLACAO (§7). -PreparePristine so faz snapshot;
#     o comentario em run-f15.ps1:41 promete restore que o codigo NAO executa
#     => o criterio de aceite do HUMAN-0009 nao fecha (IDEA #638). E1/Kani sem
#     toolchain neste host. F1 (writer do wild-write) segue sem dono (#632/633).
#     Git: commits publicados nesta frente = 4ea43987 (watcher), 2a8f95b2 (§14),
#     50ec9eb6 (§14b + fechamento + runbook), 556c8c64 (correcoes de doc);
#     HEAD = origin/main conferido com git ls-remote. Zero .rs em nenhum deles.
#     Working tree suja de 4 frentes (36 modificados + 15 untracked) — mapa de
#     donos em RESUME.md §4; nunca `git add -A`.
#   UNKNOWN: cargo check --release DESTA arvore (o 0-erros e da s446, antes das
#     36 alteracoes alheias); testes de skill_lab; serial QEMU; TICKV
#     backend=file; [RECOVER] no BOOT.LOG; sync n-sgdb (sem tool MCP aqui).
#   Detalhe: docs/memory/SESSION_447.md

# STATE - neural-os-core v1.9.99-s445 TEST - forum CURAIX: recover_count no wipe + reload pos-Tickv; F1.5 aberto

#   [s445] Header do ramlog = 24 B. recover_count u32 no offset 20, mesmo layout
#     do logwriter-efi (HDR_SIZE = 24). mark_done nao mexe no contador.
#     append le o u32, zera os 256 KiB quando magic = NEURDONE (ou outro magic
#     nao-zero que nao seja NEURLOG!), e grava o u32 de volta. Init com magic
#     desconhecido ainda zera o header inteiro, inclusive magic 0 (efeito nulo
#     se o header ja era zero). reboot_ordered sela e chama warm_reset so se a
#     decisao permitir e recover_count < 3. is_bsp esta fixo em true; a exclusao
#     do AP esta nos callers. try_bsp_recover_reset foi removido.
#   reload_persisted_wasm_skills: VFS ausente segue o passe Tickv. O fim da
#     funcao poe RELOAD_DONE=true e RELOAD_SKIPPED=false sempre. with_tickv
#     devolve None sem mount, e unwrap_or_default vira durable=0 (desconhecido
#     reportado como zero). 1a chamada em main.rs (~2495) e ANTES de tickv_smoke
#     (~2744). 2a chamada (~2760) e depois, fora do if.
#   F1.5: hermes::skill_lab::poll_and_run le 512 B em 0x0211_0000. gen_lsk1.py
#     grava blobs de 256 B (LSK1 + NUL; o parse cabe). Aritmetica de bancada:
#     (6^2 + 3*7) - 5 = 52 e (9^2 + 3*2) - 5 = 82, conferida a mao, nao no wasmi.
#     run-f15.ps1 lanca o QEMU e nao afirma PASS. Hash no log = FNV-1a64 dos
#     bytes WASM em skill/wasm/, nao do texto-fonte. Magic diferente de LSK1
#     retorna None sem log e sem CONSUMED.
#   NAO aplicado: patch RELOAD_RETRIED, pad do gerador para 512, warn unico de
#     magic nao-zero, parser que junta os dois logs.
#   MEDIDO no host (antes deste registro, nesta sessao): boot_ramlog 3/3
#     (nao cobre o wipe: PHYS_MEM_OFFSET == 0 sai cedo); skill_loader 4/4.
#   UNKNOWN: cargo check --release desta arvore, testes de skill_lab, serial
#     QEMU, TICKV backend=file, linha [RECOVER] no BOOT.LOG, strings novas no
#     uefi.img. cargo nk nao regenera uefi.img. 1 h sem #PF. Watchpoint.
#     Decisao accepted do forum sem from=HUMAN fica proposta.
#   Working tree ja estava sujo (s441-s444 e target/). Commit do pos-tarefa, se
#     houver, e so documentacao s445. Sem tag. Sem v2.0.0.
#   Detalhe: docs/memory/SESSION_445.md

# STATE - neural-os-core v1.9.99-s444 - A lane anti-frag USA o histograma (Dust x Benign)

#   [s444] A s443 criou o histograma de gaps; a lane da s442 decidia so por
#     `free`/`largest`. Furo honesto: ela pergunta QUE fragmenta e nao DE QUE
#     JEITO. Em fragmentacao BENIGNA (poucos gaps, todos grandes) gastava job LLM
#     + HITL humano numa acao sem efeito — a pior forma da loop de feedback da
#     s429-lab. `anti_frag::FragKind` (`Dust`/`Benign`/`Mixed`, limiares 60%/20% de
#     gaps <8KB, funcao PURA `frag_kind(hist)`): benigno vira observe-only ANTES
#     de chamar a IA; poeira segue para o LLM com o histograma no snapshot;
#     histograma ausente (log pre-s443) => `Mixed` (nao afirma o que nao mediu).
#     O slog passou a dizer a forma: `fragmentacao Dust detectada (...)`.
#     `mod anti_frag` agora **28 testes** (+6: 3 de forma + 1 benigno-nao-chega-ao-LLM
#     + 1 poeira-segue + `limiares_sao_exatos` com 60/20/30% exatos) e
#     **mutation 13/13** (as 8 da s442 + M9..M13: gate removido, limiar de poeira
#     60->20, limiar de benigno 20->0, `total==0` sem Mixed, `>=` invertido).
#     Licao de processo: M13 reproduziu uma corrupcao REAL que entrou no arquivo
#     durante a bateria de mutacao da s442 e passou pelo `diff -q` — restauracao
#     por **SHA-256** + reexecutar a suite depois de qualquer bateria. M10 vivia
#     com fixture que nao distinguia os limiares (67% de poeira = Dust tanto com
#     60 quanto com 20): teste que nao falha quando o numero muda nao fixa o numero.
#     Validação: anti_frag **28/28**, hermes **315/316** (falha PRÉ-EXISTENTE de
#     `permission_gate`, s440), `cargo nk` 0 erros. Detalhe: SESSION_443 §9

# STATE - neural-os-core v1.9.99-s443 - 2º consumer no TALC (ContextWindow) + HISTOGRAMA de fragmentação

#   [s443] A s439 roteou o KV cache. A auditoria achou o 2º consumer pesado de
#     LONGA VIDA ainda no bump: `k_ai::ContextWindow` (`role`/`content`/
#     `system_prompt` eram `String`). Não é "só" estar no bump — é VAZAMENTO
#     MONOTÔNICO: o bump nunca devolve memória (dealloc = no-op) e a janela tem
#     CHURN (`maybe_compact` remove mensagens), então cada mensagem descartada
#     retia seus bytes para sempre. Rotear aqui é conserto de vazamento.
#     `k_nano::allocator::TalcBuf`: buffer POSSUÍDO com `Drop` que devolve o
#     chunk ao TALC (free real), fail-closed (`None`/`false`, conteúdo intacto),
#     `Send` sem `Sync`. `grow` NÃO usa `realloc` de propósito (lição s434b: para
#     chunk TALC-resident, `HybridAllocator::realloc` chama `oom()` = derruba o
#     kernel); é alloc+copy+free, que devolve NULL honesto. `add()` passou a
#     retornar `bool` (recusado ≠ "guardei") + `talc_bytes()`.
#     s443b: **histograma de fragmentação** — `talc_walk_bins` conta gaps por
#     faixa (`TALC_HIST_BOUNDS` <8KB/8-64KB/64-256KB/256KB-1MB/1-16MB/>=16MB),
#     sai no slog (`hist=a/b/c/d/e/f`), no snapshot JSON (o LLM ve a distribuição)
#     e em `HeapObserve`. Totais dizem QUE; o histograma diz DE QUE JEITO — é o
#     que decide a ação certa (poeira x poucos gaps grandes). `tools/
#     talc_frag_report.py` lê um log real e classifica (dust-dominant x benigno),
#     com teste de contrato 3/3 que monta a linha do template real do slog.
#     Validação: k-nano **276/276** (-t1), k_ai context_window **5/5**,
#     hermes 309/310 (falha PRÉ-EXISTENTE de `permission_gate`, s440), cortex
#     126/126, `cargo nk` 0 erros; `hist=`/`TalcBuf` no kernel.elf. PRÉ-EXISTENTE
#     PROVADO: suite completa de k_ai aborta em `sgdb::bench::d_series_100k`
#     (STATUS_STACK_BUFFER_OVERRUN) — revertendo meu arquivo para HEAD o abort é
#     idêntico. Detalhe: SESSION_443.md

# STATE - neural-os-core v1.9.99-s442 - Anti-fragmentacao do TALC: IA-observa -> IA-age (HITL + efeito medido)

#   [s442] A s435 deu o NUMERO do TALC e a s433 deu o CANAL LLM; faltava o MEIO:
#     a fragmentacao era `Observe` e a "acao" do LLM era texto livre de toast
#     que nunca chegava a executor. `hermes::anti_frag` fecha o ciclo:
#     1. **Vocabulário FECHADO** (`AntiFragCmd`): o LLM escolhe uma CHAVE de um
#        catalogo com seam real -- `evict_kv` (H2O no KV global), `drop_kv`
#        (`kv_cache_reset`), `reset_moe_cache` (arena), `no_action`. Texto livre
#        e' rotulo de toast: nao existe caminho de codigo que vire comando.
#     2. **HITL obrigatorio**: Escalate no ApprovalGate (`/approve <id>` ou
#        `/deny <id>`); TTL de 10 min (aprovar depois = agir sobre snapshot
#        velho); `try_lock` com limite de 64 (nunca `lock()` atravessando
#        publicacao no bus -- como o s438 morreu).
#     3. **Efeito medido**: a amostra do TALC vem ANTES de agir; `verify` so
#        chama de `Improved` se o MAIOR GAP cresceu, `Worse` se o uso subiu,
#        `NoChange` se nem isso; `partial=1` nao afirma nada. Delay de 3s antes
#        de medir (cache do TALC e' 2 Hz: medir cedo = comparar a amostra
#        com ela mesma = falso negativo fabricado). Apos agir, cooldown
#        de 5 min -- hipotese errada nao vira metralhadora de `drop_kv`.
#     Sem modelo, sem headroom, TALC sem claim ou amostra parcial: segue
#     observe-only (a lane e' IA+HITL, nao heuristica automatica). Seams novos em
#     cortex: `kv_h2o_evict_global`/`kv_global_pages`/`kv_global_len` (`None` =
#     KV em voo = no-op honesto, fail-closed).
#     `mod anti_frag` (22 testes) + **mutation 8/8** (texto-livre-vira-cmd,
#     deny-executa, TTL, cooldown, largest-parado, clamp, parcial).
#     Verificacao: hermes anti_frag 22/22, hermes 309/310 (falha PRE-EXISTENTE de
#     `permission_gate`, isolada passa), cortex 126/126, `cargo nk` 0 erros
#     (41.9s); strings `fragmentacao detectada`/`evict_kv`/`hub.antifrag`
#     presentes no kernel.elf. PENDENTE: prova em QEMU (log `fragmentacao
#     detectada` -> `HITL #` -> `/approve` -> `efeito:`). Detalhe: SESSION_442.md

# STATE - neural-os-core v1.9.99-s441 - Fuzz host de `talc_walk_bins` (gap-lists corrompidas)

#   [s441] `talc_walk_bins` (k_nano/allocator.rs) é o ÚNICO leitor de metadado do TALC
#     que NÃO confia no talc — `node < base || node >= acme` não
#     bastava. DOIS guards novos, ambos load-bearing (provado por mutation 5/5):
#     G1 `node + TALC_GAP_MIN_FOOTPRINT (24B = MIN_CHUNK_SIZE do talc 4.4.3) <= acme`
#     — sem isso, `node` perto de `acme` lia `size` FORA do span (`node+16`), ou seja
#     `#PF` num span demand-paged (a classe rotativa que a s438 caçou 2 sessões);
#     G2 `node % align_of::<usize>() == 0` — sem isso, `next` desalinhado gerava
#     `read_volatile` desalinhado (UB; abort do check de debug do Rust).
#     `mod talc_walk_fuzz` (13 testes): scratch `[span | redzone]` com `size=32` no
#     redzone (detecta leitura fora do span sem página de guarda); self-loop, ciclo
#     de 2, 128 bins em loop (CAP é global, não 128×), node fora do span, size
#     gigante/saturante/0, 4000 gaps válidos com soma > span, desalinhado, bins nulo,
#     span vazio; fuzz dirigido 400 casos + cauda 2000 (xorshift determinístico) com
#     budget de 2s por walk = prova de "não pende". Contrapeso obrigatório:
#     `walk_over_real_talc_is_never_partial` roda `Talc::claim`/`malloc`/`free` DE
#     VERDADE e exige `partial=0` — os guards não podem rejeitar o allocator de
#     produção (senão a linha `talc` do HUB vira "nunca medida"). `free` passou a
#     `saturating_add`: inalcançável hoje (wrap exigiria span de ~4.5 PB) e
#     documentado como tal, não como testado. M4 (tirar o saturating) PASSA — por
#     isso ele é cinto-e-suspensório, não cobertura. Verificação: k-nano **265/265**
#     `-t1`, cortex 126/126 `-t1`, `cargo nk` **0 erros**; hermes 287/288 com a falha
#     PRÉ-EXISTENTE de `permission_gate` (confirmada de novo via `git show HEAD` —
#     já registrada na s440). Detalhe: SESSION_441.md.

# STATE - neural-os-core v1.9.99-s440 - Falcon3-3B 1.58bit: fim do gibberish (slim 32 + bypass Falcon + BPE 131k) + wedge terminal do InferQ

#   [s440] Falcon3-3B 1.58bit — saída sem sentido (`andsfaqt...` determinística)
#     NÃO era o tokenizer: `encode_chat_frame` (cue Llama-3 6 tokens) colapsava o
#     prompt p/ `prompt_len=6`; o fix s437 só trocou 6→8 por causa de `MAX_CHAT=8`
#     + duplo-slim. Fixes (cortex.rs/bpe.rs/infer_queue.rs): slim 8→32 + bypass
#     Falcon; vocab 131072 (`min(128000)`→`cols`, `recent Vec<u16>`→`Vec<u32>`,
#     remove filtro `t>=128000`); argmax puro no path Falcon (sem score_piece/
#     weather/coherence); contrato `gibberish_stop` (rep 4-gram>0.6 || distinct-2<0.2
#     || piece_len<3) → `stop=gibberish` nunca vai a TTS; `MachineCtx{intent_id,
#     slots,ctx_ids}` (fala-máquina ao lado do texto, path texto byte-igual).
#     Wedge terminal do InferQ: `slice_stall id=2 elapsed_us=30003962 (budget=500000us)
#     — wedge, terminal` no prefill 512 toks (matmul 512x3072x3072 ~14s). Fix:
#     chunked prefill 16 toks + yield + budget hv-gated (`slice_budget hv=WHPX
#     budget_us=120000000 sandbox=1`) + stall persistente aborta o JOB
#     (`stop=slice_budget`), nunca a fila; `set_prefill_chunk_toks` tunável
#     (clamp 1..64, default 16). Plano 5 camadas (ora-2): rejeitados 1B (AVX2 já
#     existe em bitnet_w2a8.rs, host-only), 1C (pinning/budget 50ms), 2B (arena
#     512MB/agente), 3B (IP estático 192.168.100.50); adaptados 1A (chunk tunável),
#     2A (RO 2MB loader + get_or_mmap_expert), 3A (flags VirtIO lab-only), 4 (flush
#     oportunista TICKV); adotado 5 (skill cache WASM pré-LLM). Implementado+wired:
#     2A `map_loader_region_ro` (main.rs boot scan) + `get_or_mmap_expert`; 4
#     `flush_idle` (main.rs idle closure) + high_water; 5 `skill_cache_try_register`
#     (agents.rs pré-LLM, era DCE'd). BPE: `models/bpe_vocab.bin`/`target1/bpe_vocab.bin`
#     eram SP32 32K (vocab_n=32002) — ERRADO p/ Falcon3; gerado `target/falcon_bpe.bin`
#     via `tools/export_bpe_bin.py target1/falcon3/tokenizer.json` (vocab_n=131072
#     bos=10 merges=128810) e copiado p/ `target/bpe_vocab.bin` (canônico do `find_bpe()`).
#     Verificação: `cargo check --release` 0 erros; cortex 126/126, k-nano 251/251,
#     hermes 287 + 1 falha PRÉ-EXISTENTE (`permission_gate::test_risk_level_classify`,
#     provado via stash: 282/1 sem o diff). QEMU 8c/6GB WHPX (log
#     boot_whpx_20261003_182030_8c_or2.txt): `loader RO ... 1024x2MB ro skip=0`,
#     `skill_cache miss name=hw_pnp_pci_bridge...`, `prefill_chunk 1/32→3/32` sem
#     wedge, BPE 131072. Imagem HW: `PACK_LLM=all python tools/build_image.py --hw
#     --unified --size 15360` → `target/usb_hw.img` (~14GB, 4 Falcon ×2 aliases +
#     BGE/E5/RERANKER/RUSTCDR3/AGENT/LEARNER/PIPER/STT/VISION/HWEXPRT + firmware +
#     BPE 131072).

#   [s439 · concorrente a s436] real-HW unlock (ROCEKT: Intel Core 7 240H /
#     RTX 3050 6GB / 16GB / SSD atras do VMD / WiFi MT7925). Boot real OK (8 fases,
#     UI viva, fault:none). Fixes: (1) GPU VRAM = BAR1 — detect.rs varre os 6
#     dwords e pega o maior BAR >=64MB (vram: n/a antes); (2) forja WASM a quente
#     LIGADA (self_evolve publica TOPIC_SKILL_GEN_REQUEST -> HermesAgent -> LLM
#     op-IR -> evolve::promote_model_text_to_wasm model-born; hw_pnp/bei sem
#     dummy); (3) VMD_STAGE no HUD + linha storage com NVMe; (4) USB warm port
#     reset 100->500ms + CC/to no HUD. Build 0 erros; hermes 282/283 (1 fail
#     pre-existente permission_gate). Commits cb51890d/407cb31f/f3b430cf (main).
#     AWAITING: WiFi MT7925 (sem mt76), NVMe VMD (VMD_STAGE aponta o passo),
#     dGPU BAR lane (precisa ReBAR p/ caber o modelo).

#   PISTA ATIVA: s436 — watchdog de silêncio implementado + validado sem falso-positivo;
#     bughunt do stall pós-a2_proof-done segue aberto (s436b).
#     k_nano::silence_watchdog: timer IRQ (segue disparando em spin com IF=1) compara
#     idade desde a última emissão de log (note_log_emit nos choke points únicos:
#     dispatch_bytes normal+NESTED + buffer_log) contra 60s; estourou → [SILENCE] única
#     com stamps de todos os cores (irq c0=idade do timer; prog cN=idade do progresso —
#     core com idade crescendo = sem progresso), heap/budget, APs, #PFs, última exceção.
#     Dump via interrupts::puts LOCK-FREE (o spinner pode estar segurando o lock do
#     serial). Observe-only (lição s429-lab); rate-limit 1 dump/10s (cadência OOM-HALT);
#     gate boot TIMER_TICKS≥1800. Progress por core: ap_idle_loop + heartbeat pós-tick
#     (bin); guard 1º timer IRQ evita ler gs:[8] antes do PerCpu. Limite honesto: spin
#     com IF=0 no core do timer mata o watchdog junto → QEMU-monitor/watch_corruption.
#     Validação: k-nano 249/249 (-t1, 5 novos); cargo nk 0 erros; strings no uefi.img
#     ([SILENCE], log parado ha, | irq:, | prog:, dump#); QEMU 8GB/8c log 110255: vivo
#     T+83156 (~20min), zero [SILENCE] (sem falso-positivo), zero OOM — stall do lab
#     s435 (T+23334, 2/2 boots) NÃO reproduziu nesta run; disparo real pendente.
#   PISTA ANTERIOR: s438 — QEMU 8c/8GB 1h (bit-engine). cargo clean 71.2GiB + build
#     from-scratch 0 erros (incl. nsgdb 1.3: bridge resolve_conflict->ResolveOutcome).
#     10 fixes de #PF/freeze verificados: BeiState Arc guard (all_ptrs_valid),
#     realloc min(size,new), flood cap-check+throttle, EventBus TicketLock bounded
#     (QEMU-monitor confirmou o spin), scheduler latch budget 50ms, process_wakes cap,
#     hook ptr range, #PF handler is_page_present, HDA bar==0, park observavel;
#     arnes tools/watch_corruption.ps1. NAO fechou o GOAL: #PF rotativo multi-site
#     (0x6/0x13c/0x7ee00001/0x42d) pos-jobs e o park do BSP (loop{hlt}) amplifica p/
#     freeze. 2c inconclusivo (SMP-1-AP barrier pending=1 done=0). Bloqueio: wild-write
#     -> watchpoint no writer.
#   PISTA ANTERIOR: s437 — resposta degenerada do LLM Falcon3 (log HUB triage). Causa-raiz:
#     `bpe::encode` para Falcon3 (ByteLevel 131k, sp32=0) caía em `encode_chat_frame`
#     — frame-cue Llama-3 fixo de 6 tokens (`[bos,1919,eot,128006,78191,128007]`) que
#     NÃO tokeniza o prompt → todo job via o mesmo input e a saída degenerava no mesmo
#     gibberish determinístico (`prompt_len=6` p/ 581B; logs boot_mon:3708 / boot_a2proof).
#     Fix: `is_falcon_bytelevel` (bos=10) + `encode_falcon_chat` (BOS + template Instruct
#     `<|user|>…<|assistant|>` via ByteLevel real; 26 tokens no tokenizer HF) + `encode`
#     roteia; `slim_prompt_tokens_for_heavy` mantém a cauda (não a cabeça). Telemetria:
#     `recipe`→Sev::Ok e pressure pelo headroom COMBINADO (bug: guarda `talc_cap==0` do
#     s431 tornava o warn proativo inalcançável). Validação: cortex 118/118, check 0 erros.
#   PISTA ANTERIOR: s435 — telemetria de uso REAL do TALC (idea #630 residual, monitorar
#     fragmentação em runtime de horas). talc_walk_bins percorre os gap-nodes dos 128
#     bins do talc 4.4.3 (layout confirmado no fonte; free = soma dos gaps, used = span−free,
#     largest_free = maior gap, gaps = nº fragmentos; CAP 4096 + bounds-check → partial=1
#     honesto). Cache 2 Hz (HUD/hub_triage) sob o lock do Talck, seed pós-claim; headroom
#     agora soma o FREE medido (span inteiro aposentado — HeapObserve +5 campos talc_*).
#     HUB HEALTH linha `talc` (21ª): u{}/{}M lg{}M g{} com Warn de fragmentação
#     (free≥256MB e largest×4<free). hub_triage: snapshot JSON + vereditos Observe
#     (fragmentado / metadata parcial) + slog `ok talc u..f..lg..g..` (regra 419).
#     Validação: hermes 275, k-nano 244, jarbas 134 (-t1); QEMU 8GB/8c log 182016:
#     `ok talc u0M f6911M lg6911M g1` telemetria viva 1/min, zero OOM, sistema vivo.
#   🔴 DESCUBERTO NO LAB LONGO (s436): stall silencioso DETERMINÍSTICO sem OOM — 2/2
#     boots (logs 183000/185325) congelam o log em T+23334/T+23437, QEMU vivo ~1 core
#     em spin, SEM OOM-HALT (não é oom()) e sem #PF novo. Contexto: bump no teto 2030MB
#     + a2_proof id=2 done (1º decode ok tok=2305 out_len=4, slow_slice n=22) + última
#     linha `[Log] [JARBAS] JARBAS:  and` (eco do Jarbas). Suspeito: path de eco do
#     Jarbas (log/serial lock) no cleanup pós-job; carimbar enter/exit do tick + slog
#     no echo é o próximo passo.
#   PISTA ANTERIOR: s434 — causa-raiz do OOM/TALC infer_worker (idea #630, 3 sessões):
#     realloc de chunk BUMP-residente usava o default GlobalAlloc::realloc (alloc novo
#     SEM overflow do TALC) → NULL com janela cheia sem tocar os 6911MB → oom() cego.
#     Fix: realloc bump→híbrido (TALC dá o espaço novo). Gap 2: Talck::realloc chama
#     malloc interno (NULL de chunk TALC-residente sem counter) → instrumentado.
#     Fail-closed de classe: oom() saiu do loop{hlt} → spin + heartbeat [OOM-HALT] 10s
#     (stall silencioso quebrado; revelou N cores parkados). Validação: 0× OOM/TALC em
#     ~16min (T+57771, recorde; morria em T+34k), bump no teto com sistema vivo.
#   PISTA ANTERIOR: s433 — lane LLM no hub_triage: Propose submete o snapshot HUB\0+JSON ao
#     LLM (InferQueue, reply HUB_TRIAGE_LLM, prompt com instrução determinística), parse
#     sem serde, e publica a ação GERADA via HITL toast; modelo ausente/recusa/timeout 60s
#     → fallback heurístico (s432); `{}` = declínio honesto (sem toast); gates headroom em
#     publish (fail-closed s430) e submit (should_try_llm); anti-loop reserva fp no submit.
#     QEMU 8GB/8c log 132152: submitted id=4 → fallback-timeout honesto → HITL → MoE → LLM;
#     dedupe cooldown provado. RESIDUAL 🔴: stall silencioso pós-OOM/TALC infer_worker
#     (3ª sessão; log congela T+33860, QEMU vivo em RAM) → idea #630 instrumentar.
#   PISTA ANTERIOR: s432 (6 lanes + hub_triage) — TALC claim do BUDGET COMPLETO (6912MB em 8GB) em VA
#     própria 0x400000080000 (fora da janela wrap do bump, demand-paged custo
#     zero). Causa-raiz do OOM da foto: TALC span fixo 512MB estourava quando o
#     bump chegava ao teto 2030MB, com RAM física 70% livre. Headroom combinado
#     bump+TALC; BUMP_BUDGET_CLAMPED separado do budget real. TALC_SPAN_END
#     ANTES do claim (size-tag no fim do span demand-pageava fora do range).
#     Validação: 13,6min, 0 OOM/heap-fail, bump cheio 2030MB e sistema vivo.
#     LIÇÃO: cargo nk não regenera uefi.img — cargo build -p boot obrigatório.
#   PISTA ANTERIOR: s431 (TALC claim) — Lab QEMU 8GB/8c: goal 5min na UI batido (rodada 10:
#     14,8min runtime, UI viva, 1 storm contido por park, OOM final honesto).
#     Fixes: logger no-op p/ crate `log` (LOGGER NULL deref cr2=0x18); storm
#     park por IP (fail-closed de core); watchdog slice 30s medido no alvo;
#     deadline no-progress; gates headroom 48→128MB; emotion alloc-free; BEI
#     guards c/ re-check bounded; escalada I5:boot_log observe-only. 0 erros.
#   PISTA ANTERIOR: s429 — VMD visão guest: offsets SHDW não-nativos NÃO abortam
#     mais. vmd.c provado: offset aplica-se a RECURSOS (bus = cpu − offset ⇒
#     janela BUS começa em host_phys), NUNCA a DMA de RAM (upstream identidade
#     no guest). Tradução real = DOWNSTREAM MMIO: BAR do filho (bus addr) →
#     cpu = bus + offset via MEMBAR mapeada UC (translate_bar puro;
#     child_bar_cpu; probe_at_mmio_va no NVMe). k-nano 240; 0 erros.
#     AWAITING_HW: nvme ok=true via=vmd guest no notebook.
#   PISTA ANTERIOR: s427 — loader-VRAM (`k_hal/src/gpu/loader_vram.rs`): .bitnet v6
#     do Falcon3-1B lido do FAT por CLUSTERS (callback, zero Vec do blob) e os
#     packed das layers (126 matrizes, ~256MB) vão DIRETO pra BAR. Lane VRAM
#     ativa ANTES do load do heap; `on_model_loaded` vira no-op quando
#     `loader_resident()`. HONESTO: `load_llm_v6` ainda copia os packed pro
#     heap no parse — liberação REAL do heap = stub no parser (residual
#     ADR-0112). k-hal 74, k-nano 237; 0 erros.
#   PISTA ANTERIOR: s426 — card `hints_card` (ID 8003, F11): telemetria H3 ao vivo
#     direto dos statics (stage/fwd/n/vram resid) com affordance do critério de
#     aceite (fwd<100µs, n crescendo); refresh 2 Hz idempotente por id; host sem
#     BAR = off + unknown (não finge). jarbas 123/123, k-hal 71; 0 erros.
#   PISTA ANTERIOR: s425 — persistência do BOOT.LOG visível na UI: linha `bootlog`
#     no HUB HEALTH (HUB_ROWS 20) via `boot_logger::hub_log_line()` — ok n<N>
#     / fail <backend> <razão> x<streak> / fail sem-backend / pre-fat / n/a.
#     Statics LAST_FAIL_KIND/LAST_TRY_BACKEND anotados nos paths de falha;
#     SysInfoAgent usa a MESMA string no slog (sem dual-truth). k-nano 237,
#     jarbas hub 7/7, hermes 257; 0 erros.
#   PISTA ANTERIOR: s424 — evidência de boot nunca mais se perde: (1) kernel
#     `overwrite_boot_log` não aborta em IoFail de UMA partição — continua p/ a
#     ESP (contrato OverwriteResult intacto); (2) build embute BOOT.LOG raiz
#     pré-alocado 256KB na ESP (mesmo mecanismo do volume de dados, zero FS
#     novo); (3) fix de corretude do write do dirent (setor real do entry).
#     Validação: ESP de teste com dirent BOOT.LOG 256KB na raiz. k-nano 237.
#   PISTA ANTERIOR: s423 — wake honesto `wake_to_d0` (`k_hal/src/gpu/gpu_power.rs`):
#     a dGPU dorme em D3 no boot (SESSION_260) e os gates do vram/nvidia matavam
#     o H3 e o lane VRAM em TODO notebook. Prova de vida ANTES do write PMCSR
#     (D3cold não recebe write às cegas), budget TSC 10ms, re-scan persistido no
#     GpuInfo; `wake_all` pós detect_all + backstop idempotente nos gates.
#     Gates: 0 erros; k-hal 71. HW lab pendente: GPUPWR woke + hints Resident.
#   PISTA ANTERIOR: s422 — Intel VMD binder (`k_nano/src/vmd.rs`): o NVMe de notebooks
#     Intel RST vive num domínio PCI SECUNDÁRIO (8086:a77f sem driver no lab; NVMe
#     invisível ao CF8/CFC). CFGBAR=ECAM dos filhos + busn_start VMCAP/VMCONFIG +
#     scan flat + enable via config MMIO; SHDW nativo ⇒ DMA físico direto;
#     `NvmeDriver::probe_at_mmio` extraído; fallback no probe storage → MESMO
#     global NVME_DRIVER (bin intocado). Gates: 0 erros, k-nano 237. HW lab
#     pendente: `nvme ok=true via=vmd` + BOOT.LOG persistindo no NVMe.
#   PISTA ANTERIOR: s421 — lane VRAM POR SEQUÊNCIA (layer*7+slot) corrige os 3 bugs de
#     corretude do s418 (SHAPE_INDEX servia layer 0 p/ todas; GEMV coluna trocada;
#     escala única p/ m>1). Upload sem dedupe + pré-checagem INTEIRA (parcial = off
#     honesto). Dispatch seq no apply_one_layer (7 matmuls) + proteção de shape.
#     HONESTIDADE: heap NÃO encolhe (bump sem free; pesos já no boot) — liberação
#     real exige loader-VRAM (residual, lab). QEMU 8G/6c: lane off honesto (sem
#     aperture), CPU ladder intacta, zero #PF, grow máx 1536MB. s420c: consumidor
#     HINTS (tint orb/dock/cards); s420b: train_hint_mlp + loader HINT.BIN;
#     s420: gate fail-closed headroom_low 128MB no slice (zero #PF em produção)
#   PISTA ANTERIOR: s418 — BAR Compute (pesos W2A8 residem na VRAM via BAR, GEMV host lê aperture,
#     lane VRAM no dispatch; StreamsW2a8/ComputeDevice = upgrade; lab GTX 1050 = residual)
#   PISTA ANTERIOR: s410m — forget cognitivo HITL (/forget) + leitura de conflitos (/conflicts)
#     via ApprovalGate Escalate (skills sgdb_forget/conflict_resolve); tombstone Superseded
#     antes do delete físico; fail-closed NSGDB down; registry pendente cap 32 FIFO;
#     s410m-b: elo AUDIT_OP_FORGET na hash-chain sys/audit/ do SGDB (audit_forget upstream
#     + audit_verify_nsgdb) — evidência do esquecimento sobrevive à memória apagada
#     s410m-c: put_kv sem dual-write — sync_write roteado: md/ no-op (domínio put_doc);
#     não-md via import_record (sem tick do clock local, indexa ART/BQ/lexical)
#     (s410l CRDT merge_remote; s410k mesh RX batch; s410j compact batch; s410i interop TKLV)
#   PISTA ANTERIOR: s406 heap auto-fracionado advisory (commit 2b650c56)
#   Não declarar v2.0.0

## BAR Compute (s418, ADR-0112) [OK — stage Mapped]

| Item | Estado |
|------|--------|
| vram_stream.rs (ring + canário golden TSC) | OK - 4 slots × 8×9216 i8; telemetria lock-free |
| bar_compute.rs (upload + GEMV via BAR) | OK - upload determinístico, SHAPE_INDEX, read_volatile + prefetcht0 |
| Lane VRAM no dispatcher (cortex::compute) | OK - antes do GPU device; N_VRAM telemetria |
| Seam upload pós-load (set_model) | OK - register_vram_upload_hook + layers_snapshot |
| Honestidade | OK - stage Mapped sem note_gpu_compute; iGPU=Off (DRAM compartilhada) |
| Compat vendor | OK - NV BAR1 / AMD BAR0=VRAM / iGPU Off / D3 recusa (s260) / QEMU VGA fail-closed |
| check bare-metal + testes | OK - 0 erros; k-hal 232, cortex 105, k-nano 232, hermes 257 (t=1) |
| Lab: round-trip GTX 1050 + decode longo | residual (RAM/estabilidade, NÃO tok/s) |
| StreamsW2a8 + ComputeDevice (CE/SDMA/BCS) | upgrade path ADR-0112 §7 |

## Federation de saúde + OOM fail-closed (s417) [OK parcial]

| Item | Estado |
|------|--------|
| Diagnóstico freeze/#PF | OK - 1 OOM (bump sem free → NULL → #PF → hlt no AP); carimbo ≠ culpado; crash site migra com fix |
| Fail-closed infer_queue | OK - claim recusado sob headroom crítico (64MB) + job em curso → Finishing honesto (10/10) |
| Fail-closed MoE lifecycle | OK - gate `heap_headroom_critical()` em bei_tick + flush_merges/splits/births |
| Freeze UI (tick lock) | OK - `try_with_agent_tick_lock_ms(2)` display first+boost; SCHED gap 10761→3351 |
| Hybrid allocator | PARCIAL - bump-first+TALC overflow; TALC-first stallou boot; diagnóstico `ALLOC null` pronto |
| Modelo puro máquina/frota | OK - worst-of NO_GO>UNKNOWN>GO, machine_verdict_json, parse no_std, fleet_worst (10/10) |
| Consolidação hermes | OK - MACHINE_HEALTH 1Hz + machine_prompt único + TX MCH\0 (cooldown 10s) |
| Fleet no Master | OK - fleet_health RX MCH\0 (array 16 slots, evicção), FLEET_HEALTH + escala 1×/incidente (4/4) |
| QEMU validação | OK - MCH TX 25 pubs, zero PF_DBG, a2_proof completo; boot passou do stall anterior |
| Stall silencioso pós-teto | OK - SmpMmGuard (s419) + gate headroom_low 128MB no slice (s420); validado QEMU: a2_proof completo, grow máx = teto, zero #PF |
| Federation e2e dual-node | ⏳ - RX de frota com MCH alheio real + FLEET_HEALTH no HUD |

## KV-INT8/paginado (s414) [OK]

| Item | Estado |
|------|--------|
| `KvCache` INT8 (bloco 64, escala f32) | OK - `k`/`v` -> `Vec<Vec<i8>>`; `k_all`/`v_all` dequantizam (assinatura + guard SESSION_351 intactos) |
| Ganho de memoria | OK - **3,76x** (nao 4x: a escala f32 por 64 valores conta) |
| Storage paginado | OK - `KV_PAGE=4096` i8; paginas nunca realocam (evita memcpy de ate ~152 MB) |
| Evidencia (P0) | OK - KV do 1B ctx 4096 = **603.979.776 B > modelo (544 MB)**; atencao ~2% do prefill |
| Runtime (P3) | OK - log `KV kv_mem ... int8_used=KB int8_alloc=KB f32_would_be=KB` no fim do prefill |
| Medicao no lab (`attn` por ctx 1K/2K/4K) | residual |
| Testes | OK - `cargo test -p cortex --lib kv` 6/6; `cargo check --release` 0 erros |

## Perf SMP (s413) [OK]

| Item | Estado |
|------|--------|
| T1 sync do SMP | REFUTADO - o "4,2 ms" era o **log de entrada**; dispatch real **~60 us** (TSC depois do log) |
| T2 threshold tiny f32 | NEUTRO (< variancia do host); revertido; economia real ~0,5% do prefill |
| T5 fusao SwiGLU in-place (`cortex.rs`) | inconclusivo (confundido pelo host); mantido por ser simplificacao (menos alloc/copia) |
| Bancada de perf | variancia run-to-run 7-8% -> efeitos <10% precisam host controlado ou n repeticoes |
| ADR-0111 KV-INT8/paginado | PROPOSED (IDEA #613) - KV do 1B (604 MB) **> modelo** (544 MB); atencao ~2% do prefill -> ganho e memoria/ctx longo; destrava #617 |
| T3/T4/T6/T7 (worker) | bloqueados na bancada (efeito esperado <10%) |

## Perf SMP - tile do worker ternario (s412) [OK]

| Item | Estado |
|------|--------|
| Tile de colunas (formula de tile de LINHAS) | OK - `clamp(8,256)` -> `clamp(32,256)`; 8 col = 2 B/linha de cache (64 B) |
| Worker k=2048 n=8192 (Falcon3-1B, m=8) | OK - 204 -> **105 ms** (1,9x); prefill 20,9 -> **12,7 s** (1,65x) |
| Sweep do piso | OK - 8/16/32/64/128 -> 204/154/105/130/127 ms (32 = fit L1: 16 KB strip + 8 KB x) |
| AVX2 unpack 2-bit | REFUTADO - **12x PIOR** no alvo soft-float (nao emite AVX2; f32 vira libcall); deletado |
| Sync SMP ~4,2 ms/dispatch | conhecido - custo FIXO (ternario E f32); bypass no ternario medio = regressao (~4x worker) |
| Threshold nos tiny f32 (k=8 n=256, 14x ~4,2 ms) | residual (nao implementado - decisao ponytail) |

## A2 proof - marco zero (s411) [OK]

| Item | Estado |
|------|--------|
| FALCON3-3B 1 token real (QEMU 6G/4c WHPX) | OK - `a2_proof done id=2 toks=1 prefill_us=69042824` (BOOT.LOG em disco) |
| Hang do prefill (travava em layer 1/22) | OK - lost wakeup do AP idle (`ui_yield` + `hlt` sem IPI); bounded retry + TOCTOU (`ap_work.rs`) |
| Mesh 2 nos | OK - converge (node_id 2/3, Master/Memory, peers=1); netmode `0x16400000` |
| Token legivel (BPE) | OK - BPB1 Falcon3 131072 @0x150000000 -> `JARBAS: quad` (era `)`) |
| Barrier SMP sem deadline | OK - deadline 60 s + fallback single-core honesto |
| posture FAIL de 1 amostra | OK - `POSTURE_MIN_SAMPLES=4` -> `n/a` |
| 2o token (forward de decode) | residual (max_gen>1) |
| prefill 3B ~7 s/layer (SMP) | alavanca: W2A8/GPU/AP-IDT |

## Lab vivo (s410)

| Item | Estado |
|------|--------|
| GOAL1 6c/5min (`tools/goal1-6c-5min.ps1`) | ✅ PASS (5115 ok / 236 warn / 2 fail conhecidos) |
| Mesh 6 workers c–f (2G) | ✅ estáveis T+122k+ pós-fix (RX MEM 648→2) |
| Mesh 6 Master (a) pós-fix | 🟡 re-test pendente (rodada anterior usou imagem pré-fix; OOM T+141k pré-fix) |
| KVM+VFIO GTX 1050 (`tools/run-qemu-kvm-vfio.ps1`) | 🟡 script pronto (canários ADR-0105 preservados); AWAITING host Linux IOMMU |
| GPU compute NVIDIA (ADR-0105 B1.3/B2.4) | ▶️ AWAITING_HW (CpuOnly default; VFIO lab s410c) |
| sev audit Sev::from_sub | ✅ s409+s410 fecho (23 subs → Ok, dbg→Trace; 2 emissores corrigidos) |
| I3 fantasma | ✅ fix note_trust_entries (push pattern hermes→k_ai) |
| Runtime hygiene (`docs/architecture/runtime-hygiene-checklist.md`) | ✅ s410d: 7 estruturas corrigidas + ~19 auditadas com cap |
| Motor único SGDB (sem dual-truth engine) | ✅ s410h: AiosDatabaseEngine deletado; layers/store/e2e via NSGDB externo |

## Gate ADR-0100 (Trilho A) — checklist vivo

| Onda | Item | Estado |
|------|------|--------|
| 0 | Honesty BOOT_AI T-001–T-006 | ✅ |
| 1 | I/O + HardwareInfo (mín. T-011) | ✅ |
| 2 | SMP metal K23 `online==madt-1` | ▶️ AWAITING_HW / operador |
| 3 | OTA A2 **ou** A3 | `[ ]` lab A2 próximo |
| Review | ADR formal + OK humano | `[ ]` |

## Aceite metal quente (bloqueia A)

| Item | Estado |
|------|--------|
| `E:\BOOT.LOG` real | ▶️ AWAITING_OPERATOR (s389b: fat-boot-log ON default) |
| Freeze `@ network_agent` | ABERTO |
| xHCI MSC PP pós-HCRST | 🟡 wired; validar metal |

## Trilho B (produto 1.x)

| ID | Item | Estado |
|----|------|--------|
| s391 | Desktop/UI/Orb theme+hover+dock+mesh honesty | ✅ |
| s390b | SelfHeal residual KERNEL_ERROR/Safety/checkpoint/SKILL_CREATE | ✅ |
| s390 | SelfHeal honesty I3/budget/LLM/silent | ✅ |
| s389b | Logging follow-up SCORE/phase/fat-boot-log | ✅ |
| s389 | Logging QEMU+HW ADR-0092 honesty | ✅ |
| s388 | ModelHub + Trinity CapGate/mmap honesty | ✅ |
| s387 | Falcon3 LLM header/RoPE/hub honesty | ✅ |
| s386 | W2A8/KernelPack honesty High→Low | ✅ |
| s385 | SGDB/Tickv/NSGDB bughunt High→Low | ✅ |
| s384 | LazyLock + smoltcp 0.14 + wasmi 2.0 | ✅ |
| IDEA #544 | Falcon3-3B lab ADR-0101 | 🟡 Onda 0 parcial (header Observe ✅) |
| IDEA #549 | GGUF tokenizer/KV 32K | ⏳ residual |
| IDEA #485 | F4 CPU W2A8 ladder | ✅ s386 (device = ADR-0105 AWAITING) |
| IDEA #550 | NKP B0–B3 | 🟡 B1.2/B4 AWAITING_HW |
| #558b/#559b/#561b | aceite voz metal | residual paralelo |
| B2 | Mesh peer B | AWAITING operador |
| #607 | Efeito Matrix mmap expert residual | ⏳ |
| #608 | slog info restante → canónico | 🟡 parcial s391 (UI slog) |
| s410 | Mesh 6 OOM bughunt (worker+Master anti-bloat) + sev fecho + GOAL1 6c/5min | ✅ SESSION_410 (Master re-test pendente) |
| s409 | Docs/Governança estudo ternário+GPU HAL; sev s409 (reg/query/select/state/revoke) | ✅ SESSION_409 |
| s392 | Boot/Limine bughunt H1–H5/M1–M5/L1–L3 + canvas | ✅ SESSION_392 |
| s393 | MHI bughunt docs-only (fix-1/2/3 + canvas + pins talc/x86_64) | ✅ SESSION_393 + IDEA #609; código = outros lanes |
