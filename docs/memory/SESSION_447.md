# SESSION_447 — Fechamento: carimbo §14 no harness F1.5, watcher comriterio de substancia, forum encerrado

**Data:** 2026-10-04
**Versao:** v1.9.99-s447 TEST
**Branch:** `main` · **HEAD:** `2a8f95b2` (= `origin/main`, verificado com `git ls-remote`)
**Alcance:** ferramentas de host (`tools/`). **Zero arquivo Rust tocado nesta sessao.**
**Forum:** `LOG AGENTES .txt` (OPCODE/1), identidade FREEBU. Posts: FREEBU-0106,
0107, 0114, 0128 (s446) + **0106/0144/0145** nesta. Runbook de retomada:
[RESUME.md](RESUME.md).

---

## 1. Missao §14 — o veredito de runtime so vale se o artefato bootado for identificavel

**Problema:** uma `uefi.img` em reconstrucao ja me deu serial de 0 bytes e eu culpei
o launcher. A regra que faltava era a da propria missao: *imagem bootada, nenhuma
conclusao de runtime sem demonstrar o artefato*.

**Correcao (2 arquivos, +44 linhas):**

- `tools/run-f15.ps1`: grava sidecar `<log>.imgid` com bytes + mtime da `uefi.img` e
  do disco, mais um **probe**: o literal `SKILL_LAB` procurado no
  `crates/hermes/src/skill_lab.rs` (fonte) e dentro da `target/uefi.img` (imagem).
- `tools/f15_parse.ps1`: o sidecar passou a ser **exigencia**. Ausente →
  `FALSIFIED ... sem identidade do artefato bootado`. Presente mas
  `probe_na_imagem!=True` → `FALSIFIED ... a imagem bootada NAO contem o literal da
  fonte` (ou seja: veredito sobre imagem stale). **Prova faltando e FALSIFIED, nunca 0.**

**Bug real que o teste pegou (nao era teoria):** `"uefi_bytes=" + (Get-Item ...)` dentro
de um array literal do PowerShell **parte o campo em duas linhas** — o `+` liga
diferente da vírgula. A identidade saia truncada e o parser lia um sidecar incompleto.
Corrigido com `("k={0}" -f $v)`.

**Medido:**

| check | resultado |
|---|---|
| `target/test_imgid_stamp.ps1` (executa o bloco §14 **real**, extraido por linha do launcher) | **EXIT=0**, `uefi_bytes=134217728`, `disk_bytes=3221225472`, `probe_na_imagem=True` |
| `python tools/run_f15_fixtures.py` | **22/22, EXIT=0** (15 casos antigos inalterados + 3 casos §14 falhando fechado) |
| `cargo build --release -p boot` (rebuild forcado com `touch main.rs`, 2m40s) | **0 erros** |

**Reclassificacao explícita (o ponto honesto da sessao):** veredito final medido nos
logs reais, **ambos reprovados**:

| alvo | EXIT | veredito (motivo) |
|---|---|---|
| `logs/f15_boot1.txt -Boot 1` | **1** | `a imagem mudou DEPOIS do boot (mtime sidecar=03:36:17Z agora=03:40:00Z)` |
| `logs/f15_boot2.txt -Boot 2` | **1** | `escalate=reuse reason=not_found` **e** `sidecar .imgid ausente` |
| `-Compare boot1,boot2` | **1** | `nao ha hash para comparar` |

O "boot1 PASS" da s446 **nao se sustenta**; e o `PASS` intermediario que o §14
dava no boot 1 tambem nao se sustenta (secao 14b). A evidencia de comportamento
(`act=gen` + `run a=6 b=7 result=52`) continua no arquivo; o veredito e outro.
Registrado assim, sem reescrever log.

### 1b. §14b — a checagem mediava a imagem errada (achado no proprio dia)

Escrevi a §14, rodei a suite (verde), e so depois fui olhar o **log real**: o
sidecar de `f15_boot1` diz `uefi_mtime=03:36:17Z`, e o arquivo no disco era de
`03:40:00Z`. Ou seja, `probe_na_imagem=True` era lido **no parse** — se a imagem foi
reconstruida depois do boot, a checagem prova o codigo novo num log antigo. **Um
carimbo que passa sobre a imagem errada e pior do que nenhum carimbo.**

Correcao: o sidecar grava tambem `uefi_epoch` (segundos inteiros) e
`uefi_sha` (16 hex de SHA256); o parser recalcula os tres contra o arquivo atual e
reprova se qualquer um divergir (`a imagem mudou DEPOIS do boot`). Efeito medido no
boot 1 real: passou de PASS para **FALSIFIED** — que e o veredito correto.

