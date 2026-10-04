# SESSION_446 — F1.5 veredito (PASS/FALSIFIED) + probe Miri + lock de escrita do forum

**Data:** 2026-10-04
**Versao:** v1.9.99-s446 TEST
**Branch:** `main`
**Alcance:** ferramentas de host (`tools/`). **Zero arquivo Rust tocado nesta sessao.**
**Foro:** `LOG AGENTES .txt` (OPCODE/1). Identidade: FREEBU. Posts desta sessao: FREEBU-0106, 0107, 0114, 0128.

---

## 1. F1.5 — o veredito saiu de leitura humana para exit code

O pedido era: `run-f15.ps1` com OVMF code+vars, `-cpu` fixado e exit code do QEMU
checado; depois os DOIS boots e o parse por `f15_parse`.

**Launcher** (`tools/run-f15.ps1`): pflash duplo (`ovmf_code.fd` ro +
`ovmf_vars_boot{N}.fd` rw, cópia fresca por boot), `-cpu Haswell` em WHPX /
`-cpu max` em TCG, orçamento de tempo (`-TimeoutSec`) e códigos de saída distintos
(`2` = timeout, `3` = log de 0 bytes).

**Defeito encontrado depois de rodar os boots:** o check de exit code **existia mas
estava quebrado**. `Start-Process -PassThru` só preenche `.ExitCode` se o *handle*
foi cacheado antes da espera; sem isso ele volta `null`, e `$code -ne 0` com
`$code` null é **true** — o launcher fazia `throw "qemu saiu "` (código vazio na
mensagem) até num QEMU que tinha saído 0. Os três `.run.log` capturados mostravam
`[f15] qemu exit= log=0 bytes` + throw; eu tinha lido aquilo como falha do QEMU
(também era verdade, por outros motivos) e nunca reparei no código vazio.
Fix: `$null = $proc.Handle` + `Refresh()`, e uma guarda que falha em vez de passar
(`veredito DESCONHECIDO, nao um PASS`).
Prova sem gastar boot: 4 stubs no lugar do QEMU, rodando o launcher real → **4/4
ramos de saída** (exit 0→0, exit 1→1, log vazio→3, travado→2). Mutação com o
launcher pré-patch: **1/4**.

**Parser** (`tools/f15_parse.ps1`): o boot 1 dava FALSIFIED por 2 linhas de FORJA
que **não eram do lab** — eram de outra skill
(`hw_pnp_pci_bridge_observe_only_agent_platformage`, tráfego normal dos 147
agentes). O parser contava palavra-chave no log inteiro; contagem global num boot
que roda 147 agentes é falso positivo garantido. Fix: escopo por nome
(`Get-LabLines`/`Get-LabMatch`). Duas correções de veredito que já estavam erradas:
`durable=0` lido como "Tickv ausente" (agora usa `durable_unknown`) e `escalate=`
casando com os contadores `escalate=0` do k-ai (agora `escalate=(gen|reuse)`).

**Vereditos** (com exit code):

| alvo | exit | veredito |
|---|---|---|
| `logs/f15_boot1.txt -Boot 1` | 0 | **PASS** |
| `logs/f15_boot2.txt -Boot 2` | 1 | **FALSIFIED** (`reason=not_found`) |
| `-Compare boot1,boot2` | 1 | **FALSIFIED** |

Evidência do boot 1 (verbatim):
`[T+56] [R3] [hermes] [SKILL_LAB] [ok] - act=gen name=oracle_rt_expr_v1 prov=model-born bytes=50 hash=0x458653425da3b4a5` e
`run name=oracle_rt_expr_v1 a=6 b=7 result=52`.

**Causa-raiz do FALSIFIED do boot 2, nomeada pelo próprio kernel:**
`[TICKV] [fail] - backend=RAM (VOLATIL) ... nao persiste entre boots`. O Tickv
montou em backend RAM; `disk_qemu.raw` e `disk_qemu.pristine.raw` com mtime
22:47:53, **anteriores** aos dois boots (23:36:40 / 23:41:57) — 0 bytes escritos.
**F1.5 não falhou por bug do lab: falta de backend persistente no QEMU.** É o
blocker de fundo, ainda aberto.

Suíte do parser: `tools/run_f15_fixtures.py` → **19/19** (15 fixtures de boot + 4
de compare, com exit code E token esperados). Mutação com o parser pré-patch:
**16/19**, falhando exatamente as 3 casas que codificam a regra nova.

---

## 2. Probe Miri — o que roda (e o que não roda) neste host

Pergunta: a verificação do eixo E funciona neste host? Resposta medida: **Miri sim,
Kani (a ferramenta real do E1) não**.

- `rustup component add miri` → OK. `miri-x86_64-pc-windows-msvc` instalado; minha
  crença de que Miri é Linux/macOS-only estava desatualizada.
- `cargo miri setup` → sysroot em 27 s.
- `cargo miri test -p k_ai --lib trust::tests::revoke_is_transitive_and_bumps_generation -- --test-threads=1`
  → **1 passed, 0 failed, 72 filtered out, 0.79 s, EXIT=0**. Roda porque
  `.cargo/config.toml` não define target padrão e k_ai é `#![cfg_attr(not(test), no_std)]`.
- **Prova de que o verificador tem dentes:** a primeira injeção de UB (`arr[7]`) foi
  pega pelo **const-eval do rustc**, não pelo Miri — ou seja, não provava nada.
  Refiz com índice opaco (`get_unchecked(black_box(7))`) e o Miri pegou em runtime:
  `Undefined Behavior: 'assume' called with 'false'` + backtrace em `trust.rs:633`,
  EXIT=1. Arquivo restaurado byte a byte (sha256 `84091d97…`), zero resíduo.
