# STATE - neural-os-core v1.9.99-s493 - NeuralFS substrato AIOS (FAT data=DEGRADED)
#   [s493] NeuralFS.md §14 + storage_posture; write_file_stream; hub neural_paths_for+read_range;
#     mkneuralfs seed (MBR 0x7F magic VERIFIED 64MB smoke); build_image/unified --fs neuralfs;
#     extract_esp_bootlog; boot_logger ESP-first; SysInstaller ecosystem/models+logs; FAT LOO warn.
#     Host: posture 2/2, stream 1/1, check k-nano/cortex/neural-kernel 0. Runtime QEMU Active NRFS
#     + metal usb_hw = UNKNOWN. Detalhe: SESSION_493.md.
#   [s492] materialize_hub_v7.py: FALCON3.V7/PRO.V7/FALCON10.V7/LEARNER.V7 hardlinks;
#     mkfat32 V7-first 1 dirent/SKU; fat_names_for .V7; audit REJECT=0. Nichos v5 intactos.
#     Detalhe: SESSION_492.md.
#   [s491] Equiparação TII canônica nas 8 gavetas: Active=Falcon3-3B 1.58bit; Pro=7B/10B;
#     Learner=1B (Falcon-E-1B alt); Vision=SigLino (legado VISION.BIN); H1/Mamba FORA ate loader
#     SSM; Agent vazio/não-TII; RustCoder/HwExpert/Reranker=não-TII; Laya/Jev=intent. Docs ADR-0078
#     + README + from_name aliases + fat_names SIGLINO + mkfat32. Prova: host from_name. Runtime N/A.
#     Detalhe: SESSION_491.md.
#   [s490] #PF storm (F1, classe #630) - writer capturado e cadeia corrigida. CAUSA: job_done() creditava
#     barreira global sem epoch -> arrival atrasado liberava o dispatch seguinte cedo -> BSP dropava
#     TernaryJobCtx com worker vivo -> talc reciclava o bloco -> store c_ptr=0 em ternary_worker (cr2
#     pequeno, err=write, IP fixo 0xffffffff8092b7b4; 4/7 boots reproduziam; fingerprint de 16B do PF_DBG
#     provou que o ELF usado p/ simbolizar nao era o que bootou = fim do "IP migrante").
#     FIX: JobSlot{epoch,counts}; reset_barrier bumpa epoch antes de zerar DONE; job_done(epoch,counts) nao
#     credita stale/nao-participante; enqueue_barrier so p/ workers de matmul (hermes segue count=false).
#     Harness: tools/watch_pf_writer.ps1 + tools/pf_ctx_watch.py (gdb Python re-arm por publish, 3 ultimos
#     ctx); falsificador: tools/epoch_falsifier_boots.sh.
#     PROVA: cargo check --release 0; ap_work 5/5 (stale_job_done_nao_credita_barreira_nova); 4 boots TCG
#     0 storms (baseline 4/7), matmul exercitado workers=8 T+13251.
#     HONESTIDADE: origem exata do c_ptr=0 = UNKNOWN (write zero nao capturado; cadeia barreira = com
#     falsificador); metal = UNKNOWN (watchpoint so existe em TCG).
#     LICOES: simbolizar IP exige o ELF que BOOTOU; protecao documentada com 0 callers nao protege;
#     watchpoint precisa re-arm por publish.
#     PENDENTES: F1 1h 8c/8GB (agora destravado); contador STALE_DONE p/ prova direta; aceite metal.
#     Detalhe: SESSION_490.md.
#   [s489] Path metal: validate_for_slot→register_bytes; AlwaysTiny+mark_slot HwExpert; PCI inject
#     + family names; pio metal=pio_hw.max(footprint), QEMU=8MB miss honesto. Host VERIFIED
#     (validate_for_slot + check release). Metal mid-niche VERIFIED = UNKNOWN ate usb_hw.img.
#     Pos-audit: AGENT/LEARNER *.size_mismatch; FALCON3 L28 *.misname_L28; mkfat32 find_valid_llm;
#     Learner→Falcon3-1B; rustcoder footprint 322. Detalhe: SESSION_489.md.
#   [s488] Hub MoE + revisao de fluxo ADR-0088. FEITO: resolve_route→dispatch; ExpertKind::Agent;
#     mid-slots LOO; AlwaysTiny; moe_compose pipelines; REMOVIDO skip saudacao MoE; AirLLM so apos
#     classify e so se rota geradora. PROVA host: resolve/pipeline/slot tests OK. LOO runtime UNKNOWN.
#     Detalhe: SESSION_INDEX (linha 488) + commit s488+s489 (67507ee5).
#   [s487] Passo "ovo-galinha" do load-on-first-use (fecha o arco s483 2b -> s485 evict -> s487). O seam EXISTIA desde o s483, mas TRES gates o anulavam e o registro do s483 chamou o resultado de "fail-safe": (1) qemu_dev_skip_models (hypervisor().is_sandbox()) envolvia TODO o bloco do hub, inclusive register_slot_loader -> SLOT_LOADER=0 em sandbox -> try_load_on_demand sempre false -> caminho antigo e seam NUNCA exercitavel em dev; (2) select_generator_slot exigia hub_has_blob(GeneratorPro) -> Pro sem blob nunca era escolhido, mas LOO existe para carregar o blob que falta (ovo-galinha: precisa do blob p/ escolher o slot, precisa do slot p/ carregar o blob); (3) early-return em try_hub_slot_fat (qemu_loader_2b && model_is_loaded() && slot in {Vision,Pro} -> return "Active ja carregado via QEMU-loader") recusava o load mesmo se o seam disparasse. Classe unica: gate que protege trabalho tambem mata o caminho que deveria exercita-lo.
#     FEITO: (1) P0 gate SEPARADO no bin - sandbox pula so o EAGER NeuralFS; register_slot_loader roda SEMPRE (log "HUB ok sandbox: LOO seam only (sem eager NeuralFS) - first-use + pio_cap"). (2) select_generator_slot: slot_real no lugar de hub_has_blob; prompt complexo com Pro ausente devolve GeneratorPro DE PROPOSITO (dispara LOO); fallback ja existia e e honesto - dispatch_expert -> generate_from_slot None -> "pro miss -> CURRENT_MODEL (Active)" com warn (cortex.rs:5848-5853). (3) P1 adapter hub_load_on_demand: NeuralFS (disco instalado) ANTES de FAT (pendrive/live), bool derivado de slot_real. (4) REMOVIDO o early-return qemu_loader_2b: o que ele protegia (PIO gigante) continua protegido pelo pio_cap (PIO_QEMU 8MB sob qemu_hv, PIO_FAST 400MB em metal) - era guard duplicado com o cap, e a duplicacao era o bloqueio. qemu_loader_2b segue usado no calculo do cap (main.rs:4918/4945).
#     PROVA host: cargo check --release exit=0; cargo test -p cortex --lib model_hub 2/2. PROVA runtime WHPX (NoDisk + Falcon3 loader 989MB @0x100000000, 6G/2c): logs/boot_hub_nodisk_s487_20261018_130517.txt — HUB ok "LOO seam only" + "loader registrado"; ModelHub active=1 generator_pro=alias; LLM LOADED 1012764KB. Confundidor: FAT disk 512MB estola em K33 tickv (hub fica depois) — aceite do seam = NoDisk.
#     HONESTIDADE (ADR-0113): SEAM REGISTRADO = VERIFIED em sandbox. Load real de Pro/Vision/Reranker via LOO = UNKNOWN (pio_cap 8MB sob qemu_hv). Validate/evict runtime = UNKNOWN ate LOO passar do cap (metal ou cap liberado).
#     LICOES: seam atras de gate de sandbox nasce morto em dev (gate deve pular TRABALHO, nao REGISTRO); ovo-galinha se rompe por INTENCAO; guard anti-duplicacao redundante com pio_cap vira bloqueio; boot com FAT novo pode estolar no tickv antes do hub — NoDisk isola o claim do seam.
#     PENDENTES: LOO load real (metal/cap); validate+evict runtime; #PF storm falcon3 (watchpoint); regenerar usb_hw.img; mesh re-run. Detalhe: SESSION_487.md.
#   [s486] Passo 5-6 do plano s481 (podar fat_names_for, apagar slot_from_bitnet_bytes + load_models_multi). A REGRA que governou: o plano dizia "sem callers"/"peso morto", mas a licao da PROPRIA s481 (grep que exclui diretorio produz conclusao falsa) exigiu re-verificacao no repo INTEIRO antes de qualquer delecao - e ela mudou o quadro: 2 premissas cairam, 2 delecoes se confirmaram.
#     VERIFICACAO (grep repo inteiro: crates/ tools/ docs/, .rs .py .md): load_models_multi (cortex.rs:5962) MORTO (unico match em codigo = a propria definicao); slot_from_bitnet_bytes (model_hub.rs:546) MORTO TRANSITIVO (unico caller = load_models_multi 5970/5982); ModelSlot::from_name VIVO (model_fit.rs:175 slot_footprint_mb - path do cap do Reranker s483 + maybe_fit) -> fica; MICROPY.WASM "peso morto" (s484) PREMISSA REFUTADA (modulo hermes VIVO: try_init_at_boot() em main.rs:2755 boot path; le VFS /micropython/micropython.wasm com fallback MVP embutido; eval recusa honesta micropython_mvp_stub); podar fat_names_for PREMISSA MOOTA (morreu com o cancelamento dos renames s484 - truncamento simetrico; nomes = outputs reais do mkfat32 + politica s458 PRO.v6/FALCON7B guardados).
#     FEITO: (1) apagado slot_from_bitnet_bytes (model_hub.rs ~32 LOC - heuristica params->slot / tamanho->slot). (2) apagado load_models_multi (cortex.rs ~34 LOC - loader multi-blob do boot que o load-on-first-use s483 2b tornou orfao; era o unico caller dos dois acima). (3) MICROPY.WASM - nada a podar, registro corrigido: o arquivo NEM EXISTE em target1//models -> a linha condicional do mkfat32 (find_file) ja e no-op hoje (empacota nada, custo zero); modulo hermes fica (front viva, licao s449 "sem consumidor e estado temporal"); perna FAT->VFS nunca foi conectada (leitor le path VFS, nao o nome FAT da raiz; sem mount /micropython no bin) - anotado como fato, nao corrigido (YAGNI). (4) fat_names_for - no-op honesto: a poda era o Passo 5 DEPOIS do Passo 1 (nomes canonicos); Passo 1 cancelado na s484 -> premissa da poda morreu junto; podar quebraria a politica s458.
#     PROVA: cargo check --release exit=0 (0 erros); cargo test -p cortex --lib model_hub 1/1 (evict s485 intacto); cargo test -p cortex --lib model_fit 6/6 (path from_name/slot_footprint_mb intacto). Delecao liquida ~66 LOC de codigo morto compilado, zero mudanca de comportamento.
#     LICOES: "sem callers" de plano antigo e hipotese, nao fato - o s481 escreveu "sem callers" para funcoes que TINHAM caller entre si (cadeia de morteza e transitiva, traca ate a raiz antes de apagar); peso morto declarado por grep de nome pode ser modulo vivo (MICROPY.WASM zero leitores confundiu o ARQUIVO FAT com o MODULO - modulo e boot-wired, arquivo nem existe; grep de nome de arquivo != grep de caminho de leitura); poda condicionada a passo cancelado e no-op (Passo 5 dependia do Passo 1; cancelado o 1, a poda perdeu o motivo - planos em cadeia: quando um elo cai, re-avaliar os seguintes).
#     PENDENTES: aceite de runtime do hub (validador s484 + evict s485) - HW real ou gate liberado; #PF storm sob falcon3 (pre-existente, IP migrante, watchpoint); direcao B do evict (recarregar Active) so com caminho runtime; frente MICROPY (bytecode real) - se precisar, conectar FAT->VFS ou embutir. Detalhe em docs/memory/SESSION_486.md.
#   [s485] Passo 3 do plano @oracle (evict explicito de 1 slot com heap_headroom_low()). DESCOBERTA QUE SIMPLIFICOU: clear_model() (cortex.rs:5648) JA ERA o primitivo de evict do Active (CURRENT_MODEL=None + MODEL_LOADED=false + EMBED_DIM=0 + clear_model_header + mark_active(false)) - faltava footprint voltar a None, alias Pro nao virar mentira, e o gate no ponto de carga. E heap_headroom_bytes() (allocator.rs:1043-1048) mede headroom COMBINADO (bump + TALC free medido nos gap-nodes, s435) - o evict libera exatamente o que o gate mede.
#     FEITO: (1) evict_slot(slot)->bool em cortex/model_hub.rs (primitivo publico): Active -> clear_model() + note_footprint(Active,0) + limpa bit alias de GeneratorPro SE Pro nao tem blob proprio (senao hub_status mostraria "alias" apontando para um Active morto); slots do HUB -> HUB.lock().slots[i].take() + drop (libera tensores AGORA) + mark(false) + note_footprint(0) + warn com freed~MB; RustCoder/HwExpert -> false (statics legados com consumers proprios no MoE routing - sem evict, YAGNI, fora do plano); evict de slot vazio = false (nao finge sucesso). Evict e DEGRADACAO: sempre warn, nunca ok (regra fallback s480). (2) Gate no bin try_hub_slot_fat junto ao gate do Reranker do s483 (onde sz e conhecido): slot in {GeneratorPro, Agent} && heap_headroom_low() && model_is_loaded() -> evict_slot(Active) com warn "hub evict Active p/ slot={} sz={}KB (headroom_low)". (3) Coerencia do swap: apos o evict, o branch ja existente "Active vazio e slot pro/fast vira primary" (main.rs ~5040) faz o Pro recem-carregado virar CURRENT_MODEL - o swap Active<->Pro fica completo e o sistema continua com gerador primario.
#     DIRECAO UNICA (honestidade): o plano diz "Active<->Pro|Agent", mas RECARREGAR o Active em runtime nao tem caminho hoje (Active so carrega no boot). A direcao implementada e a unica real: sob pressao, o nicho grande custa o Active. Depois do evict, generate_via_model cai no fallback dos geradores do hub (Pro -> Reranker) - degradacao com warn explicito, nao silencio.
#     PROVA: cargo test -p cortex --lib model_hub 1/1 (evict_slot_libera_blob_e_limpa_estado: register dummy no Agent -> slot_real true -> evict true -> slot_real false + footprint None + evict de vazio false + experts false; DummyModel com os 4 metodos obrigatorios da trait generate/embed_dim/vocab_size/max_seq; slot Agent porque nenhum outro teste toca HUB[7], grep em cortex/src). cargo check --release exit=0 (0 erros). Crash 0xc0000096 da suite completa: pre-existente (provado 3x no s484, stash).
#     RUNTIME = UNKNOWN (mesmo bloqueio s483/s484): sob QEMU qemu_dev_skip_models pula o bloco do hub inteiro - o gate nunca dispara em sandbox. Aceite exige HW real ou gate liberado pelo dono. IMPLEMENTED, nao VERIFIED (ADR-0113).
#     PENDENTES: aceite de runtime do evict; Passo 5-6 do plano s481 (podar fat_names_for/slot_from_bitnet_bytes/load_models_multi + MICROPY.WASM peso morto); #PF storm sob falcon3 (pre-existente, IP migrante, watchpoint); direcao B do evict (recarregar Active sob demanda) so quando existir caminho runtime para o Active. Detalhe em docs/memory/SESSION_485.md.
#   [s484] Passo 0 do plano s481 (validate entre read e load + nomes 8.3 no build). PREMISSA DERRUBADA ANTES DE IMPLEMENTAR: o plano pedia "fix dos nomes mortos 8.3" (ROUTER.BITNET/MICRO.BITNET truncados pelo writer, kernel procurando o nome longo = MoE router FAT DEAD). ANTES de renomear 9 sites, li o matcher (fat32.rs:1138 lookup_file_size -> encode_83): o kernel trunca ext para 3 chars EXATAMENTE como o writer (encode_83("ROUTER.BITNET") = [ROUTER  ][BIT] = o mesmo dirent). NOMES LONGOS CASAM - truncamento simetrico dos dois lados. Renomear teria sido refactor inutil em 9 sites. O risco REAL do truncamento simetrico e COLISAO (dois nomes -> mesmo dirent 11 bytes -> o 2o some do root) - e isso que o guard verifica. Corolario: ler o matcher antes de "consertar" nomes; a busca de 13 chars nunca casa como STRING mas casa como DIRENT CODIFICADO.
#     ARTEFATOS DE NICHOS EXISTEM (premissa s482/s483 errada): target1/ tem VISION.BIN 33.1MB (v5), RERANKER.BIN 165MB (v5), AGENT.v6 736MB (v6), LEARNER.v6 117.9MB (v6), todos magic 0xBE11BE11; find_file (mkfat32.py:148) cobre target1/ PRIMEIRO e mkfat32.py:423-431 ja empacota com os nomes que fat_names_for procura. O alinhamento build<->kernel JA EXISTIA; faltava a validacao entre read e load.
#     FEITO: (1) validate_bitnet_blob em cortex/model.rs (renomeado de validate_niche_blob: regra de FORMATO, nao de slot - sem param slot, YAGNI; fronteira de formato unica em model.rs). <16B -> too_short; magic ruim -> bad_magic; v6/v7 com data[14]!=0 (expert/router) -> not_llm (HwExpert tem path dedicado load_hwexpert_v6 main.rs:4612-4826; parsear como LLM e fantasma); v6/v7 LLM -> probe_llm_file_size Ready(n) com n==data.len() -> Ok, senao size_mismatch/truncated_header/bad_header (s480: fantasma de tamanho = recusa); v1..=5 -> so magic -> Ok (warn de migracao vem do load_model_v6; VISION/RERANKER v5 passam); outra versao -> unsupported_version. (2) Wire nos DOIS paths entre read_file e load_model com warn + continue (FAT try_hub_slot_fat ~5026 "hub REJECT"; NeuralFS try_hub_slot_neuralfs ~4866 mesmo gap +3 linhas). Wiring: bin usa cortex_crate::model:: (NAO crate::cortex::model:: - facade do bin nao tem o modulo; 1a tentativa E0433, corrigido para o caminho que o bin ja usa em probe_llm_file_size main.rs:284/4467). (3) Guard 8.3 no mkfat32.py: warn_83_collisions(files) antes do write (cobre TODOS: modelos, firmware, NKP, CONFIG/BOOT.LOG/NSGDB/TLSPINS/HINT) - [!] COLISAO 8.3 quando dois nomes colapsam para o mesmo dirent (o 2o some) + [~] 8.3 truncado informativo. Demo com nomes reais: 5 truncamentos avisados, colisao sintetica detectada, None ignorado, ZERO falso-positivo entre nomes reais.
#     PROVA: cargo test -p cortex --lib model 11/11 (novo validate_bitnet_blob_gates: v6 valido Ok, truncado size_mismatch, magic ruim, curto, v5 Ok, v8, model_type!=0 not_llm; blob v6 via synth_header + parse_model_header, auto-consistente, sem include_bytes). cargo check --release 0 ERROS (1m13s). mkfat32.py sintaxe OK + demo do guard. Crash 0xc0000096 STATUS_PRIVILEGED_INSTRUCTION na suite completa do cortex: PRE-EXISTENTE provado pela 3a vez (git stash -> rodar -> crasha igual, mesmo binario cortex-9efca80e55e91f2b.exe -> stash pop limpo; SESSION_418/483).
#     RUNTIME = UNKNOWN (mesmo bloqueio do s483): sob QEMU qemu_dev_skip_models pula o bloco do hub inteiro - o validador nao dispara em runtime. Aceite exige HW real ou gate liberado pelo dono. IMPLEMENTED, nao VERIFIED (ADR-0113).
#     REGISTRADO, NAO FIXADO: MICROPY.WASM empacotado com ZERO leitores em todas as crates (peso morto confirmado por grep; candidato Passo 5-6 do plano s481). Nomes longos funcionam via truncamento simetrico - NENHUM rename feito; o guard de colisao e a protecao da classe.
#     PENDENTES: aceite de runtime do validador (HW real/gate); Passo 3 (evict explicito Active<->Pro|Agent com heap_headroom_low()); NeuralFS ainda eager; #PF storm sob falcon3 (pre-existente, IP migrante, watchpoint); Passo 5-6 (podar fat_names_for/slot_from_bitnet_bytes/load_models_multi + MICROPY.WASM). Detalhe em docs/memory/SESSION_484.md.
#   [s483] Passo 2 do hub (decisao do HUMAN = opcao B: 2a + 2b completo com hook gated). 2a RERANKER ALCANCAVEL: main.rs pio_cap do Reranker 32MB -> slot_footprint_mb(slot.name()) (era INALCANCAVEL por construcao: artefato 165MB vs cap 32MB + if sz>pio_cap{continue} = skip sempre com log info parecendo tentativa; IMPLEMENTED que se apresenta como wired); + GATE heap_headroom_critical() antes do read_file (corrigir o cap torna a leitura de 165MB possivel e sem gate e o padrao OOM->#PF->park s415-420). model_fit.rs: bracos vision/siglip/vit->33MB, reranker/bge_reranker->165MB, agent/qwen3b->752MB. LICAO: o footprint MEDIDO (Passo 1) so existe DEPOIS do load - no momento de dimensionar o load measured e None e "reranker" caia em _=>None, entao meu primeiro edit dava unwrap_or(0)=cap 0=NUNCA carrega (pior que antes). O footprint tem DUAS metades: medido (pos-load, s482) e hint estatico (pre-load, este passo); faltava a segunda.
#     2b LOAD-ON-FIRST-USE: cortex/model_hub.rs ganha pub type SlotLoader = fn(ModelSlot)->bool, static SLOT_LOADER:AtomicUsize (0=nao registrado), register_slot_loader(f), try_load_on_demand(slot), e ensure_slot_loaded(slot) que NAO pega nenhum lock (o loader usa HUB e TicketLock nao e reentrante s438 - carregar dentro do lock seria deadlock). generate_from_slot chama ensure_slot_loaded ANTES do match (= antes de qualquer lock). slot_real virou pub. Padrao bridge identico ao da TLS (SESSION_241): tipo na crate FE, register_* na FE, boot registra com cast explicito, consumidor chama a API da FE. main.rs: REMOVIDAS as 5 chamadas eager try_hub_slot_fat (Reranker/Vision/Pro/Learner/Agent) e no lugar register_slot_loader(hub_load_on_demand); adapter de 3 linhas porque try_hub_slot_fat retorna () e tem varios return; (mudar a assinatura seria diff grande sem ganho) e o bool sai do estado real (slot_real) depois da tentativa. O boot deixa de fazer 5 leituras PIO por boot sem ninguem ler o resultado.
#     CORRECAO MINHA ("sem consumidor" era meio errado): eu tinha dito que Vision/Reranker/Learner/Agent nao tem consumidor. METADE ERRADO: generate_from_slot EXISTE e E CHAMADO (cortex.rs:5845, path do Trinity/MoE). O que nao existe e consumidor de Vision/Reranker/Learner/Agent - para eles o hook nunca dispara (custo zero, resultado correto). Active/Pro sao os que gatilham. O grep de caller e a unica prova.
#     PROVA: build cargo build --release 0 ERROS; host test cargo test -p cortex --lib model_fit 6/6. O TESTE FOI REESCRITO: a versao s482 fixava slot_footprint_mb("vision")==None como PRECONDICAO - ou seja FIXAVA A MENTIRA COMO EXPECTATIVA (passava 6/6 enquanto a mentira valia). O s483 afirma a verdade nova: NENHUM slot do hub pode responder None (senão slot_too_tight(None)==false = "sempre cabe"). Verificado: falhou com left:Some(33) right:None antes da correcao - o fix estava certo, a expectativa estava errada. Runtime (falcon3-3b, 1 no x 7G, 180s): T+8359 vivo, [PANIC] literal 0, tick=32 - NAO regrediu. cargo test -p cortex --lib (suite completa) crasha com 0xc0000096 STATUS_PRIVILEGED_INSTRUCTION - PRE-EXISTENTE, provado pelo metodo do repo: git stash -> rodar -> crasha igual -> stash pop (SESSION_418 documenta o mesmo crash em k_ai/hermes).
#     2a/2b RUNTIME = UNKNOWN (bloqueio honesto): sob QEMU, qemu_dev_skip_models (hv.is_sandbox()) pula o bloco do hub INTEIRO. Neste boot o log "loader registrado (load-on-first-use; 0 leituras PIO no boot)" NAO aparece; SLOT_LOADER fica 0 -> try_load_on_demand retorna false -> generate_from_slot(Pro) cai no caminho antigo (HUB vazio -> None -> Active). Fail-safe por default, comportamento identico ao anterior no QEMU. Isto e by design e seguro, mas 2a/2b sao IMPLEMENTED, nao VERIFIED: aceite em runtime exige HW real (ou o gate de sandbox liberado pelo dono - decisao que o @oracle marcou explicitamente como do Maintainer). Registrei bloqueado em vez de declarar "provado".
#     BUG PRE-EXISTENTE (mantido do s482, sem regressao): #PF storm sob falcon3 persiste com IP MIGRANTE (o sitio muda a cada boot = assinatura da classe wild-write/corrupcao s438/s415-417): s482 ip=0xffffffff808f2204 cr2=0xfb8 | s483 ip=0xffffffff80912334 cr2=0x1fb8 | baseline s479 ip=0xffffffff80216bc0 cr2=0x40000007fff8 + ip=0xffffffff80b341b6 cr2=0x3f4ccccd. NAO e do Passo 2 (o baseline ja tinha 2 storms). Precisa watchpoint para capturar o WRITER (tools/watch_corruption.ps1 / QEMU monitor). Simbolizacao por llvm-nm no ELF da arvore continua falhando (faixa de enderecos divergente) - nao nomear simbolo sem extrair o ELF de dentro da ESP (SESSION_446).
#     PENDENTES: aceite de runtime de 2a/2b (HW real ou gate liberado pelo dono); Passo 3 (evict explicito de 1 slot Active<->Pro|Agent com heap_headroom_low()); NeuralFS ainda eager (loop try_hub_slot_neuralfs main.rs:5046 continua no boot - deixei fora do 2b porque e outro meio de transporte); #PF storm sob falcon3; Passo 0 (validate_niche_blob + nomes canonicos no build - precisa de artefato Vision/Reranker para o footprint medido ter o que medir). Detalhe em docs/memory/SESSION_483.md.
#   [s482] Passo 1 do hub: footprint medido por slot (+ harness de #PF que parou de mentir). Este e o Passo 1 do plano que @oracle desenhou no s481 (honesty -> alcance -> evict). MUDANCA: cortex/model_hub.rs ganha static SLOT_FOOTPRINT:[AtomicU64;8] (bytes MEDIDOS por slot, 0=sem blob), note_footprint(slot,bytes) chamado no topo de register_bytes (cobre os 4 caminhos: GGUF/LLM-v6/HWExpert-v6/redirect; no redirect HwExpert->Active grava NOS DOIS, senao Active fica stale) e measured_slot_mb(slot)->Option<u64> (None=nunca inferido). cortex/model_fit.rs: slot_footprint_mb() consulta o medido ANTES do header global (medido > header global > hint estatico). O que mata (medido): antes vision/reranker/agent nao tinham braco no match -> _=>None (linha 200) -> slot_too_tight(None)==false -> "SEMPRE CABE"; e active/pro/learner respondiam pelo MESMO loaded_model_header (do ultimo carregado).
#     PROVA: host test model_fit::ram_policy_tests::slot_footprint_e_por_slot_nao_header_global (antes slot_footprint_mb("vision")==None; agora vision=33, generator_pro=1900, e o pair deixa de ser indistinguivel), 6/6 em cargo test -p cortex --lib model_fit. Build cargo build --release 0 erros. Runtime (falcon3-3b, 1 no x 7G, 200s): model 1012764KB (tamanho correto), LLM LOADED, [PANIC] literal ausente, T+10165, tick=32 (era 1 no s481) - NAO regrediu. RUNTIME DO FOOTPRINT EM SI: BLOQUEADO/UNKNOWN - o falsificador do @oracle (carregar Vision e ver generator_pro parar de reportar 33MB) exige um blob Vision/Reranker que NAO EXISTE no repo (so .bin de dep-graph/query-cache); depende do Passo 2 (build dos nomes canonicos). ADR-0113: logica VERIFIED por host test, propriedade runtime UNKNOWN.
#     ACHADO COLATERAL - HARNESS DE #PF MENTIA DE DOIS JEITOS: tools/run-mesh-model.ps1 acusava "ALGUM NO COM #PF" num boot saudavel. (a) 5 das 12 ocorrencias de "#PF" no log sao TEXTO ("patch_idt: ... #UD/#GP/#PF hook", "avoid #PF"), nao faults - fault real exige "[EXC] #PF" (7 no log); (b) o regex "panic" sem ancora casava panic=true do registro cross-boot do k_ai SELF lendo o ramlog do boot ANTERIOR (mesma classe do s477). Corrigido para usar o marcador que o kernel JA EMITE ("#PF storm" / "RECOVER-HALT") e o literal [PANIC]. Validado contra 3 logs: falcon3-atual -> FALHA(storm), falcon3-baseline -> FALHA(storm), BGE -> SAUDAVEL.
#     BUG PRE-EXISTENTE ABERTO (#PF storm sob falcon3): o boot do falcon3 tem #PF storm real (streak=3+ park core), NAO causado pelo Passo 1 - baseline s479 (boot_mesh_c_FALCON3.txt, pre-Passo1) tem 2 storms (ip=0xffffffff80216bc0 cr2=0x40000007fff8 heap TALC; ip=0xffffffff80b341b6 cr2=0x3f4ccccd = 0.8f spill de AffectSnapshot::NEUTRAL) contra 1 no run atual (ip=0xffffffff808f2204 cr2=0xfb8). O IP muda a cada boot e o baseline tem MAIS - assinatura da classe wild-write/corrupcao (s438/s415-417: o sitio do crash migra). NAO e whack-a-mole: precisa watchpoint (tools/watch_corruption.ps1 / QEMU monitor) para capturar o WRITER. Simbolizacao por llvm-nm no ELF da arvore FALHOU (nenhum simbolo abaixo do IP) - enderecos de runtime e ELF nao estao na mesma faixa (SESSION_446: nao atribuir crime a outro build; extrair o ELF de dentro da ESP antes de nomear simbolo).
#     PLANO DO @ORACLE (3 achados verificados por mim): (1) AirLLM NAO e estrategia de troca - gguf_streaming.rs:322 = "generate STUB (attn/FFN nao wired) - refuse inventar tokens", :86 diz net=0 callers; e um bool de FIT (needs_airllm), nao um scheduler; layer streaming sobre ATA PIO = I/O por token. A troca real e residencia (Active sempre + 5 pequenos ~644MB sob demanda) + 1 evict explicito Active <-> (Pro|Agent). (2) Reranker INALCANCAVEL: main.rs da ModelSlot::Reranker => 32MB de pio_cap contra artefato de 165MB, e if sz>pio_cap{continue} -> skip sempre, mas o log sai info (IMPLEMENTED que se apresenta como wired). (3) Boot e EAGER (main.rs:5032-5036 tenta Reranker->Vision->Pro->Learner->Agent em todo boot) - Passo 2 troca por load-on-first-use. Nome canonico (s481) fica; role no header v7 NAO (slot-id e chave interna, nome e chave de descoberta; o que faltava era registro de footprint em runtime = este Passo 1). Ordem: 1 honesty (feito) -> 2 alcance+demanda -> 3 evict com gate de headroom.
#     PENDENTES: Passo 2 (pio_cap do Reranker + load-on-first-use; falsificador: boot cold com RERANKER.V7 presente ainda mostra reranker=1 -> o eager nao foi revertido); Passo 3 (evict explicito de 1 slot com heap_headroom_low()); runtime do footprint depende do build dos nomes canonicos; #PF storm sob falcon3 (item pre-existente). Detalhe em docs/memory/SESSION_482.md.
#   [s481] resolve/informa do hub: severidade honesta, presenca real (anti-alias), ausencia->fallback + tabela de nichos no README (detalhe em docs/memory/SESSION_481.md)
#     DUAS CORRECOES MINHAS (mesma causa): meu grep anterior cobria cortex/hermes/jarbas/k_ai e EXCLUIU neural-kernel/src/. (a) hub_status() JA TEM caller em main.rs:5043 - eu disse "sem nenhum caller" e estava ERRADO, o gap nunca existiu; (b) mark_pro_alias tem caller VIVO em main.rs:5041 (marca Pro quando CURRENT_MODEL_EMBED_DIM >= 2048, e o 3B tem 3072) -> o alias ESTA NO AR: com Active 3B, slot_loaded(GeneratorPro)=true sem existir blob Pro.
#     FIX (crates/cortex/src/model_hub.rs, +39/-14): (1) slot_real(slot) NOVO - presenca REAL anti-alias: para os 5 slots do HUB so conta blob em HUB (hub_has_blob); Active/RustCoder/HwExpert vivem nos Mutex legados, entao slot_loaded e a verdade para eles. Elimina a mentira do alias na decisao. (2) maybe_fit agora considera AUSENCIA: antes so caia em fallback por too_tight (RAM) e um nicho ausente com RAM folgada era devolvido vazio; agora ausencia e RAM caem no mesmo pick_fit_fallback sem duplicar a decisao. (3) severidade honesta: o log de fallback saiu de "ok" para "warn", re-enquadrado como niche={} -> fallback={} reason={} (motivos absent|too_tight). Fallback e degradacao, nao sucesso (ADR-0113).
#     Prova de runtime (falcon3-3b, no unico x 7G, 240s, logs/boot_mesh_a_FALCON3.txt run 20261017_114605): NAO regrediu - model 1012764KB (tamanho CORRETO, o 590680KB fantasma nao aparece), LLM LOADED llama8b.bin size=1012764KB, LOADED - model active in ModelHub, T+3658 (MAIOR que os 1106 do s479). tick=1 e o comportamento conhecido do 3B (a inferencia segura o tick budget), NAO regressao.
#     TABELA slot->modelo->tarefa escrita no README (nao existia): 8 slots do hub (Active=FALCON3.V7, GeneratorPro=PRO.V7, Agent=AGENT.V7, RustCoder=RUSTCDR.V7, HwExpert=HWEXPRT.V7, Vision=VISION.V7, Reranker=RERANKER.V7, Learner=LEARNER.V7) + fora do hub (BGE.BIN, PIPER.BIN, STT.BIN, ROUTER.BITNET). Nomes 8.3 uppercase porque o kernel pula LFN (fat32.rs 0x0F); ROUTER.BITNET tem ext de 6 chars e nao cabe -> ROUTER.BIT.
#     Pendentes: Passo 1 SLOTS= + nomes canonicos no build + guard 8.3 no writer (mkfat32.py:729); Passo 2 validate_niche_blob; Passo 4 validator entre read e load; Passo 5-6 podar fat_names_for e apagar slot_from_bitnet_bytes + load_models_multi (sem callers). Nichos alem do gerador ainda sem resolucao propria (subsistemas degradam ad-hoc, ex.: BGE->emb=pseudo). mesh peers=0; metal USB BOOT.LOG (s467).
#   [s480] bge-m3 deixa de entrar em panic: resize_heap_to_mb fail-closed + NotLlm propaga (detalhe em docs/memory/SESSION_480.md)
#     PASSO 0 (allocator.rs:1641): resize_heap_to_mb agora recusa fail-closed quando TALC_READY. Mecanismo VERIFIED: CURRENT_HEAP_MB (teto do BUMP, :1173/:455) era usado como se fosse o tamanho do TALC, cujo span de boot e budget (~75% RAM, :2089) -> extend recebia req MENOR que old -> assert do talc (new_heap must contain old_heap). Medido: current=512MB target=1795MB -> req=1,75GB vs old=~5GB. start_virt mapeava paginas dentro do span ja reivindicado. Unico caller: cortex.rs:2630 (load_model legado); memory_agent.rs:214 ja tinha removido o dele ("mirava a pool errada").
#     PASSO 3 (main.rs): qemu_loader_llm_len passa a devolver Option<usize>; NotLlm e probe esgotado -> None. A const BITNET_2B_V4_BYTES = 604_856_373 (o "590680KB" que NENHUM artefato do repo tem) foi removida dos DOIS call sites (:3953 e :4062, fallback de 0x120000000). Causa: probe_llm_file_size JA classificava NotLlm corretamente (model.rs:164-173) mas o `_ => break` (:291) descartava o veredito e caia na constante -> BGE_M3 (embedding, 135MB) e ROUTER (25KB) eram carregados como LLM generativo com shapes fabricados (layers=24).
#     Comprimento do blob no loader vem SO do probe: fat_sz descreve o arquivo no disco e pode ser OUTRO modelo - nao pode virar comprimento da regiao do loader.
#     Prova de runtime (bge-m3, no unico x 7G, 240s, log logs/boot_mesh_a_BGE_M3.txt run 20261017_113653): tick 65 -> 8448; [PANIC] literal = NENHUM (antes: talc + cortex.rs:534 unwrap None); log "blob @0x100000000 tem magic BitNet mas nao e LLM (v6/v7 + model_type=0) - loader ignorado"; subsistema assume fallback "[BGE] ausente - emb=pseudo (hash/TF bits)"; PF=8 (so demos Ring3 P6). Recusar honestamente e melhor que carregar errado.
#     Achado de harness: run-mesh-model.ps1 reportou Panic=1 mas [PANIC] literal case-sensitive e NENHUM - o script conta "panic" case-insensitive e casa com o registro cross-boot do k_ai SELF (boot_failure ... panic=true) lido do ramlog do boot ANTERIOR. Veredito de panic = [PANIC] literal, nunca regex case-insensitive.
#     Pendentes (plano do hub, ora-3): resolve/informa - "escalate slot=pro -> active reason=too_tight" hoje sai com severidade ok (fallback e degradacao -> warn); hub_status() nao tem NENHUM caller; fallback so dispara por RAM (too_tight) e nao por ausencia; nichos alem do gerador (Vision/Reranker/Learner/BGE/Piper/STT) nao tem resolucao de nicho. Passo 2 validate_niche_blob; Passo 4 validator entre read e load; Passo 1 SLOTS= + nomes canonicos 8.3 no build; Passo 5-6 podar fat_names_for e apagar slot_from_bitnet_bytes + load_models_multi (sem callers). Mesh peers=0; metal USB BOOT.LOG (s467).
#   [s479] #PF storm tick=1 = recursao mutua infinita affect (nao era heap) + matriz 8 modelos + HW emulado maximo (detalhe em docs/memory/SESSION_479.md)
#     CORRECAO do s478: o "range do SLAB" era diagnostico ERRADO (aritmetica hex). 0x4000_0000_0000 = 64 TiB (nao 4 TiB) -> cr2 estava DENTRO da janela de 512 KiB e o in_slab disparava. Regra nova: conferir hex->TiB antes de declarar um range.
#     Causa-raiz REAL (VERIFIED por disassembly): recursao mutua infinita de 2 funcoes introduzida em 92d25242/s468d -> globals.rs:66 sync_affect_snapshot -> affect_channel.rs:120 sync_affect_snapshot_both -> volta. Stack de 8 MiB do scheduler do BSP (main.rs:5518) transborda -> #PF no limite do heap VA -> park core em tick=1.
#     Gate de heap do s476 nao podia funcionar: o caminho da recursao NAO ALOCA antes de recursar (o unico Vec::with_capacity esta depois do call recursivo), logo heap_headroom_critical() era false no tick 1.
#     Fix (4 arquivos, +34/-52): globals.rs:65 faz o trabalho real (publica canal tipado + escreve AFFECT_SNAPSHOT que o compositor lia congelado em NEUTRAL); affect_channel.rs:107 vira shim de 1 linha (ciclo quebrado, snapshot duplicado eliminado); allocator.rs mantem (err & 0xB) RSVD e REVERTE start=HEAP_START para LARGE_HEAP_START (cobrir o buraco mediao|esmagava 512 KiB do heap VA em silencio); smp/mod.rs:518 AP_STACK trace->info.
#     Prova (mesh 4x1G/4c sem modelo): tick = 3201/3073/2944/2784 (antes 1 em todos), mesh TX MCH ok=true, zero #PF storm. Os 8 #PF restantes sao os demos Ring3 P6 intencionais (ring3.rs:182) -> contador por regex e falso-positivo.
#     Matriz 8 modelos (no unico x 7G, 240s cada): saudavel+carregado = falcon3-3b (1012764KB, v6 h=3072 L=22), bitnet2a-4b (778810KB, v7 tok_len=5390533), piper (61342KB + 46070 amostras TTS). TRAVADO = bge-m3 (PANIC talc.rs:836 new_heap must contain old_heap, 6/9 phases).
#     Achado transversal ABERTO: 5 de 8 logs imprimem tamanho ficticio identico model 590680KB (577 MB) - nenhum arquivo do repo tem esse tamanho. Probe le header v6 em blobs v3/v4 (Piper=3, BGE/STT=4) e fabrica LLM fantasma; contido em 4, fatal no bge-m3.
#     HW emulado maximo (tools/run-qemu-fullhw.ps1): 10 devices enumerando - video (fb 1280x800 bpp=4 + virtio-gpu-pci), NVMe, virtio-blk, AHCI/SATA, e1000, xHCI (1b36:000d 8 portas os_owned), usb-storage/MSC (MSC desc cfg=1 mps=1024), HDA, ATA/IDE. Achado: DUAS NICs quebram o net bootstrap (6/9 PHASE, 0 ticks, preso no smoke de DNS, MAC default em vez da configurada) -> -VirtioNet virou opt-in.
#     4 scripts novos (ASCII-only, argv dump por ciclo, metricas null-safe): run-mesh-model.ps1, run-mesh-models-batch.ps1, run-mesh-empty.ps1, run-qemu-fullhw.ps1.
#     Pendentes: bge-m3 panic + probe 590680KB; mesh peers=0 (TX ok, nenhum peer descoberto); duas NICs; metal USB BOOT.LOG (s467), F1 1h 8c/8GB, #PF AP #3 (cr2=0x13b8).
#   [s478] #PF storm de pressão do heap: try_fault_in_heap sem gate + range do SLAB descoberto (detalhe em docs/memory/SESSION_478.md)
#     Problema: gate heap_headroom_critical() do s476 estava só no sleep_cycle; try_fault_in_heap é chamado diretamente do handler de page fault sem guard. #PF storm reapareceu com cr2=0x000040000007fff8 (range do SLAB, não coberto por nenhum range do try_fault_in_heap → no_rng → #PF não curado → storm → park core).
#     Causa-raiz (3 problemas): (1) gate incompleto — heap_headroom_critical() não estava em try_fault_in_heap; (2) range do SLAB (HEAP_START a LARGE_HEAP_START) não coberto — faults no SLAB caíam em no_rng; (3) handler de page fault chamava try_fault_in_heap sem verificar heap_headroom_critical().
#     Fix (2 arquivos): allocator.rs — gate heap_headroom_critical() antes de alloc_physical_frame() em try_fault_in_heap + range in_slab (HEAP_START..LARGE_HEAP_START) mapeado via identidade HHDM; interrupts_ext.rs — !heap_headroom_critical() && antes de chamar try_fault_in_heap no handler de page fault.
#     Prova de runtime (QEMU TCG, smp=4, 4GB): log logs/boot_whpx_20261016_010907.txt. pf=0 (zero page faults), heap=1024/3840M, tick=43723+, aps=3, [SILENCE] log parado ha 810s — sistema estável 13+ min sem #PF. Antes do fix: #PF storm em segundos após o Runtime.
#     Classificacao: fix = runtime-VERIFIED (QEMU TCG).
#   [s477] USB BOOT.LOG em HW real: fix EP0 A+B+C runtime-VERIFIED (detalhe em docs/memory/SESSION_477.md)
#     Problema: caminho MSC/BOT existia no kernel mas nunca rodava em dev (launcher nao anexava device MSC ao xHCI); validacao USB em HW real rotulada AWAITING_HW. Experimento do forum (BUFFY-0014, device usb-storage): bloqueador = EP0 timeout.
#     Causa-raiz (BUFFY-0015, via subagente oracle): 2 bugs no ciclo de vida do anel EP0. A) regressao s465 (e2d79925): tr_pa stale pos-troca de anel (ep0_ring devolvia tr_va=novo/tr_pa=velho -> evento real descartado como 'isoc'); B) anel EP0 reutilizado sem tracking de enqueue/cycle (TD#2 escrito em regiao ja consumida -> nunca-completa).
#     Fix (4 arquivos, +104/-44): A) ep0_reserve deriva tr_pa=tr_va-pmoff e o campo tr_pa foi APAGADO; B) cursor ep0_enqueue/ep0_cycle nos 3 writers + status_pa por offset do cursor; C) P24a honesto (falha de transporte nao reivindica porta). Opcional: 'isoc' => Sev::Warn.
#     Prova de runtime D2 (WHPX de 1a, usb-storage no xHCI): log logs/boot_usb_msc_fix_20261015_115605.txt (120505 B, sha256 A8EBC983...A85CE). ZERO EP0 timeout (antes 7+), MSC enumerado, BOOT.LOG NO DISCO USB (msc=true), P24a honesto. Prova no artefato: readback externo do disk_qemu.raw (dirent BOOT.LOG size=262137 + conteudo com as linhas deste boot).
#     Forum: BUFFY-0011..0016 (arco fechado). Classificacao: fix = runtime-VERIFIED (QEMU WHPX + artefato); metal = UNKNOWN (QEMU PASS != METAL PASS).
#   [s476] Fórum OPCODE/1 retomado: fixes v6/v7 do loader com prova de runtime + gate de heap no sleep_cycle (detalhe em docs/memory/SESSION_476.md)
#     P1 FREEBU-0150: gate fail-closed heap_headroom_critical() no topo de CellNetwork::sleep_cycle (caller único connect(); padrão moe.rs/SESSION_416); teste host connect_is_dedup_and_pair_survives 1/1; fan_out/fan_in dedup'd e limitados por construção.
#     Prova de runtime D2 (2 boots WHPX 420s, imagem reconstruída 0 erros): Boot A ACEITE "HWEXPERT LOADED v6 (QEMU-loader @0x13e000000)" — v6 não precisa reemitir como v3 (fecha FREEBU-0148); Boot B ACEITE v7 solo h=2560 L=30 tok_len=5390533, size 778810KB delta 0, zero REJECT h=0 L=0 (fecha SESSION_475). Logs + sha256 em logs/boot_expert_v6_*.txt e logs/boot_v7_solo_*.txt.
#     Lições: parser de modelo é contrato com o layout do header (v6 hidden@18 vs legado hidden@14 = REJECT silencioso h=1 L=0 — dispatch por versão/model_type no caller); probe de tamanho cobre o header INTEIRO (tok BPB1 5.4MB não cabe em 64B — probe_llm_file_size Need/Ready + kv_term=hidden*k_dim delta=0 em 8 artefatos); QEMU-loader empilha extras DEPOIS do fim real do primário (0x13DD9BAB0; 0x129200000 cai DENTRO do LLM → regions overlap).
#     Honestidade: #PF storm pós-PostRuntime em ambos os boots, sites rotativos (classe #630/F1, não é regressão; OBSERVED/causa UNKNOWN — watchpoint é o próximo passo).
#     Harness: run-qemu-whpx.ps1 -Force (preflight de RAM com overcommit consciente) + tools/run_boot_evidence.ps1 (vars OVMF frescas por ciclo, .args.txt por ciclo).
#     Fórum: BUFFY-0006..0010. Pendente: mesh re-run (CURAIX-0060), F1 1h 8c/8GB, #PF storm (watchpoint), USB BOOT.LOG em HW real.

