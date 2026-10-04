# SESSION_448 — Braco de ablacao do F1.5: o disco de partida vira variavel declarada (IDEA #638)

**Data:** 2026-10-04
**Versao:** v1.9.99-s448 TEST
**Branch:** `main`
**Alcance:** ferramentas de host (`tools/`). **Zero arquivo Rust tocado.**
**Retomada:** [RESUME.md](RESUME.md)

---

## 1. Ficha (ADR-0113: PROBLEMA / EVIDENCIA / CAUSA / MENOR CORRECAO)

**PROBLEMA.** `run-f15.ps1` documentava "snapshot the clean disk once, **restore
before a run**" e o codigo so fazia o snapshot (`if (-not (Test-Path $Prist))`).
O disco de que cada boot partia era uma variavel nao declarada — e no boot 1 essa
variavel decide se `act=gen` prova alguma coisa.

**EVIDENCIA (medida nesta sessao, no disco real do lab):**

| arquivo | `lab_state` | detalhe |
|---|---|---|
| `target/disk_qemu.pristine.raw` (mtime 03/10 22:47) | **0 (limpo)** | sem vestigio da skill |
| `target/disk_qemu.raw` (medido 04/10 ~00:57) | **1 (sujo)** | dirents FAT32 `skill/wasm/oracle_rt_expr_v1` e `skill/wasm_prov/oracle_rt_expr_v1` em ~1657 MiB |

Ou seja: o confound **nao era teorico** — o disco do lab ja tinha sido sujo por
um boot anterior. E ~3 min depois o mesmo `disk_qemu.raw` voltou a `lab_state=0`
(outra thread restaurou/reconstruiu): **o estado do lab e volatil**, entao a
pergunta "de que disco o boot partiu" so pode ser respondida por um **registro
feito no boot**, nunca por uma afirmacao posterior.

**CAUSA.** O carimbo §14 provava a identidade da **imagem**; o disco — a outra
entrada do experimento — nunca foi carimbado.

**MENOR CORRECAO.** `tools/f15_pristine.py` (`ensure`/`restore`/`scan`/`check`) +
bloco `[7]` no launcher (restore **opt-in**, scan **sempre**) + quatro campos no
sidecar (`restore`, `restore_motivo`, `disk_lab_state_before`, `pristine_bytes`)
+ tres regras fail-closed no parser. Nenhuma mudanca em `tickv.rs`, no kernel ou
no boot.

**CUSTO MEDIDO.** Varrer 3 GB custa **1,6-4,9 s** (mmap + `find`, blocos de 64 MiB)
frente a um orcamento de boot de 900 s. No teste com discos de 4 MB: **0,03 s**.
Por que mmap e nao `read()`: 3 GB nao cabem na memoria e o fatiavel precisaria de
64 MB por vez.

## 2. As tres regras fail-closed (o que faz o braco *valer* como prova)

1. **Sidecar sem `restore=` ou sem `disk_lab_state_before=` → FALSIFIED.** Sidecar
   antigo nao prova de que disco o boot partiu.
2. **boot 1 com `disk_lab_state_before=1` → FALSIFIED** (`act=gen` nao prova
   geracao num disco que ja tinha a skill).
3. **boot 2 com `restore=1` → FALSIFIED** (o restore apaga o estado que o boot 1
   deveria ter persistido: experimento rigged).

## 3. Alternativas rejeitadas (e por que)

| alternativa | por que nao |
|---|---|
| restaurar sempre (implicitamente) no boot 2 | apaga exatamente o estado cuja persistencia se quer provar; o `-RestorePristine` e opt-in e a regra 3 barra o uso errado |
| SHA256 do disco inteiro a cada boot | ~10-20 s por boot e **nao responde a pergunta**: "o disco tem a skill?" e o que importa, nao "o disco mudou?" |
| confiar em mtime do disco | qualquer escrita muda o mtime (ruim de lab, NVRAM, etc.); nao distingue skill de ruido |
| filtrar no log em vez do disco | o contaminado e o disco; o log esta limpo por construcao |
| `Copy-Item` sem verificacao | restore parcial deixa o boot rodando sobre um disco que ninguem sabe qual e |

## 4. Risco aceito e por que o restore e opt-in

`target/disk_qemu.raw` e **estado compartilhado do lab**: outra thread estava
bootando enquanto esta sessao rodava (o mtime do disco mudou de 00:45 para 01:00
durante o trabalho). Sobrescrever 3 GB sem pedido destrói o boot de outra agente,
entao:
- `-RestorePristine` e switch explicito e so faz sentido no boot 1 (o launcher
  **recusa** no boot 2);
- o launcher loga `restore=1|0 disk_lab_state_before=0|1` antes de lancer o QEMU;
- `test_f15_stamp.ps1` roda com `$RestorePristine = $false` de proposito e
  **verifica** que o sidecar saiu com `restore=0` (o teste nao pode mexer no disco
  compartilhado).

## 5. Como falsificar / como validar

**Falsificadores (todos medidos):**
- Tirar o scan → `b1_lab_state1` passa a reprovar so por causa da regra, nao da
  medicao: o valor vem do disco, nao do fixture.