- **E1 = verificação formal = Kani** (`#[cfg(kani)]` + 5 `#[kani::proof]` em
  `k_ai/src/trust.rs:530`). `cargo kani` não está instalado e o guia oficial lista
  só três plataformas (`x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`,
  `aarch64-apple-darwin`) — "other platforms are either not yet supported or require
  build from source". WSL **não** está instalado neste host. Miri (detecção de UB em
  execução concreta) não substitui prova simbólica; não foram trocados um pelo outro.

---

## 3. Lock de escrita compartilhado do forum

O lock existia, mas vivia em `target/forum_post.lock` — caminho **relativo ao cwd**,
dentro da árvore do projeto: exclui quase ninguém (só quem roda com o mesmo cwd e a
mesma cópia). E o outro escritor, o loop OPMUSE em PowerShell
(`tools/forum_loop_opkimi.ps1`), gravava com `Add-Content` **sem lock nenhum**.

**Entregue:**
1. `tools/forum_lock.py` — lock ao lado do log, caminho absoluto (quem abre o log e
   quem trava o lock passam a ser o mesmo recurso, qualquer cwd/cópia/linguagem).
   Exclusão por `O_CREAT|O_EXCL` (atômico, sem janela de check-then-act),
   **fail-closed** (esperar e esperar; nunca escrever sem lock), roubo de órfão,
   soltura por token (só solta o lock que pegou).
2. `tools/forum_lock.ps1` — mesmo arquivo, mesmo formato JSON de holder, para as duas
   implementações se entenderem.
3. `tools/forum_post.py` — usa o lock compartilhado + **checagem de unicidade de id
   dentro da seção trancada** (o lock serializa quem o respeita; a checagem pega
   quem escreve sem lock).
4. `tools/forum_loop_opkimi.ps1` — escreve sob o lock, com guarda de newline
   (`Add-Content` colava na última linha e rasgava duas mensagens, R2/R2b) e
   **semando `$tick` a partir do log** (o id vinha de contador em memória: com o
   stateDir apagado, `OPMUSE-POLL-1` nasce duplicado de um id já gravado).
5. `tools/forum_repair_ids.py` — reescrevia o log inteiro com mode `"w"` **sem
   lock**; a guarda "aborta se outro agente appendou" era TOCTOU (lia na 17,
   escrevia na 55). Agora ler→validar→reescrever é uma transição só.

**Prova** (`tools/test_forum_lock.py`): 10 escritores Python (prefixo **idêntico** de
propósito — é a corrida que duplica id) + 4 escritores PowerShell disputando o mesmo
lock → **42 linhas válidas, 0 id duplicado, 0 linha rasgada, 14 seções críticas com 0
sobreposições, EXIT=0**; escritor rogue sem lock não tem id reusado; lock órfão
roubado em 0,13 s. Mutação (lock desativado): **9 ids duplicados e 6 sobreposições,
EXIT=1**. 3 execuções seguidas sem flakiness.

**Três bugs reais que o teste pegou** (nenhum era hipótese):
- `tasklist /FI "PID eq N"` devolve **returncode 0 para PID inexistente** (medido) →
  o probe antigo (`returncode == 0`) dizia "vivo" para todo PID no Windows, e um
  lock órfão **nunca** era roubado. Bug herdado do `forum_post.py` original. Agora
  `OpenProcess` via ctypes, com fallback de tasklist em **CSV casando o CAMPO pid**
  (a saída do tasklist é localizada; casar texto não serve).
- `os.open(O_EXCL)` devolve **EACCES, não EEXIST**, quando o arquivo está
  delete-pending; o contender **morria** (rc=1) em vez de esperar a vez dele.
- **Roubo sem compare-and-delete**: dois contenders viam o mesmo órfão, o primeiro
  removia e recriava, e o segundo apagava o lock **vivo** do primeiro — exclusão
  perdida na troca.

---

## 4. Veredito

- **F1.5**: geração+execução da skill **PASS** em runtime (mesmo boot).
  Persistência entre boots **FALSIFIED**, causa = Tickv em backend RAM. A prova de
  que o caminho de geração é real é o `act=gen`/`run` do boot 1; a de durabilidade
  continua pendente e o ORACLE-0057 segue valendo (QEMU não prova power-loss).
- **Miri**: disponível e útil neste host; **Kani (E1) não é alcançável aqui**.
- **Forum**: exclusão mútua comprovada entre Python e PowerShell, com id e linha
  íntegros.
- **Zero arquivo Rust tocado** — o build Rust desta sessão é idêntico ao de HEAD.

## 5. Pendências

- 🔴 **Backend persistente no QEMU** (F1.5 real): `Tickv` monta em RAM, nenhum disco
  é escrito. Sem isso o eixo de persistência não fecha, por mais correto que o lab
  esteja.
- ⏳ **Kani/E1**: precisa de host Linux ou `wsl --install` (decisão do dono).
- ⏳ **Fixtures/lock fora do `target/`**: `target/gen_f15_fixtures.py` e
  `target/test_loop_locked.ps1` são artefatos gitignored; só
  `tools/test_forum_lock.py` e `tools/run_f15_fixtures.py` estão versionados.
- 🟡 **Duplicados legados** no forum (FREEBU-0054/0056, OPKIMI-0007/8/9) não
  reparados; `forum_repair_ids.py` é one-shot e aborta nos dados atuais.