#   [s468d] BEI hardening + DynamicMoE GGUF + AP offload + traits + property tests + typed AFFECT channel (detalhe em docs/memory/SESSION_468d.md)
#     P0.1/P0.2: Watchpoint DR0 armed on `affect_regulator` (s452); `BeiState` pinned to fixed HHDM frame via PMM — eliminates REMAP race in `map_page_direct`.
#     P1: `record_result` real metrics — `CortexAgent` stores `LAST_LLM_SUBMIT_TICK` on submit; `HermesAgent` computes latency (ticks) + heuristic confidence on `LLM_RESPONSE`, calls `bei_state().record_result()`.
#     P2: `connect_components` documented honestly — lock order `net→pc→mem→affect→supervisor→dmoe→lifecycle→budget`; "PARTIAL/noop" warning removed.
#     P3: `DynamicMoE` loads GGUF experts — `load_expert_from_gguf()` parses GGUF, dequantizes tensor by name, converts to ternary via `f32_to_ternary_packed`, registers `BitLinear` in `MoELayer` + `ExpertLifecycleManager` (bits=2). `AutoLearnAgent` loads expert GGUF from FAT32 on `learn_topic()`.
#     P4: `BeiHeapPolicy` centralizes thresholds (low=128MB, critical=48MB); `ThrottledLog` trait replaces 7+ ad-hoc statics (`LAST_LOG`, `LAST_BUDGET`, etc.).
#     P5/Item 10: `bei_tick` split — mesh/FL/CRDT offloaded to AP workers via `k_nano::smp::ap_work::enqueue`; new `ap_jobs.rs` with `mesh_fl_crdt_tick_job`.
#     Item 6: `op_ir_schema_hint()` expanded with real ops + examples; `model_skill_prompt` uses it.
#     Item 11: `affect_regulator` sync verified — `Arc<Mutex>` shared, BEI decay+snapshot under lock, Hermes/Display `incorporate_affect`, compositor reads snapshot.
#     Item 8/9: `BeiHeapPolicy` + `ThrottledLog` unified in P4.
#     Item 12: Traits for testability — `k_ai::traits` (`ExpertLifecycle`, `BudgetManager`), `cortex::traits` (`PlasticityController`).
#     Item 13: `DynamicMoE` property tests (8) — expert count ≤ max, merge/split/birth counts, metadata sync, entropy ≥ 0, confidence ∈ [0,1]; `flush_births` guards `max_experts`.
#     Item 14: `AFFECT_SNAPSHOT` typed EventBus channel — `affect_channel.rs` with serialize/deserialize, typed publish/subscribe, backward-compat `sync_affect_snapshot_both`.
#     Item 15: `bei_tick` contract documented in `bei.rs` header — lock order, heap gates, fail-closed, mesh integration, metrics.
#     Build: `cargo check --release` 0 errors; `cargo test -p cortex property_tests` 8/8 passed.
#     Image: `target\usb_hw.img` (8319 MB, kernel sha `a2fe1c24c0ab1680`) ready for metal flash.