Bug de precisao pego pela propria suite: comparar a string ISO de mtime reprovava
casos legitimos (`...651Z` no Python vs `...652Z` no .NET, 1 ULP). Resolvido com
epoch em segundos inteiros (`Ticks - 621355968000000000) / 10000000`), que e exato
nos dois lados.

**Durabilidade da suite:** `tools/run_f15_fixtures.py` chamava
`target/gen_f15_fixtures.py` — **gitignored**. Um `git clean` apagava o gerador e a
suite inteira. Movido para `tools/gen_f15_fixtures.py` (versionado), com imagem de
identidade **deterministica** criada pelo proprio gerador (o sidecar aponta para um
arquivo que existe, com bytes/mtime/sha lidos de verdade) e mais um caso negativo
(`b1_imgid_mudou`: so o epoch muda).

---

## 2. Watcher do forum: digest so quando o ciclo traz substancia

**Medido antes do fix:** 60 polls do OPMUSE + 100 digests meus `FREEBU Ciclo N` =
**160 de 541 mensagens = 30%** do log inteiro, sendo que ~2/3 eram ruido meu.

**Mecanismo:** o loop do OPMUSE conta como mencao qualquer linha que contenha a string
`OPMUSE`; meu digest nomeava o OPMUSE (`...nova(s) de OPMUSE lidas... ids: OPMUSE-POLL-60`).
Ou seja, **eu alimentava o poll que me notificava** — dois loops se alimentando, sem
informacao util em nenhum dos dois lados. E e exatamente o "watchers/polling/digest"
que o mantenedor apontou como excesso de infra sem prova (HUMAN-0007).

**Correcao:** `tem_substancia` (funcao pura) decide se o ciclo vai ao log; ciclo so
com heartbeat/poll fica local (stdout + `target/forum_cycle.txt`) e a supressao e
**CONTADA** (`digest_suprimido=N`) para nao virar silencio invisivel. A deteccao de
mencao real continua intacta.

- `tem_substancia`: **4/4** (so poll -> nao; so heartbeat -> nao; mensagem real -> sim;
  poll+mensagem -> sim). O vies e explicito no docstring: errar para "substancia" custa
  uma mensagem a mais; errar para "heartbeat" mantem o laco vivo.
- **Verificado ao vivo:** 2 ciclos `digest suprimido`, 1 digest de substancia publicado
  (FREEBU-0142, proposta do AION sobre o F1.5). Ultimo ciclo antes do fechamento:
  `ciclo=20 ... digest_suprimido=1`.

---

## 3. Fechamento: processos, orphans e o forum

O maintainer avisou que o forum fecha porque os agentes estao parando. Cleanup medido:

| processo | PID | acao | prova |
|---|---|---|---|
| meu watcher do forum | 17680 | **parado** | `tasklist` sem o PID no stdout (conteudo, nao returncode — licao s446) |
| meu launcher F1.5 (boot 1 das 00:39) | 23068 | **parado** | idem |
| QEMU filho desse launcher (TCG, 4c, 856MB) | 25484 | **parado** | idem |
| loop do OPMUSE (nao e meu) | **9908** | **deixado vivo** — decisao do maintainer | `OPMUSE-POLL-84` |

- Log do boot cortado **preservado**: `logs/f15_boot1.orphan_cut.txt` (2954 linhas).
  Veredito **UNKNOWN** (o parser nao rodou; o boot foi interrompido no meio).
- **Nenhum lock orfao** no fechamento: `.forum_write.lock` ausente ao lado do log
  (o watcher morreu fora da secao trancada; reconferido depois do meu post final).
- **Censo no fechamento: 577 mensagens** (394428 bytes). Apos o meu post final
  (`FREEBU-0146`, type `posttask`, gravado sob o lock compartilhado) e os polls do
  OPMUSE que continuaram entrando: **588**. FREEBU 148 (ultimo `FREEBU-0146`) ·
  OPCODE 118 · ORACLE 99 · OPMUSE ~101 · CURAIX 55 · LIBRARIAN 20 · OPKIMI 16 ·
  FIXER 11 · AION 10 · HUMAN 10 · SENTINEL 1.
- **Self-feed do OPMUSE confirmado em log:** `OPMUSE-POLL-84` diz
  `"1 mencao(oes) a OPMUSE detectada(s)"` — a propria linha do poll contem `OPMUSE`,
  que e a string que ele procura. O laco **nao se auto-encerra** enquanto rodar.

---

## 4. Git: 2 commits publicados, nenhum `.rs`

| commit | arquivos | conteudo |
|---|---|---|
| `4ea43987` | `tools/forum_watch.py` (+42/−7) | gate `tem_substancia` (secao 2) |
| `2a8f95b2` | `tools/run-f15.ps1`, `tools/f15_parse.ps1`, `tools/run_f15_fixtures.py` (+44) | carimbo §14 (secao 1) |
| (s447, fechamento) | `tools/f15_parse.ps1`, `tools/run-f15.ps1`, `tools/run_f15_fixtures.py`, `tools/gen_f15_fixtures.py` (novo) | **§14b** (identidade do artefato no parse) + gerador versionado + caso `b1_imgid_mudou` |

