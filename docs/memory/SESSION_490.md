# SESSION_490 — #PF storm: captura do writer com watchpoint (fim do "IP migrante")

**Data:** 2026-10-18
**Contexto:** bug aberto desde o s479, herdado como "F1 / #PF storm, IP migrante, precisa
watchpoint". A investigação anterior tentava simbolizar o IP contra `target/limine-esp-tree/kernel.elf`
e concluía "endereço de runtime e ELF não compartilham faixa".

## 1. O erro de método: era o ELF errado, não o IP

O log `logs/boot_whpx_20261017_190455.txt` registra, para cada #PF, os 16 bytes em `[PF_DBG] ip_bytes`.
No IP `0xffffffff808f60d4` (build de 17/10) os bytes são:

```
89 14 88 EB 81 48 8B 7C 24 70 48 C7 ...
```

No ELF **de hoje**, o mesmo endereço contém `e8 bb f2 ff ff 48 83 ...` (meio de um `call`).
Fingerprint não casa ⇒ **o ELF usado para simbolizar não era o que bootou**. O "IP migrante"
era, em parte, artefato de simbolizar builds diferentes.

## 2. Identificação do writer (estática, com prova de unicidade)

Busca do padrão de bytes no ELF atual (parse dos program headers → vaddr):

```
89 14 88 EB 81 48 8B 7C 24 70  ->  hit ÚNICO em 0xffffffff8092b7b4
```

`objdump --disassemble` dessa região = `cortex::parallel_matmul::ternary_worker`:

```
mov   0xd0(%rsp),%r8          ; r8 = ctx
mov   0xd8(%rsp,%rsi,4),%edx  ; valor
mov   0x10(%r8),%rax          ; rax = ctx.c_ptr    (TernaryJobCtx: w_ptr 0x00, x_ptr 0x08, c_ptr 0x10, m 0x18, k 0x20, n 0x28)
mov   0x28(%r8),%r8           ; r8  = ctx.n
imul  %r8,%rcx ; add %rdi,%rcx ; add %rsi,%rcx
mov   %edx,(%rax,%rcx,4)      ; STORE  <- cr2 = c_ptr + idx*4
```

`TernaryJobCtx` (parallel_matmul.rs) confirma os offsets. Com `cr2` pequeno (0x2feb8, 0x2ffb8,
0x2cfb8, 0x26fb8 — todos múltiplos de 4, < 0x30000) e `err=0x2` (write, supervisor), a única
leitura compatível é **`c_ptr == 0`**.

## 3. Reprodução em runtime (TCG, 4 de 7 boots)

`tools/watch_pf_writer.ps1` (QEMU TCG 5G/8c + gdbstub) reproduziu o storm em **4 de 7** execuções,
**sempre no mesmo IP** — o store identificado acima:

```
[PF_DBG] ip=0xffffffff8092b7b4 cr2=0x000000000002feb8 err=0x0000000000000002
[EXC] #PF storm ip=0xffffffff8092b7b4 cr2=... streak=3+ park core (fail-closed)
```

Isso fecha a dúvida do item 1: sob TCG o IP **não migra** — é o mesmo store, em todo boot.

## 4. Watchpoint: o que ele capturou

`tools/watch_pf_writer.ps1` + `tools/pf_ctx_watch.py` (engine gdb-Python):

- breakpoint no publish (`AtomicPtr<TernaryJobCtx>::store`, `rsi` = ctx);
- a cada publish, arma/renova watchpoint de escrita em `ctx+0x10` (c_ptr), mantendo os 3
  últimos endereços (o bloco do heap é reciclado em poucos endereços);
- só retém o alvo quando o valor **vira zero**.

Captura literal (`logs/wpf_gdb_20261018_131543.txt`):

```
[WP] store T_CTX rsi=0x400000e197c0      (publish)
[WP] PUBLISH ctx=0x400000e197c0 c_ptr=0xe91200 k=3072 n=3072
[WP] watch ARMED em 0x400000e197d0
[WP] store T_CTX rsi=0x0                 (fim do dispatch -> ctx dropado)
Thread 7 hit Hardware watchpoint 2: *(unsigned long*)($ctx+0x10)
Old value = 15274496 (0xe91200)   New value = 56 (0x38)
0xffffffff80aa0edc in <talc::talc::Talc<talc::oom_handler::ErrOnOom>>::register_gap
```

⇒ **o bloco do `TernaryJobCtx` é liberado e reciclado pelo allocator dentro do mesmo ciclo de
dispatch** (metadados de gap/free-list no offset 0x10). O endereço de ctx se repete entre
dispatches (`0x4000008c6ee0`, `0x400000e19660`, `0x4000008c96d8`, ...).

**O write que zera `c_ptr` do ctx que falhou NÃO foi capturado** nas execuções (nem com 3
endereços vigiados). Logo, a origem exata do 0 segue **UNKNOWN**.

## 5. Mecanismo (HYPOTHESIS forte, não VERIFIED)

- `ap_work::job_done()` credita a barreira **global** `DONE`, sem identificar o dispatch.
- `ACTIVE_EPOCH` / `bump_epoch()` existem mas têm **0 callers** (grep em `crates/`), e
  `try_dequeue` não filtra por epoch — apesar de o doc do módulo prometer "APs só executam se
  epoch == ACTIVE_EPOCH".