#   [s467] Pós-tarefa s466b (detalhe em docs/memory/SESSION_467.md)
#     Lab mesh4 4x2G/4c com HDA+xHCI sim: OVERALL PASS (peers=3, A Master,
#     B/C/D Worker, smp_online=4; LLM ausente = honest-skip esperado @2G sem
#     high-RAM; xHCI P5/P6 CCS=1 = tablet+kbd; foto PRE 00:05.0 gratis).
#     Imagem HW usb_hw.img 8319MB (PACK_LLM=falcon3-1b,falcon3) 10/10+smoke.
#     Auditoria reviewer externo: 9/9 fechada no working tree (0 codigo novo;
#     testes boot_logger 13/13). edc61ac4: lab imgDir target-first + -Force +
#     disco FAT32 snapshot + intel-hda/xHCI. f57b2eff: BOOT.LOG dedup (1 dirent,
#     guard count_bootlog_dirents.py); regulatory.db/WIFI.CFG = sem consumidor,
#     nao empacotar. Pendente: foto PRE/POST 00:14.0 em metal; MSG reset (HW).

#   [s466] MESH REPRISE 4G/4c PÓS-S464 (detalhe em docs/memory/SESSION_466.md)
#     Raiz do "1 core" no mesh: run-qemu-p2p-mesh.ps1 preferia target1\uefi.img
#     STALE (kernel sha 0617d4c8 pré-s464, bug bit-11 EXTD) -> fail-closed ->
#     LAPIC intocado -> smp_online=1. Discriminador FAT: target\ = 8b0bb7b8 =
#     arvore. FIX: imgDir flip (target/ primeiro, target1/ fallback) + -Force.
#     Modelos: -WithModels -NoDisk; FALCON3 cabe em 4G (high RAM ate 0x140000000;
#     PIPER+ honest-skip). PROVA WHPX 2x4G/4c: M1-M4 PASS nos 2 nos (smp_online=4,
#     LLM LOADED 1012764KB, peers=1 Master/Worker). GUEST_DEAD externo 00:57,
#     nao crash. Li: preferencia de imagem = contrato de freshness; provar o
#     artefato BOOTADO (FAT walk), nao o da arvore.
#
#   [s465] FOLLOW-UP USB EP0/BOT (detalhe em docs/memory/SESSION_465.md)
#     Correlação EP0 (TRB+slot+DCI+CC, drain pré-doorbell, tr_pa no XhciState);
#     CDB explícito 6/10 (7 callers); DMA persistente 1 página (free removido);
#     BOT loga estágio+opcode+CC. Mass Storage Reset DEFERIDO (sem HW p/ provar).
#     HOST: check 0 erros; k-nano 386 passed (sem testes novos — bare-metal/MMIO).
#     HW itens 1/5 (PRE/POST, USB 2.0) são procedimentos, sem código.
#
#   [s464] AUDITORIA USB BOOT.LOG (detalhe em docs/memory/SESSION_464.md)
#     Verificação estática de auditoria externa + 9 fixes (working tree, sem HW).
#     P0 bulk_transfer deadlock (guard XHCI_STATE escopado ao doorbell); FAT exige
#     written==write_len + verify_written_chain (readback pela cadeia, falha=IoFail);
#     sync_cache USB/virtio conferido; CSW tag+resíduo; free_dma; EFI set_position+
#     get_position==len+flush; S5 gated persisted_to_storage (try_flush==true é
#     "gravado em disco", não "RAM selada"); flags=0 ao consumir selo.
#     HOST: MemDisk+FAT32 mínima, 13 testes boot_logger (parcial/corrupção/0/511/
#     512/513/truncada/read-error); check 0 erros; k-nano 381 passed (serializado).
#     xHCI reset + CCS=0/00:14.0 INTOCADOS (sem evidência pré-HCRST). Encerramento
#     pendente de HW: gravação conhecida + readback + flush + recovery + origem.
#
#   [s464] REGRESSÃO APIC EXTD CORRIGIDA + EVIDÊNCIA QEMU (detalhe em docs/memory/SESSION_464.md)
#     apic.rs (s463) testava IA32_APIC_BASE bit 11 (EN, sempre 1) como EXTD (bit 10,
#     SDM §10.4.4) -> fail-closed em TODO boot QEMU -> smp_online=1 (M5/mesh4/QEMU3
#     rodaram BSP-only; 10-04/05 = 4..6). FIX apic_base_extd() bit 10 + labels
#     EN/EXTD + teste regressão (k-nano 12/12, check 0 erros). EVIDÊNCIA smoke WHPX:
#     LAPIC inicializado + AP LAPIC 1..3 online + CorePools total=4 + smp_online=4
#     (logs/boot_whpx_20261010_221534.txt). Launchers: qemu_mem_preflight.ps1 novo
#     (RAM gate -Force/exit1 + WHPX probe); args dump por spawn (argv+host_free);
#     rotação de serial; run-qemu-whpx retry por $LASTEXITCODE (catch era dead code).
#     REFUTADO: "fallback WHPX→TCG sob pressão" — TCG era o default do p2p-mesh.
#     PENDENTE: reprise M5/mesh4 com APs online; x2APIC real se EXTD=1; lanes 6-7.
#
#   [s463] CAMPANHA DE CONFIABILIDADE DO MESH (detalhe em docs/memory/SESSION_463.md)
#     OOM root-caused: bump-first + dealloc no-op = churn permanente (~31,9KB/tick;
#     heap 512->1179MB em 34min) + PMM duplo-uso (residual SESSION_252 ora-1 FECHADO)
#     -> String::clone->memcpy #PF -> storm -> park BSP em T+37384/37067.
#     FIXES (todos com testes host): M1 preflight 64KB antes de serializar; M2/M2b
#     PeerState+backoff+retry limitado; M4a 9 testes FRAG/FRACK + TX guard honesto
#     (>64KB = false, era true mentiroso); M3c demand-page honesty (P|W recusa,
#     fail-open high-half removido); TALC-first pós-boot (route_alloc gate +
#     set_boot_phase_done no Runtime) = bump 12MB vs 1.155GB; PMM reserve (kernel
#     image + boot_ramlog) + detector duplo-uso + hang reserve_range corrigido;
#     EventBus CLONED_BYTES; SecurityAgent::alerts cap 64; GGUF payload gate
#     (payload curto = Err, era Ok).
#     VALIDAÇÃO: M5 2 instâncias T+79k (~72min) ZERO anomalias; Mesh4 4 nós × 4c/2G
#     via hub L2 (tools/run-mesh4-lab.ps1 NOVO) marco M3 cruzado, zero anomalias,
#     peers=3 estável. Logs: logs/boot_mesh_{a,b}_m5.txt + boot_mesh_{a..d}.txt.
#     PENDENTE: M4b transporte payloads grandes (teste direcionado); cap engine
#     neural-sgdb (fora do workspace); lanes 6-7 adiadas.
#
#   [s456] IMAGEM HW usb_hw.img PACK_LLM=all + rebuild do zero (detalhe em docs/memory/SESSION_456.md)
#     target/usb_hw.img 12,4 GB (ESP + DATA FAT32 0x0C). ESP kernel.elf sha=árvore;
#     DATA 71 entradas (1B/3B/7B/10B + slots + firmware). FIX tools/mkfat32.py:
#     LEARNER.BIN vinha com 544MB do Falcon3-1B (fallback antes do arquivo real) —
#     agora 117,9 MB corretos. Rebuild: cargo clean 43GB + build 1m58s 0 erros;
#     nsgdb v1.4.3 default+all-features 0 erros (mcp_server.exe vivo não relinkado).
#
#   [s455] MESH 2 instâncias 6c/6GB (re-teste pós-fix -cpu Haswell) + monitor 30 min
#     (detalhe em docs/memory/SESSION_455.md)
#     Run pós-S454: ambas bootação até PostRuntime, mesh convergiu (peers=1,
#     RX/TX ativo, CRDT, skill broadcast, segurança ok) e ficou estável ~27 min.
#     VERIFICADO: 0 #PF, 0 panic, 0 silence, 0 OOM, 0 [err], exc=6 (self-test
#     Ring3 demo), fail=2 (conhecidos: TICKV RAM + ELF truncado), peers=1 em
#     ambos (confirmado por grep direto no log — MESH_HEALTH peers=1 consistente).
#     P1 (behavioral, não falha): matmul barrier timeout em T+170/178 (pending=5
#     done=0, ok=false ~8.5s) — provável overhead SMP WHPX 6c; sistema continuou.
#     P2 (bug do monitor, não do sistema): parser do mesh_watch_30min.py reporta
#     peers=0 falso-positivo — canal [nk][FL] emite peers=0 que o parser pega
#     como último match, mas MESH_HEALTH peers=1 (confirmado). IDEA #656.
#     P3 (IDEA #657): matmul barrier timeout no boot — não se repitiu em 27 min.
#          Comparação S454 vs S455: boot #GP->boot ok; OOM T+6018->não repetiu;
#     pf 14->0; exc 15/8->6. QEMU vivos ao final (PID 10660/13552).
#     PLANO DE CORREÇÕES: docs/plans/plane-correções-mesh-2qemu.md
#     AÇÕES FERRAMENTA (não kernel): corrigir parser do mesh_watch (IDEA #656);
#     identificar caminho de shutdown (P1.2 do plano); rodar S455 novamente com
#     parser corrigido + CSV limpo. Zero .rs tocados.
#
#   [s453] LAB LOOP 6G/8c (detalhe em docs/memory/SESSION_453.md)
#     Run pós-S454: ambas bootação até PostRuntime, mesh convergiu (peers=1,
#     RX/TX ativo, CRDT, skill broadcast, segurança ok) e ficou estável ~27 min.
#     VERIFICADO: 0 #PF, 0 panic, 0 silence, 0 OOM, 0 [err], exc=6 (self-test
#     Ring3 demo), fail=2 (conhecidos: TICKV RAM + ELF truncado), peers=1 em
#     ambos (confirmado por grep direto no log — MESH_HEALTH peers=1 consistente).
#     P1 (behavioral, não falha): matmul barrier timeout em T+170/178 (pending=5
#     done=0, ok=false ~8.5s) — provável overhead SMP WHPX 6c; sistema continuou.
#     P2 (bug do monitor, não do sistema): parser do mesh_watch_30min.py reporta
#     peers=0 falso-positivo — canal [nk][FL] emite peers=0 que o parser pega
#     como último match, mas MESH_HEALTH peers=1 (confirmado). IDEA #656.
#     P3 (IDEA #657): matmul barrier timeout no boot — não se repitiu em 27 min.
#     Comparação S454 vs S455: boot #GP->boot ok; OOM T+6018->não repetiu;
#     pf 14->0; exc 15/8->6. QEMU vivos ao final (PID 10660/13552).
#     AÇÃO FERRAMENTA (não kernel): corrigir parser do mesh_watch para priorizar
#     peers do canal P2P/MESH_HEALTH, não do FL. Zero .rs tocados.
#
#   [s453] LAB LOOP 6G/8c (detalhe em docs/memory/SESSION_453.md)
#     `tools/run-qemu-lab-loop.ps1`: 6 GB / 8c / WHPX Haswell, HW simulado maximo
#     (OVMF pflash + IDE uefi/dados, e1000 slirp, intel-hda + hda-duplex,
#     qemu-xhci + usb-tablet/kbd, virtio-gpu, FALCON3.BIN + netmode.flag por
#     `-device loader`), restore pristine 4 GB por ciclo, veredito em CSV.
#     Ciclo 1 MEDIDO: PASS, 253.655 B, tick 26.206, 424 s, 0 #PF / 0 panic /
#     0 corrupcao / 0 exc em runtime. Teste do harness 10/10 EXIT=0.
#     Isolamento obrigatorio de outra thread (mata todo qemu-system-x86_64):
#     binario `target/lab/lab8c-vm.exe`, imagens `target/lab/*`, portas
#     4455/4456, monitor 5556+ciclo. Parar: logs/lab_loop.pid.
#     4 `[fail]` no boot sao da frente de STORAGE (TICKV put/get, NSGDB Q4,
#     Q-jump = AION) + self-test do ELF loader. NAO tocados.
#     Abertos na lane: `-VirtioBlk` (backend=file, degrau 2 do funil D3) tem
#     dry-run OK mas **runtime NAO medido** (IDE #651); e o expert de HW agora
#     CHEGA ao guest mas o parser rejeita (pede v3, artefato e v6 — #652).
#     Detalhe e as 6 armadilhas do instrumento: SESSION_453.md §6.

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
| Decode host .V7 (rig torch, greedy + rep 1.15 + mask 2137) | OK - 1B.V7 Q1+Q6 pass, 2.2-3.6 tok/s (~3-4x fonte); W2A8-host refutado; 7B/10B HYPOTHESIS (ferro insuficiente) |
| I2_S scalar (QEMU WHPX 4G/4c, Falcon3-3B) | MEDIDO - 0.013 tok/s (78s/tok); prefill 123s; sem ganho vs FP legacy (ambos soft-float escalar); default OFF mantido |
| 2B4T boot (QEMU WHPX 4G/4c) | FALHOU - Runtime OK mas load_model REJECT h=0 L=0 (v7 header vs v6 loader); requer v7 loader completo |
| Falcon3-1B 1.58b (host suite) | COERENTE - Q1/Q6 pass, ~2.7 tok/s CPU fp32; Q2-Q5 vermelhos por calibração 3B; .V7 693MB gate green |
| Falcon3-1B 1.58b (QEMU) | NÃO MEDIDO - WHPX falhou neste host → TCG 2c; loader QEMU carrega 3B fixo @0x100000000; TCG prefill 239s/L0 inutilizável |
| I2_S AVX2 (kernel) | IMPLEMENTADO - sign-select blend/vpsrav + hadd + vmulps + tail k%32; gate allow_avx2 (WHPX -cpu Haswell passa); parity test bit-exact vs scalar; dispatch matmul_hybrid |
| I2_S AVX2 (QEMU WHPX) | GATE OK - boot OK, LLM loaded, AVX2 enabled logado; tok/s NÃO medido (benchmark pulado no QEMU); requer HW real p/ speedup |
| Rep-penalty kernel | CONFIRMADO - argmax_row_hf_vocab + infer_queue run_fine + Medusa usam sampler com rep 1.15 + forbidden-id; sample_token_coherence fixado |
| E30 per-row scales | IMPLEMENTADO + TESTADO - write_q6k per_row + convert --per-row-scales; suite idêntica (2/6), zero impacto coerência; não adotar |
| E09 KV ternary recall | IMPLEMENTADO + TESTADO - quantize_kv_ternary + bench_kv_ternary; inconclusivo (modelo falha tarefa, 0% EM); medição memória errada; requer modelo fine-tuned QA |
| E06 draft-verify | PROTÓTIPO + TESTADO - --draft-verify + JSON schema state machine; quebrado (mismatches tokenizer); unconstrained greedy 100% JSON validity; requer rewrite |
| 2B4T boot (QEMU) | v7 HEADER MISMATCH - boot OK mas load_model REJECT h=0 L=0 (v7 header vs v6 loader); solução: converter .V7 p/ v6 header OU estender load_model_v6 p/ v7 |

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