- Sem o `restore`, o teste `test_f15_stamp`/`test_f15_ablation` mostra o disco
  **seguindo sujo** depois do "restore" (asserção `disco intacto (dirty)`).
- `ensure` com fonte suja e sem `--force` → **exit 3** e nenhum arquivo criado
  (asserção `ensure nao criou snapshot recusado`). Um snapshot tirado de um disco
  sujo nao e "pristine" — e assim que o controle se mente sozinho.

**Validacao (tudo nesta sessao):**

| check | resultado |
|---|---|
| `python tools/run_f15_fixtures.py` | **26/26, EXIT=0** (22 anteriores + `b1_lab_state1`, `b1_sem_ablacao`, `b2_restore1`, `b1_sha_erro`) |
| `powershell -File tools/test_f15_ablation.ps1` | **13/13, EXIT=0** — executa o bloco `[7]` REAL sobre discos de 4 MB: restore copia, verifica, apaga a skill; sem restore o disco segue sujo; `ensure` recusa fonte suja; restore no boot 2 e recusado |
| `powershell -File tools/test_f15_stamp.ps1` | **EXIT=0** — executa `[7]`+`[14]` reais e confere 15 campos do sidecar contra os arquivos |
| `Parser::ParseFile` nos 3 `.ps1` | sintaxe OK nos 3 |

### 5b. Finding do mesmo dia: a imagem pode estar **travada** por outro QEMU

`Get-FileHash target/uefi.img` lancou `FileReadError: "usado por outro processo"`
enquanto outra thread rodava o lab com a imagem aberta. O launcher **morreria**
nesse ponto — e o carimbo §14b é precisamente a evidencia que nao pode depender
de sorte.

Correcao: o `try/catch` grava `uefi_sha=ERRO:lido-em-uso` e **nao derruba o run**
(o run ainda e legitimo; quem tira conclusao e o parser), e o parser reprova com
`a identidade NAO foi estabelecida no boot`. Fixture `b1_sha_erro` fixa a cadeia
inteira. O teste de stamp sai **2 (inconclusivo)** com a imagem travada, em vez
de fingir um veredito: ambiente ocupado nao e falha de codigo.

Camada correta: **degradar no carimbo, reprovar no veredito.**

## 6. Bug de precisao que os testes pegaram (segunda vez no mesmo arquivo)

Compara o epoch do sidecar contra o arquivo reprovava **todos os 6 casos
positivos**: `epoch sidecar=1791086757 agora=1791086758`. Duas causas somadas, e
as duas importam:
1. a string ISO de mtime perde 1 ULP entre Python e .NET (ja tratado na s447);
2. **`/` no PowerShell e divisao em double e o cast arredonda** — com mtime em
   757.8 s o PS dava 758 e o Python (`int()` trunca) dava 757. Pior: `Ticks` sao
   ~1,8e17, **acima de 2^53**, entao a divisao em double perde o bit antes do cast.

Correcao (identica no launcher e no parser, sem `Floor` magico):
```powershell
$ticks  = [int64]$it.LastWriteTimeUtc.Ticks - 621355968000000000
$epoch  = [int64](($ticks - ($ticks % 10000000)) / 10000000)
```
O resto antes de dividir mantem tudo em inteiro exato. **Truncamento, nao
arredondamento** — e o Python faz `int()`, que trunca.

## 7. O que ficou UNKNOWN

- **Nenhum boot de QEMU rodou com `-RestorePristine`** (opt-in; o disco do lab
  estava em uso por outra thread). O braco esta **IMPLEMENTADO e verificado em
  host**, nao **OBSERVED no metal** (HUMAN-0002/D2): falta um boot real do boot 1
  com `restore=1` e `disk_lab_state_before=0` no sidecar.
- O restore de **3 GB** nao foi exercitado (o teste usa 4 MB). O custo medido da
  varredura (1,6-4,9 s) sugere a copia na ordem de 10-20 s, mas isso e estimativa
  nao medida.
- `ensure` so cria o snapshot **se ele nao existir**; quem decide descartar um
  pristine velho e o maintainer (`-ForcePristine`).

## 8. Licoes

1. **Um controle que ninguem registra nao e controle.** O "pristine" existia ha
   s446; o que faltava era registrar se ele foi usado.
1b. **Degradar no carimbo, reprovar no veredito.** Uma evidencia que nao pode ser
   coletada (imagem travada) nao pode derrubar a coleta nem virar ausência: ela
   vira um valor que o veredito sabe reprovar.
2. **Estado compartilhado entre threads se prova por carimbo no boot**, nunca por
   inspecao posterior: o mesmo arquivo estava sujo e limpo com 3 min de
   diferenca durante esta sessao.
3. **Numero de serie entre duas linguagens e um contrato, nao um detalhe**:
   string ISO (1 ULP) e divisao em double (arredondamento + 2^53) falharam as duas
   — e so apareceram porque existe fixture que compara contra o arquivo real.
4. **Fail-closed tambem precisa cobrir o erro de quem vai tirar proveito**: o
   `ensure` recusa criar um "pristine" a partir de um disco sujo, porque o modo
   natural de se enganar aqui e um controle mal tirado que finge ser controle.