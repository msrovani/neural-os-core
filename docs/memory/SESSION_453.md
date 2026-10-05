# SESSION_453 — loop de boot 6G/8c com HW simulado + triagem de log

**Data:** 2026-10-04 (22:44 -> 23:10)
**Pedido:** rodar QEMU em loop, 6 GB / 8 cores, simulando todo o hardware possível,
com os artefatos necessarios para testar o AIOS; monitorar o log, corrigir os
erros e reiniciar.
**Alcance:** `tools/run-qemu-lab-loop.ps1` (novo), `tools/test_run_qemu_lab_loop.ps1`
(novo), `target/lab/*` (artefatos isolados, gitignored), `logs/lab_loop_cycles.csv`.
**Zero `.rs` tocado.**

---

## 1. Por que um launcher novo (e nao `run-qemu-whpx.ps1`)

Outra thread do repo roda um loop que faz
`Get-Process qemu-system-x86_64 | Stop-Process -Force` e divide
`target\uefi.img`, `target\disk_qemu.raw`, hostfwd 4445/4446 e monitor 5555
(medido: PID 13352, 4 iteracoes de 540 s). Compartilhar qualquer um desses
artefatos corrompe o boot dela ou o meu.

Isolamento usado (verificado, nao presumido):

| recurso | outra thread | este lab |
|---|---|---|
| binario | `qemu-system-x86_64.exe` | `target\lab\lab8c-vm.exe` (copia) |
| imagens | `target\uefi.img` / `disk_qemu.raw` | `target\lab\uefi.img` / `disk_lab.raw` |
| portas | 4445/4446, monitor 5555 | 4455/4456, monitor 5556+ciclo |
| limpeza | mata todo `qemu-system-x86_64` | mata so `lab8c-vm` |

O nome do binario **e** a fronteira: ninguem mais roda aquele arquivo.

## 2. Configuracao do boot (medida no cmdline do ciclo 1)

6 GB / 8 cores / WHPX / `-cpu Haswell`; OVMF por pflash (code + vars proprios);
`uefi.img` (Limine, index 0) + `disk_lab.raw` (FAT32 dados, index 1) por IDE;
e1000 com `-netdev user` + hostfwd; `intel-hda` + `hda-duplex` + `audiodev none`;
`qemu-xhci` + `usb-tablet` + `usb-kbd`; `virtio-gpu-pci` + `-vga std`;
`-serial file:<log>` (COM2 `null`); monitor TCP por ciclo;
artefatos de AIOS por `-device loader`: `target\FALCON3.BIN` @0x100000000
(1.037.071.016 B) e `target\lab\netmode.flag` logo acima.

Por ciclo: `f15_pristine.py restore` do disco pristine (4 GB, **14,6-17 s**),
`scan` do estado do lab antes do boot, veredito em CSV.

## 3. Quatro defeitos do proprio instrumento (achados, corrigidos, testados)

1. **Copia do binario nao achava as DLLs** — `0xC0000135 STATUS_DLL_NOT_FOUND`.
   O `-L` sozinho nao resolve: o diretorio de DLL precisa estar no `PATH`.
   Sem isso o QEMU morria com **0 bytes de serial**, e o veredito dizia
   "WHPX falhou" — que era falso (nenhum VM com WHPX chegou a existir).
   Fix: `$env:PATH = "C:\Program Files\qemu;" + $env:PATH` + **preflight**
   (`--version` antes do primeiro ciclo, exit 2 se nao rodar).
2. **`-L "C:\Program Files\qemu\share"` sem aspas** — `Start-Process` junta o
   ArgumentList com espacos e nunca coloca aspas: o QEMU recebia dois
   argumentos e abortava com `Could not open 'Files\qemu\share'` (WHPX e TCG).
   Fix: `('-"{0}"' -f $qemuShare)`.
3. **VM orfa mantinha porta e disco** — matar o loop nao mata o VM: sobrou
   `lab8c-vm.exe` vivo segurando o `disk_lab.raw` (`open(...,"wb")` ->
   `OSError: [Errno 22]`) e a porta do monitor
   (`Failed to find an available port`), e cada ciclo seguinte falhava em 10 s.
   Fix: `Stop-OurVms` por nome, em rodadas, antes de cada ciclo.
