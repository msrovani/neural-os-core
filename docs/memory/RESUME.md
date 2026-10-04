# RESUME — como voltar a desenvolver o neural-os-core (e o forum OPCODE/1)

**Data do registro:** 2026-10-04 · **Estado:** v1.9.99-s447 TEST · **Branch:** `main`
**Tip verificado:** `556c8c64` = `origin/main` (conferido com `git ls-remote`, nao so
com o tracking ref). O tip anda a cada commit de doc; o que vale e o **estado
medido** abaixo, nao o hash. · **Registro completo:** [SESSION_447.md](SESSION_447.md)

> Regra que continua valendo depois de qualquer pausa: **codigo implementado nao e
> propriedade demonstrada**. `UNKNOWN != 0`, `HOST TEST PASS != RUNTIME PASS`,
> `QEMU PASS != METAL PASS`, `consenso de agentes != decisao humana`.

---

## 0. Em 30 segundos

- **O P0 e F1.5 end-to-end** (ordem direta do manteneder, HUMAN-0006). Funil
  obrigatorio (D3, HUMAN-0003), nessa ordem:
  1. estabilidade do boot/RAM (#630),
  2. **persistencia real em disco** (VirtIO-blk, `TICKV backend=file`),
  3. falsificador de **2 boots reproduzido**.
  Fora desse funil = proposta adiada (E1-E4, Ring3, proveniencia estao congelados).
- **Estado do F1.5:** boot 1 **OBSERVED** (gera e roda a skill no log), boot 2
  **FALSIFIED** (`escalate=reuse reason=not_found`). Causa isolada pelo AION:
  `try_mount_from_ckpt` ([tickv.rs:397](../../crates/k_nano/src/storage/tickv.rs#L397))
  reconstroi o indice **so do ultimo checkpoint** e nunca varre a cauda
  (append final do boot1 = 74752; indice reconstruido no boot2 = 53760).
  Dono: **AION** (AION-0009 causa, AION-0010 P0 com falsificador, em review no ORACLE).
- **Nao tocar** em `crates/k_nano/src/storage/tickv.rs` nem no caminho de storage
  sem o AION — sao 3 arquivos modificados de outra frente, sem commit.
- **Braco de ablacao (§7): IMPLEMENTADO em host (s448)**, falta o boot no metal.
  `tools/f15_pristine.py` (`ensure`/`restore`/`scan`) + bloco `[7]` do launcher:
  o disco de partida virou variavel declarada (`restore=`, `disk_lab_state_before=`
  no sidecar) e o parser reprova 3 situacoes: sidecar sem o registro, boot 1 que
  partiu de um disco que **ja tinha** a skill, e boot 2 com `restore=1`.
  Medido: o confound era real — `disk_qemu.raw` chegou a ter
  `skill/wasm/oracle_rt_expr_v1` em disco, e ~3 min depois estava limpo de novo
  (outra thread restaurou): o estado do lab e volatil, entao so um carimbo
  tirado **no boot** responde a pergunta.
- **Toda decisao do forum sem `from=HUMAN` e proposta** (D1). Quorum nao decide.

---

## 1. Ambiente (medido neste host, 2026-10-04)

| item | valor medido |
|---|---|
| workspace | `C:\DEV\neural-os-core-latest` |
| QEMU | `C:\Program Files\qemu\qemu-system-x86_64.exe` |
| OVMF code (ro) | `target/ovmf_code.fd` |
| OVMF NVRAM | template `C:\Program Files\qemu\share\edk2-i386-vars.fd`; `target/ovmf_vars.fd` **acumula e corrompe** (vars corrompido = serial de 0 bytes, medido 03/10) |
| Miri | instalado e funcional neste host |
| Kani | **ausente**: o guide oficial lista so linux-gnu/apple-darwin (x86_64/aarch64); WSL nao instalado → E1 formal nao roda aqui |
| n-sgdb MCP | servidor vivo (PID 17984, `C:\DEV\neural-sgdb\scripts\mcp-server.ps1`), mas **nao exposto nesta sessao** → sync do SESSION/IDEA no SGDB ficou pendente |

**Pegadinhas de build (custaram tempo):**

- `.cargo/config.toml` **nao** define target padrao → `cargo check --release` no host
  compila o bin para o host e inventa erro fantasma de `alloc_error_handler`/`lang item`
  (llicao SESSION_417). O build canonico e `cargo build --release -p boot`.
- Rebuild real ≈ **2m40s**. **0,17–0,31s = cache**. Forcar com
  `touch crates/neural-kernel/src/main.rs` ou `cargo clean -p neural-kernel`.
- **`cargo nk` NAO regenera `uefi.img`.** Fluxo canonico:
  `cargo build --release -p boot` → `python tools/build_image.py --bios` →
  **provar literal novo dentro de `target/uefi.img`** → so entao rodar o QEMU.
  Rodar o lab sem isso testa kernel stale (ja aconteceu nas rodadas 9/10 da s430).

---

## 2. F1.5 — comandos exatos

```powershell
# 0) artefato novo (build_image nao e opcional)
cargo build --release -p boot
python tools/build_image.py --bios

# 0b) braco de ablacao: snapshot do disco LIMPO (so cria se nao existir;
#     recusa fonte que ja tem a skill do lab -- exit 3)
powershell -File tools/run-f15.ps1 -PreparePristine

# 1) boot 1: gera a skill (LSK1 'G') e roda. -RestorePristine devolve o
#    snapshot ao disco ANTES do boot (opt-in: o disco do lab e compartilhado).
powershell -File tools/run-f15.ps1 -Boot 1 -RestorePristine -TimeoutSec 900
powershell -File tools/f15_parse.ps1 -Log logs/f15_boot1.txt -Boot 1

# 2) power cycle + boot 2: tenta recordar (LSK1 'E')
powershell -File tools/run-f15.ps1 -Boot 2 -TimeoutSec 900
powershell -File tools/f15_parse.ps1 -Log logs/f15_boot2.txt -Boot 2

# 3) veredito dos DOIS boots juntos (e aqui que a propriedade aparece)
powershell -File tools/f15_parse.ps1 -Compare "logs\f15_boot1.txt,logs\f15_boot2.txt"
```

- O `-Compare` e **sozinho**: um unico argumento com ** virgula ** e **sem** `-Log`.
  Passar `-Log` junto faz o parser reclamar `-Compare espera dois logs separados por
  virgula` (medido nesta sessao, custou duas rodadas).
- Exit code do parser: **0 = PASS, 1 = FALSIFIED**. O launcher nao tira veredito:
  ele so entrega log + status (por design).
- **Ablacao (§7):** `-PreparePristine` cria o snapshot (so de fonte limpa);
  `-RestorePristine` o devolve ao disco — **so no boot 1** (o launcher recusa no
  boot 2, porque o restore apagaria justamente o estado a provar). Sem o switch,
  o launcher roda o `scan` e registra `restore=0` + `disk_lab_state_before=0|1`.
  Diagnostico do disco: `python tools/f15_pristine.py check --disk target\disk_qemu.raw`.
- **§14 + §14b (s447):** o veredito so vale com o sidecar `<log>.imgid` ao lado do log.
  Ausente, `probe_na_imagem!=True`, ou **identidade divergente** → **FALSIFIED**.
  A identidade e `uefi_bytes` + `uefi_epoch` + `uefi_sha`: o parser recalcula os tres
  contra o arquivo **atual**. Sem isso o carimbo provava a imagem de agora num log
  antigo (foi exatamente o que aconteceu com o boot 1 real).
  `run-f15.ps1` grava o sidecar sozinho.
- O que o launcher faz por boot: chama `tools/gen_lsk1.py`, gera
  `target/lab_skill_boot{N}.bin`, copia `target/ovmf_vars_boot{N}.fd` **fresco**,
  e roda com `-cpu max` (TCG) ou `-cpu Haswell` (WHPX), `-smp $Cores`, `-m 4G`.
- Hash no log = **FNV-1a64 dos bytes do WASM** gravado em `skill/wasm/`, **nao** do
  texto-fonte. Por isso comparar exige os dois boots.
- Hook do lab: [skill_lab.rs](../../crates/hermes/src/skill_lab.rs), magic `LSK1`
  lido em `0x0211_0000`, 512 B.

**Veredito medido hoje (nao e hipoteese — sao exit codes rodados):**

| alvo | EXIT | veredito |
|---|---|---|
| `logs/f15_boot1.txt -Boot 1` | **1** | `a imagem mudou DEPOIS do boot` (sidecar 03:36:17Z vs arquivo 03:40:00Z) |
| `logs/f15_boot2.txt -Boot 2` | **1** | `escalate=reuse reason=not_found` + sem sidecar |
| `-Compare boot1,boot2` | **1** | `nao ha hash para comparar` |

O comportamento do boot 1 esta no log (`act=gen` + `run a=6 b=7 result=52`); o
**veredito** reprova. Nao trocar um pelo outro.

**Falta para o experimento fechar (além do fix do storage):**

1. **Braco de ablacao** — `PreparePristine` precisa restaurar o disco antes do run
   (hoje so copia). Sem ele nao ha controle "partiu de limpo".
2. `RELOAD_RETRIED` + campo `durable_unknown=` no slog (o `durable=0` atual reporta
   desconhecido como zero — ver [TODO.md](../../TODO.md), bloco s445).

---

## 3. Forum OPCODE/1 — estado no fechamento

- **Arquivo:** `C:\Users\msrov\OneDrive\Área de Trabalho\LOG AGENTES .txt`
- **Formato:** OPCODE/1, NDJSON, 1 mensagem por linha, append-only, UTF-8.
- **Censo no fechamento:** **577 mensagens** (394428 bytes); depois do meu post final
  (`FREEBU-0146`, `posttask`) e dos polls do OPMUSE que seguiram entrando: **588**.
  FREEBU 148 (ultimo `FREEBU-0146`) · OPCODE 118 · ORACLE 99 · OPMUSE ~101 (poll) ·
  CURAIX 55 · LIBRARIAN 20 · OPKIMI 16 · FIXER 11 · AION 10 · HUMAN 10 · SENTINEL 1.
- **Lock de escrita:** `.forum_write.lock` **ao lado do log** (nao no projeto),
  `O_CREAT|O_EXCL`, fail-closed, roubo de orfao por token com compare-and-delete.
  No fechamento: **nenhum lock orfao** (verificado).
- **Dups legados** (pre-lock, **nao reparados** — correcao = mensagem nova com `ref`):
  `FREEBU-0054`, `FREEBU-0056`, `OPKIMI-0007/8/9`.
- **Meu watcher foi PARADO no fechamento** (PID 17680). Para religar:
  `python -u tools/forum_watch.py --watch 480` (log em `target/forum_watch_run.log`,
  status em `target/forum_cycle.txt`). Ele so publica digest quando o ciclo traz
  substancia — ciclos so com heartbeat/poll ficam locais e **contados**
  (`digest_suprimido=N`).
- **Continua gravando depois do fechamento (nao e meu):** PID **9908**,
  `%TEMP%\opencode\forum_opkim_watch.ps1` (loop do OPMUSE, 1 tick/min, ja em
  `OPMUSE-POLL-84`). Ele tem **self-feed**: o corpo do poll contem a string `OPMUSE`
  que ele proprio procura, entao o laco nao se auto-encerra. Decisao do maintainer.

### Decisoes do mantenedor (HUMAN-0001..0009) — resumo operacional

| id | decisao | o que muda no dia a dia |
|---|---|---|
| D1 (HUMAN-0001) | fim da ilusao de consenso: nada vira `accepted` sem `from=HUMAN`; quorum = `proposed_by_agents` | nao anunciar task como concluida por consenso |
| D2 (HUMAN-0002) | zero commits/zero done sem evidencia no metal: log serial de boot real em QEMU/HW + comando + exit code + hash do log | host verde e condicao necessaria, nunca suficiente |
| D3 (HUMAN-0003) | estabilidade antes de capabilities; funil unico (RAM → disco → falsificador de 2 boots); E1-E4/Ring3/proveniencia congelados | trabalho fora do funil vira proposta, nao acao |
| D4 (HUMAN-0004) | higiene de escrita: CAS/lock entre ler `max_id` e gravar; telemetria interna vai para log de infra separado; log principal so para substancia | digest/heartbeat/poll nao vao para o log principal |
| D5 (HUMAN-0005) | matriz de dono: ORACLE=arquitetura/validacao · LIBRARIAN=SOTA/bench · FIXER=lanes de implementacao · **FREEBU/AION=memoria/harness/QEMU/runtime** · OPCODE=protocolo · OPKIMI=verificacao independente | proposta fora da propria lane vira input |
| TASK (HUMAN-0006) | parar eixo novo; forcar `TICKV backend=file dev=virtio`; rodar os 2 boots; trazer serial do boot 2 **sem regeneracao** | e o que esta em curso (AION) |
| CRIT (HUMAN-0007/0009) | risco de autoengano por excesso de infra; nao matar a arquitetura — tornar o F1.5 um experimento **implicavel** (hash, logs, ablacao, sem hardcode) | cada peça de infra precisa de prova por tras |

---

## 4. Working tree sujo — mapa de donos (nunca `git add -A`)

36 modificados + 15 untracked de **4 frentes**. Nada disso foi commitado e nada
disso e meu:

| dono | arquivos |
|---|---|
| **AION** (storage, s446+) | `crates/k_nano/src/storage/{tickv,flash}.rs`, `virtio_blk.rs`, `virtio_modern.rs` |
| **s441-s444** (heap/frag) | `k_nano/{allocator,lib}.rs` (+915 linhas no allocator), `k_ai/context_window.rs`, `k_ai/trust.rs`, `hermes/{hub_triage,anti_frag}.rs`, `tools/talc_frag_report.py`, `tools/test_talc_frag_report.py` |
| **s445** (boot/skills) | `k_nano/{boot_ramlog,boot_report}.rs`, `hermes/{skill_loader,wasmi_rt,evolve,agents,fs/*}.rs`, `hermes/skill_lab.rs`, `agent-core/lib.rs`, `event-bus/{bus,lib,stamp}.rs`, `k_nano/bench_stats.rs`, `logwriter-efi/main.rs`, `neural-kernel/{main,interrupts_ext,shutdown}.rs`, `jarbas/audio/jarvis.rs` |
| **outros** | `cortex/{cortex,infer_queue}.rs`, `k_nano/{net/mesh,silence_watchdog,smp/percpu}.rs`, `.github/workflows/ci.yml`, `ROADMAP.md`, `docs/architecture/INDEX.md`, `AGENTS.md`, `docs/memory/SESSION_441..443.md`, `docs/architecture/0113-*.md`, `sgdb_memory.db`, `tools/{bench_boot,forum_loop_opkimi,forum_freebu_intro,measure_repo}.ps1/.py`, `tools/.forum_loop_state/` |

Regra: `git add <arquivo>` por arquivo. Antes de comitar `.rs` de outra frente,
confirmar com o dono (D5).

---

## 5. Armadilhas medidas (custaram tempo nesta sessao — nao repetir)

1. **`-match`/`-notmatch` sobre ARRAY** no PowerShell devolve os **elementos**, nao
   booleano → a asserção mente. Aconteceu **3×**. Antes de casar:
   `$joined = $arr -join "`n"`.
2. **`"k=" + (expr)` dentro de array literal** parte o campo em **duas linhas** (o
   `+` liga diferente da vírgula) → sidecar truncado. Usar `("k={0}" -f $v)`.
3. `-like '# --- [14]*'` → `[14]` e **character class**. Usar `.StartsWith(...)`.
4. `Start-Process -PassThru` → `.ExitCode` so vem preenchido se `.Handle` foi
   cacheado **antes** da espera; senao `null` e `$code -ne 0` e TRUE (faz `throw`
   ate em exit 0). Era esse o bug do check de exit do launcher.
5. `tasklist /FI "PID eq N"` devolve **returncode 0 para PID inexistente** → provar
   vida por **stdout**, nunca por exit code.
6. `os.open(O_EXCL)` devolve **EACCES** (nao EEXIST) quando o arquivo esta
   delete-pending (Windows) → matava o contender em vez de fazer retry.
7. Git Bash traduz `/FI` para caminho (`C:/Program Files/Git/FI`) → medir via Python.
8. Heredoc do shell (`<<'PY'`) mangleia `\\` e `\n` em patch → usar `write_file`.
9. **Serial sob SMP perde linhas inteiras** → evidencia canonica e o `BOOT.LOG` em
   disco + hash, nunca amostra filtrada por `[T+]`.
10. **TSC sempre DEPOIS de qualquer log** — instrumento no caminho mede o instrumento
    (o `sync_us` de 4,2 ms da s412 era ~95% log).
11. Ferramentas `.ps1`/`.py` **ASCII-only** (PS 5.1 le como cp1252; nao-ASCII vira
    lixo **sem erro de sintaxe**).
12. **`|` dentro de uma celula de tabela markdown parte a linha.** `O_CREAT|O_EXCL`,
    `restore=1|0` e `lab_state=0|1` quebraram linhas minhas em tres sessoes. Escapar
    `\|` e, para conferir, contar colunas com split em `(?<!\\)\|` — o split ingenuo
    conta um `\|` ja escapado como separador e da o veredito errado (falso
    "quebrada" numa linha boa).
13. **Numero de serieo entre duas linguagens e contrato:** string ISO de mtime perde
    1 ULP (Python vs .NET) **e** `/` no PowerShell e divisao em double com cast
    arredondando — `Ticks` (~1,8e17) passa de 2^53. Formula exata, igual nos dois
    lados: `(($ticks - ($ticks % 10000000)) / 10000000)` = truncamento (o Python
    `int()` trunca).
14. **Artefato do lab e estado compartilhado e pode estar TRAVADO** por outro
    QEMU/build: `Get-FileHash target/uefi.img` falha com "usado por outro
    processo" (medido s448). O carimbo degrada para `uefi_sha=ERRO:lido-em-uso` e
    o parser reprova; o teste de stamp sai **2 (inconclusivo)**. Nunca tratar
    "nao consegui ler" como "deu certo".

---

## 6. Gates baratos (rodam sem QEMU) — usar antes de gastar boot

| gate | comando | esperado |
|---|---|---|
| suite do veredito F1.5 | `python tools/run_f15_fixtures.py` | **26/26**, exit 0 (gerador em `tools/gen_f15_fixtures.py`, versionado) |
| braco de ablacao | `powershell -File tools\test_f15_ablation.ps1` | **13/13**, exit 0 (executa o bloco `[7]` REAL sobre discos de 4 MB) |
| carimbo §14 | `powershell -File tools\test_f15_stamp.ps1` | exit 0, 15 campos do sidecar coerentes com os arquivos (roda com `restore=0` de proposito: nao pode mexer no disco compartilhado) |
| lock do forum | `python tools/test_forum_lock.py` | 0 id dup, 0 linha rasgada |
| carimbo §14 | (movido p/ a linha de cima: `tools/test_f15_stamp.ps1`) | — |
| UB host | `cargo miri test -p k_ai --lib trust::tests::revoke_is_transitive_and_bumps_generation -- --test-threads=1` | 1 passed, 72 filtered |
| build | `touch crates/neural-kernel/src/main.rs && cargo build --release -p boot` | 0 erros (rebuild real ~2m40s) |

---

## 7. Fila sugerida (ordem do funil, nao do entusiasmo)

1. **Perguntar ao maintainer** se o loop do OPMUSE (PID 9908) deve continuar — e a
   unica coisa que ainda escreve no log "fechado".
2. **Um boot 1 de verdade com `-RestorePristine`** — é o que falta para o braco de
   ablacao sair de IMPLEMENTADO e virar OBSERVED (D2). So com o disco do lab livre:
   `target/disk_qemu.raw` e estado compartilhado e outra thread bootava nela.
3. **So depois do fix do AION**: rodar os 2 boots e trazer o veredito **com sidecar
   `.imgid`, identidade §14b e registro de ablacao §7 valendo**. Sem isso nada fecha (D2).
4. **F1** (1h sem `#PF`, writer do wild-write ainda nao localizado) via
   `tools/watch_corruption.ps1` — depois do funil, nunca antes.

## 8. O que ficou UNKNOWN (nao afirmar o contrario)

- `cargo check --release` **desta** arvore depois das mudanças das outras frentes (o
  ultimo rebuild real 0-erros e da s446, antes dessas 36 alteracoes).
- Testes de `skill_lab`, serial QEMU, `TICKV backend=file` no QEMU, `[RECOVER]` no
  `BOOT.LOG`, string nova no `uefi.img` — todos UNKNOWN nesta arvore.
- O 3º boot 1 (00:39, QEMU PID 25484) foi **cortado no fechamento**; log preservado em
  `logs/f15_boot1.orphan_cut.txt` (2954 linhas). O `logs/f15_boot1.txt` é **esse** run
  (o launcher apaga o log no começo), então tem sidecar — e reprova por `§14b` (a imagem
  foi reconstruida depois do boot). Fechado como **UNKNOWN** por não ter Veredito limpo.
- E1/Kani: sem toolchain neste host. F1: writer nunca localizado (IDEA #632/#633 🟡).
- **Braco de ablacao no metal**: nenhum boot de QEMU rodou com `-RestorePristine`;
  o restore de 3 GB tambem nao foi exercitado (os testes usam discos de 4 MB).
- Sync n-sgdb: servidor vivo mas sem tool nesta sessao → SESSION_447 e as IDEA
  #638/#639/#640/#641 ainda **nao** estao no SGDB.