- Um job atrasado (AP que pegou `HEAD` antes de `clear_queue`, ou que ainda está no loop)
  chama `job_done()` já na barreira **nova** → `DONE >= PENDING` antes de todos os workers reais
  terminarem → BSP faz `T_CTX.store(null)`, dropa o `Box<TernaryJobCtx>` e o worker atrasado
  segue escrevendo em memória liberada/reciclada → `c_ptr` lido como 0 → storm.

Assinatura que sustenta: `worker_max_us` de 6 s num matmul 16×3072×3072 (timing largo), storm
~2,7 s depois, sempre no mesmo store, e o bloco comprovadamente reciclado após o clear.

## 6. Classificação (ADR-0113)

| Claim | Estado |
|---|---|
| Store que falha = `ternary_worker` (`cortex::parallel_matmul`) | **VERIFIED** (runtime, 4 boots, mesmo IP) |
| `c_ptr == 0` no fault | **VERIFIED** (cr2 pequeno + err=write + disasm) |
| Bloco do ctx reciclado pelo allocator após o dispatch | **VERIFIED** (watchpoint: `talc::register_gap` em ctx+0x10) |
| Origem do `c_ptr = 0` (quem zera) | **UNKNOWN** (watchpoint não viu o write zero) |
| Barreira libera cedo por `job_done` de job atrasado | **HYPOTHESIS com evidência experimental** (falsificador: 0/4 storms após o fix vs 4/7 antes; prova direta do arrival stale ainda ausente) |
| `epoch` protegia o dispatch | **REFUTADO no código anterior** (`bump_epoch`/`ACTIVE_EPOCH` com 0 callers) — agora wired |
| Fix epoch/participação elimina o storm observado | **VERIFIED em TCG** (4/4 boots, 0 PF) · metal UNKNOWN |

## 7. Correção implementada (epoch + participação na barreira)

`crates/k_nano/src/smp/ap_work.rs`:

- `JobSlot` ganhou `epoch: u32` e `counts: bool`; `enqueue` estampa a epoch corrente.
- `reset_barrier(pending)` agora **bumpa a epoch** antes de zerar `DONE` (arrival atrasado já nasce stale) e devolve a epoch.
- `job_done(epoch, counts) -> bool`: só incrementa `DONE` se `counts && epoch == ACTIVE_EPOCH`.
- `try_dequeue() -> Option<QueuedJob>` carrega epoch/counts; o slot vazio também credita via `job_done(epoch, counts)`.
- Novo `enqueue_barrier(f, job_id)` = job que participa da barreira.

`crates/cortex/src/parallel_matmul.rs`: os dois dispatches (`matmul_worker`, `ternary_worker`) passaram a usar `enqueue_barrier`. Job de outro agente (`hermes::ap_jobs::enqueue`) segue como não-participante — roda, mas não libera barreira alheia.

Prova host: `cargo check --release` 0 erros; `cargo test -p k-nano --lib smp::ap_work -- --test-threads=1` **5/5** (dois testes novos: `stale_job_done_nao_credita_barreira_nova`, `job_nao_participante_nao_credita`).

## 8. Falsificador: 4 boots TCG (mesmo setup da reprodução)

| | storms | PF_DBG | matmul enter | BOOT SCORE | T+ final |
|---|---|---|---|---|---|
| Baseline (antes do fix, 7 runs) | 4 de 7 | até 22 | — | ok | ~3300 (storm) |
| **Depois do fix (4 boots)** | **0/4** | **0** | 5–6 | 2 | 5559–13251 |

O caminho foi exercitado de verdade: `matmul exit m=16 k=3072 n=3072 workers=8 worker_max_us=10255863` (aps=7). Nenhum `#PF` em nenhum dos 4 boots, inclusive nos que passaram de T+13000 (4× além da janela onde o storm aparecia).

Classificação: **fix IMPLEMENTED + runtime-VERIFIED em TCG (QEMU)** para o storm observado; **metal = UNKNOWN**.

## 9. Pendências que sobram

1. **Contador de arrivals stale**: o fix já ignora o arrival de leva antiga; falta
   **observá-lo** (um `STALE_DONE` atômico + 1 log no primeiro) para provar que a cadeia
   hipotetizada ocorria de fato — hoje a evidência é o falsificador 0/4 vs 4/7.
2. **Aceite em metal** do fix (TCG é o único ambiente com reprodutibilidade; metal = UNKNOWN).
3. **F1 1h 8c/8GB** — agora desbloqueado do storm, ainda não executado.
4. Watchpoint com 4 slots e log de **todo** write (não só zero) para o ctx do fault, se o
   canary/contador não bastar para fechar a origem do `c_ptr = 0`.

## 10. Como reproduzir

```powershell
# captura (watchpoint)
powershell -NoProfile -ExecutionPolicy Bypass -File tools\watch_pf_writer.ps1 -WaitSec 600 -RamGB 5 -Smp 8
# falsificador do fix: 4 boots TCG de 320 s, procurando "PF storm" no serial
tools\epoch_falsifier_boots.sh 320 4   # 4 boots TCG; cmdline QEMU completa no proprio script
```

Endereços absolutos no script valem para o ELF atual — regenerar após rebuild
(`nm -C --numeric-sort target/limine-esp-tree/kernel.elf | grep parallel_matmul`).