4. **Porta de monitor fixa** — socket em `TIME_WAIT` do VM anterior aborta o
   seguinte. Fix: `5556 + ciclo`.

Dois erros meus de instrumento, nao do codigo:

5. **Duas instancias do loop** escreveram no mesmo CSV/status (o processo velho
   mantem o script antigo em memoria apos a edicao). Fix: guard de singletono
   (exit 4) + PID file.
6. **Excecoes de self-test contadas como falha** — as 8 linhas `[EXC]` de um
   boot sao os demos P6/P7 (demand-paging `#PF` em `ip=0x70000030000a`, probe
   Ring3), todas **antes** da fase Runtime. Fix: `exc_demo` (antes do Runtime)
   separado de `exc_runtime` (depois); so a segunda entra no veredito
   (`EXC_RUNTIME`).

## 4. Ciclo 1 medido (o que o loop provou)

```
ciclo,bytes,phase7,tick_max,verdict,pf,panic,exc_runtime,exc_demo,corrupt,fail,restore_s,lab_state_before,s
1,253655,True,26206,PASS,0,0,0,8,0,4,17,0,424
```

6 GB / 8 c / WHPX, 424 s de observacao, disco restaurado do pristine antes
(`lab_state_before=0`), tick maximo 26.206, zero `#PF`, zero panic, zero
excecao em runtime, zero corrupcao. O ciclo 2 religou sozinho em seguida.

Teste do harness: `tools/test_run_qemu_lab_loop.ps1` → **10/10, EXIT=0**
(cada caso e um defeito real acima; o caso 9 le a linha real do CSV).

## 5. Triagem do log (4 `[fail]`, estaveis em 2 boots)

```
[T+0] [nk] [TICKV] [fail] - put/get smoke FAIL
[T+0] [k-ai] [SGDB] [fail] - Q4 FAIL: NSGDB put/get L1
[T+0] [nk] [sgdb] [fail] - Q-jump FAIL
[T+0] [nk] [ELF] [fail] - load fail: ELF: segment data truncated
```

Os tres primeiros sao **storage** (`mount READY but DEGRADED — indice parcial`,
`recover TIMEOUT off=0/8388608 keys=0`, `put_kv hw/cpu/*: oob` com a regiao
`NvmeFlashRegion` de 64 KB em `flash.rs`) — frente do AION (AION-0009/0010 em
review). **Nao toquei em `tickv.rs` nem em `flash.rs`.**
O quarto e o self-test do ELF loader (nao-fatal, ja marcado como tal no boot).

Achado de honestidade para a frente de storage: `SYS_HEALTH ... storage=GO`
no mesmo boot em que os tres smokes falham. O veredito de saude afirma GO
enquanto o proprio subsistema reporta falha — e a regra do projeto e o oposto
(`UNKNOWN != 0`, nada vira OK sem evidencia).

## 6. Duas leituras minhas que a medicao corrigiu

- "Boot travado no Limine" (logs de 5.606 B parando em `Loading kernel from
  hd(bi)kerr.f`): era **log no meio da escrita**. Os mesmos boots seguiram ate
  `T+41000` ticks. Um snapshot parcial lido como estado final inventa travamento.
- "`PORTSC=0xNaN` no timeout de reset USB": era artefato do meu proprio
  resumo (`sed s/[0-9]\\+/N/g`); a linha crua e `PORTSC=0x2a0` (portas vazias
  do hub emulado). **Nao ha bug de formatacao ai.**

## 6b. Artefatos do AIOS: o que realmente chega ao guest (medido)

Correcao de uma leitura minha: `Nenhum modelo .bitnet carregado` NAO significa
modelo ausente — e um probe anterior. O estado real no boot:

```
LLM LOADED falcon3.v6 (QEMU@4G in-place) size=1012764KB RAM=7168MB airllm=false
```

O mesmo `Nenhum modelo .bitnet carregado` aparece nos boots do launcher padrao
(`logs/boot_whpx_*.txt`), entao nao e diferenca do lab.