- Push: `5c8dd9f2..2a8f95b2 main -> main`, **exit 0**.
- `git rev-parse HEAD` == `git rev-parse origin/main` == `git ls-remote origin
  refs/heads/main` = `2a8f95b2`.
- `git diff --name-only origin/main..HEAD -- '*.rs'` → **vazio**.
- Working tree segue suja com **outras frentes** (36 modificados + 15 untracked),
  mapeados por dono em [RESUME.md](RESUME.md) §4. `git add` foi sempre por arquivo.

---

## 5. Confirmado que a imagem **nao** estava stale

Auditoria do outro lado da §14: 7/7 literais de codigo dos arquivos que o AION mexeu
estao presentes dentro de `target/uefi.img`. O unico "ausente" era um **comentario**
(`virtio_blk.rs:249`). Ou seja: `hash(fonte) != hash(imagem)` **nao** esta em jogo
nesta arvore — o que falta e o backend persistente, nao uma imagem velha.

---

## 6. F1.5: o que continua aberto e de quem e

| item | dono | estado |
|---|---|---|
| `TICKV backend=file` no QEMU + os 2 boots | **AION** | AION-0009 (causa: `try_mount_from_ckpt` so do ultimo ckpt, nunca varre a cauda: append final boot1 = 74752 vs indice reconstruido boot2 = 53760) · AION-0010 (P0 com falsificador) em review no ORACLE |
| braco de **ablacao §7** | FREEBU | `PreparePristine` so faz snapshot; o comentario em `run-f15.ps1:41` promete restore inexistente → **IDEA #638** |
| `RELOAD_RETRIED` + `durable_unknown=` | livre | TODO s445; `durable=0` reporta desconhecido como zero |
| F1 (1h sem `#PF`, writer do wild-write) | livre | writer nunca localizado (IDEA #632/#633 🟡); arnes `tools/watch_corruption.ps1` pronto |
| E1/Kani | — | sem toolchain neste host (guide oficial so cobre linux-gnu/apple-darwin; sem WSL) |

---

## 7. UNKNOWN / limitacoes (nao afirmar o contrario)

- `cargo check --release` **desta** arvore depois das 36 alteracoes das outras frentes:
  o ultimo 0-erros (2m40s) e da s446, **antes** delas.
- Testes de `skill_lab`, serial QEMU, `TICKV backend=file`, `[RECOVER]` no `BOOT.LOG`,
  string nova no `uefi.img`: UNKNOWN nesta arvore.
- O 3º boot 1 (00:39) foi cortado no fechamento: log preservado, **veredito UNKNOWN**.
- Sync n-sgdb: o servidor esta vivo (PID 17984) mas nao ha tool MCP nesta sessao →
  este SESSION e as IDEA #638/#639 ainda nao estao no SGDB.
- Nenhum ADR novo foi escrito (escopo da sessao: audit/evidencia), e por isso nenhum
  registro em `docs/architecture/INDEX.md`.

## 8. Licoes (a 3ª vez que a 1ª aparece — por isso vai para AGENTS.md)

1. **`-match` sobre ARRAY no PowerShell devolve os elementos, nao um booleano.**
   Aconteceu 3× nesta sessao, sempre no sentido de "passou". `$arr -join "`n"` antes
   de casar, e `assert` no resultado — nao no intent.
1b. **Carimbo que passa sobre a imagem errada e pior que nenhum carimbo.** O probe
   lido no parse prova o artefato de AGORA. Comparar identidade (bytes+epoch+sha) e
   o que separa "provei o codigo novo" de "provei que o codigo novo existia".
1c. **Suite verde nao prova que a medicao media a coisa certa.** A §14 passou 21/21 e
   reprovava um boot real valido; o defeito so apareceu ao olhar o sidecar real.
2. **`"k=" + (expr)` dentro de array literal parte a linha.** O `+` liga diferente da
   virgula: um campo vira dois. `("k={0}" -f $v)`.
3. **Veredito de runtime sem identificar o artefato medido e afirmacao sobre nada.**
   Sem sidecar, `UNKNOWN` de imagem vira `FALSIFIED` por ausencia — nunca por default.
4. **Eu alimentava o poll que me notificava.** Filtro por nome proprio no digest fecha
   o self-feed; e supressao de telemetria precisa ser **contada**, senao vira silencio.
5. **Parar processos e parte do registro:** antes de matar, preservar o log e provar a
   morte pelo stdout do `tasklist` (returncode 0 = PID inexistente).