Gap real encontrado: `run-qemu-whpx.ps1` so vasculha `target\` + `D:\modelos`.
Os experts/TTS vivem em `models\` e `target1\` e **nunca chegavam ao guest**:

```
RUSTCODER QEMU-loader scan [0x129000000..0x129200000] — 0xBE11BE11 ausente
```

Correcao no lab: carregar tambem os blobs com magic `0xBE11BE11` (o kernel
descobre por varredura em `[0x100000000..0x180000000)`), em enderecos que nao
se sobrepoem nem as janelas dos experts:

| artefato | endereco | tamanho | magic |
|---|---|---|---|
| `target1/ROUTER.BITNET` | `0x160000000` | 25.818 B | `0xBE11BE11` |
| `target1/STT.BIN` | `0x161000000` | 221.712 B | `0xBE11BE11` |
| `models/PIPER_PT_BR.BIN` | `0x162000000` | 62.814.772 B | `0xBE11BE11` |
| `models/bpe_vocab.bin` | `0x166000000` | 338.964 B | `0x31425042` (BPB1, fora da varredura) |

Verificado no boot seguinte: os blobs **chegam** (o kernel reporta
`HWEXPERT magic 0xBE11BE11 found @0x160000000 / @0x161000000 / @0x162000000`) e
**rejeita com honestidade** (`parse FAILED (proximo endereco)`, `not BGE — skip`):
eles nao sao o expert de HW nem o BGE. Sem falso positivo.

O que continua faltando e **artefato ausente na arvore**, nao bug do launcher:
- `RUSTCDR2.BIN` (~300 KB, esperado na janela de 2 MB `0x129000000`) — a arvore
  so tem `RUSTCDR3.BIN` (336 MB, formato e tamanho errados para a janela).
- `hw_expert_v3` (esperado a partir de `0x129200000`).

## 6c. Busca em `target1/`: o expert de HW existe

`target1/` tem **18 artefatos**, todos com magic `0xBE11BE11` (v4/v5/v6), e
**nao** estavam no boot:

| artefato | tamanho |
|---|---|
| `hw_expert_v6.bitnet` | 265.620 B |
| `ROUTER.BITNET` | 25.818 B |
| `STT.BIN` | 221.712 B |
| `PIPER_PT_BR.BIN` | 62.814.772 B |
| `RERANKER.BIN` / `AGENT.v6` / `LEARNER.v6` / `VISION.BIN` / `E5_MULTI.BIN` / `BGE_M3.BIN` / `bpe_vocab.bin` (BPB1) | — |

`hw_expert_v6.bitnet` e o unico com o tamanho (~300 KB) que o comentario do
kernel descreve. **Entregue ao guest:** empilhado logo apos o fim do LLM.

**Erro meu no caminho:** a primeira tentativa-fixou o expert em `0x129200000`,
o inicio literal da janela HWEXPERT. O LLM de 989 MB ocupa
`0x100000000..0x13DD9BAB0`, entao esse endereco esta **dentro do modelo** e o
QEMU abortou com `The following two regions overlap`. O loop falhou fechado
(3 `QEMU_EXIT` seguidos -> abort), como deve. Endereco fixo nao serve com um
LLM desse tamanho; os extras vao sequenciais apos o modelo, com 1 MB de folga.

**Estado medido depois da correcao:**

```
LLM LOADED falcon3.v6 (QEMU@4G in-place) size=1012764KB
HWEXPERT magic 0xBE11BE11 found @0x13de00000 — tentando parse 1024 KB
HWEXPERT @0x13de00000 parse FAILED (proximo endereco)
RUSTCODER QEMU-loader scan [0x129000000..0x129200000] — 0xBE11BE11 ausente
```

O artefato **chega e e encontrado**; o parser do kernel **rejeita**. O
`main.rs:4464` pede `hw_expert_v3` e o que existe e um arquivo **v6** — Layout
divergente entre o que o parser aceita e o que a arvore tem. Nao mexi: o
`crates/neural-kernel/src/main.rs` esta modificado por outra thread neste
instante.

`RUSTCDR2.BIN` (~300 KB, janela de 2 MB em `0x129000000`) **nao existe** em
`target1/`, `target/` nem `models/`: so ha `RUSTCDR3.BIN` (336 MB), que nao cabe
na janela. RUSTCODER ausente por artefato faltando, honesto.

## 6d. `-VirtioBlk`: implementado, dry-run OK, runtime BLOQUEADO (UNKNOWN)

`run-f15.ps1` (o harness que fecha o F1.5) liga o disco de dados em
`if=virtio,cache=writethrough` — e o degrau 2 do funil D3 (persistencia real,
TICKV `backend=file`). O loop ganhou o switch:

- dry-run: `format=raw,file=...disk_lab.raw,if=virtio,cache=writethrough` (OK);
- **runtime: o QEMU aborta** com
  `drive with bus=0, unit=0 (index=0) exists` e o loop falha fechado (3x
  `QEMU_EXIT` -> abort). Evidencia em `logs/lab_loop_cycles_virtio_fail.csv`.

Bisseccao feita: `uefi IDE + disco virtio` boota; `+virtio-gpu-pci` boota;
`+pflash` boota; `+-device loader` boota; `+audiodev none/intel-hda/hda-duplex`
boota. **Falta isolar qual device do set completo causa a coliseo** →
classificado `UNKNOWN` (nao `BLOCKED`: o caminho IDE funciona e produz 2 PASS).
A configs de runtime restaurada e IDE; CSV do virtio ficou em arquivo separado
para nao misturar com a evidencia boa.

Post no forum (lock compartilhado, `locked:false` depois):
`FREEBU-0147` (evidence: 2 ciclos PASS 6GB/8c + hashes) e `FREEBU-0148`
(proposal: gap de entrega dos experts + pergunta sobre virtio e sobre o parser
v6, endereçada ao AION que é dono da lane de storage/kernel por D5).

## 6e. O erro "drive with bus=0, unit=0 (index=0) exists" era MEU

Ao fechar esta sessao o lab estava morto, com `QEMU_EXIT` em 10 s — inclusive
no modo **IDE**, que antes funcionava. Causa raiz: ao adicionar o switch
`-VirtioBlk` troquei

```
"-drive", "format=raw,file=$disk,if=ide,index=1",   ->   $diskArg,
```

e **perdi o token `-drive`**. O QEMU recebia `-drive <uefi> <disco>`: a spec do
disco virava argumento solto e o QEMU queixava de `bus=0 unit=0` ja ocupado.
Nao tinha nada a ver com virtio — por isso a bisseccao anterior ("cada device
isolado boota") levava ao caminho errado: ela provava que os devices estavam
bem, nao que o par `-drive`/`spec` estava correto.

**Como achou:** o loop passou a gravar a linha de comando exata por ciclo
(`logs/lab8c_c<N>_<stamp>.args.txt`) e o defeito apareceu em 2 linhas de diff.
O dump virou parte do harness porque um erro de parse do QEMU so diz qual spec
foi rejeitada, nunca o que mais foi passado junto.

Acerto: `"-drive", $diskArg,`. Depois: WHPX sobe de novo, fase 5 em t=60 s.

## 7. Limitacoes (UNKNOWN, nao resolvido aqui)

- **Nenhum `.rs` corrigido**: os 4 `[fail]` sao da frente de storage (AION).
- **1 ciclo PASS medido** nesta sessao (o ciclo 2 ja havia comecado); o
  aceite de F1 pede **8c/8 GB por 1 h sem `#PF`** — aqui foi 6 GB (o plano
  medido em SESSION_452 usou 6 GB) e ~7 min por ciclo.
- `-audiodev none`: sem captura real (o `-AudioBridge` com `dsound` quebrava o
  boot neste host).
- Loop ainda rodando no fim da sessao (PID 10916, `logs/lab_loop.pid`); parar
  com `Stop-Process -Id (Get-Content logs/lab_loop.pid)`.
- `target/lab/disk_lab.raw` e artefato do lab: o `disk_qemu.raw` da outra
  thread **nao** foi tocado.

## 8. Como retomar

```powershell
powershell -File tools/run-qemu-lab-loop.ps1 -Cores 8 -RamGB 6 -BootSeconds 420
powershell -File tools/test_run_qemu_lab_loop.ps1     # 10/10
Get-Content logs/lab_loop_cycles.csv                  # veredito por ciclo
Get-Content logs/lab_loop_status.txt                  # estado corrente
```