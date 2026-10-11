# SESSION_466 — Mesh 2×4G/4c reprise: raiz do "1 core" = kernel STALE em `target1/` + prova pós-s464 (`smp_online=4`)

**Data:** 2026-10-11 · **Sprint:** s466 · **Bloco:** Lab/Mesh
**Status:** ✅ GOAL batido (M1–M4 PASS nos 2 nós) · **Host:** WHPX Haswell 4c/4G ×2, `-Force` overcommit aceito (free 3.3G vs needed 9G)

---

## 1. Contexto

Reprise do lab mesh 2×4G/4c de 10-10 (SESSION_455/s464): as duas instâncias subiam
**`smp_online=1`** e **sem LLM**. O smoke isolado (`run-qemu-whpx.ps1` → `target\uefi.img`)
provou `smp_online=4` no mesmo host/OVMF — ou seja, o gate EXTD do s464 **não** era a causa
faltante; algo no launcher do mesh levava o kernel errado/errôneo.

## 2. Discriminador H1/H2 (kernel dentro da imagem, não da árvore)

Extração FAT32 de `kernel.elf` de cada imagem (`tools/verify_usb_esp.find_esp` — as imagens
são MBR + GPT protective, partição em LBA 2048; 7z ausente no box):

| Imagem | sha256 kernel.elf | Tamanho | Match árvore (`target\limine-esp-tree\kernel.elf` = `8b0bb7b896363ea1`) |
|---|---|---|---|
| `target\uefi.img` | `8b0bb7b896363ea1` | 27 445 168 B | ✅ |
| `target1\uefi.img` | `0617d4c84026b874` | 27 436 432 B | ❌ (pré-s464) |

- **H1 CONFIRMADO:** o mesh bootava o kernel **antigo pré-s464** (bug bit-11 do gate EXTD) →
  fail-closed incondicional (`APIC_R_FW_EXTD_HV_NO_X2`) → `LAPIC intocado` → `smp_online=1`.
- **H2 (OVMF) REFUTADO:** binário edk2 code-only byte-idêntico (3 653 632 B); o smoke passou
  sob o mesmo binário.

Causa do 1 core: `run-qemu-p2p-mesh.ps1` preferia `target1\uefi.img` e só caía em `target\`
se `target1` não existisse — `target1/uefi.img` ficou **stale** (SESSION_306 já documentava a
armadilha; a ordem de preferência era o bug).

## 3. Análise "sem modelo" (2 causas; a 2ª corrigida)

- **(a) minha:** `-NoDisk` do launcher desliga os loaders (linha 83: NoDisk → `useModels=false`).
  Corrigido com a combinação `-WithModels -NoDisk` (WithModels vence o acoplamento; disco
  secundário é irrelevante para os loaders, que são RAM-mapped).
- **(b) CORREÇÃO de análise prévia:** a 4G **o LLM cabe**. Em i440fx, `-m 4G` derrama o excesso
  acima do hole de 4 GiB: high RAM = `0x100000000..0x140000000` (`ram_mb=5120` = top-of-RAM,
  explicado). FALCON3.BIN (989 MiB, termina `0x13DD9BAB0`) **cabe**; só PIPER (~`0x13E100000`+),
  bpe e hw_expert estouram `0x140000000` e o kernel **honest-skip** (`is_page_present` guard,
  SESSION_262). A política antiga "modelos exigem ≥5 GB de guest" está **retirada** — modelos
  podem ficar ON a 4G; extras além da RAM são pulados honestamente.

## 4. Fixes aplicados (launcher)

1. **`imgDir` flip** (raiz do 1 core): `target\` **primeiro** (saída fresca do cargo build),
   `target1\` só como fallback, com comentário s466 citando os dois shas.
2. **`-Force` plumbing** no `Assert-QemuMemFree` (overcommit WARN ruidoso, exit via -Force).
3. **Landmine sync:** `target1\uefi.img` ← `target\uefi.img` (protege `run-mesh3/4/6-lab.ps1`
   que ainda apontam/fallbackam para `target1`).

Parse `[Parser]::ParseFile` OK (PS 5.1 cp1252 — só ASCII no .ps1).

## 5. Validação (run 00:33, monitor hook-driven 25 min — anti-loop SESSION_463)

`run-qemu-p2p-mesh.ps1 -Accel whpx -Cores 4 -Mem 4 -NoDisk -WithModels -Force`
Launchers A=3276/B=23336; args dumps com `imgDir=target` + 5 loaders; `Models=True` ambos.

| Milestone | A (Cloverleaf) | B (Hal9000) | Evidência |
|---|---|---|---|
| **M1 SMP** | PASS | PASS | `smp_online=4` (A `[T+9]`, B `[T+16]`); `LAPIC inicializado` `[T+0]`; `aps=3`; **zero** `fw_extd_hv_no_x2` |
| **M2 Runtime** | PASS | PASS | `[SCHED] tick` avançando; agents=54 polled |
| **M3 LLM** | PASS | PASS | `LLM LOADED ... (QEMU@4G in-place) size=1012764KB` — size == FALCON3.BIN (1 037 071 016 B); o **label** `llama8b.bin` é o nome display hardcoded do slot QEMU@4G, não o arquivo |
| **M4 Mesh** | PASS | PASS | A `role=Master` + `MESH_HEALTH peers=1`; B `role=Worker` + `peers=1` |

**Veredito:** `logs\mesh4c_v2_verdict.txt`. **Fim:** ambos os QEMU mortos por fora às 00:57
(~24 min) — **GUEST_DEAD externo**, não crash: últimos dumps saudáveis (`aps=3`, `pf=0`,
sem panic/#PF/triple/OOM em nenhum log). Provável kill externo (outra frente/operador);
launcher imprimiu `[FIM]` sem timeout interno.

## 6. Anomalias observadas (não-blocking desta run)

- **SILENCE log stall:** `[SILENCE] log parado ha 60s` ~1 min após Runtime em ambos; o canal
  serial congela enquanto o kernel vive (watchdog dumps a cada ~10s até dump#171/163,
  tick ~121k). Classe conhecida (SESSION_411/438/419 — serial lossy sob contenção SMP);
  scheduler tick reportado baixo (32/64) porque o próprio slog depende do canal congelado.
  Residual — não regressão desta run.

## 7. Lições

1. **Launcher que prefere `target1/` boota kernel stale para sempre** — a saída fresca do
   cargo vive em `target\`; `target1/` é staging de modelos, não de imagem. Ordem de
   preferência = contrato de freshness; preferir sempre o artefato do build atual.
   (Reforço da SESSION_306 com o mecanismo exato: stale ≠ "não sync depois do build" —
   é "nunca mais visto depois que `target1/` passou a existir".)
2. **Provar o artefato BOOTADO, não o da árvore:** `hash(kernel.elf)` de DENTRO da imagem
   (FAT walk) é o discriminador; `hash` do `target/limine-esp-tree` só prova o build.
   (SESSION_464 "hash(source)≠hash(image)=alerta" aplicado com sucesso — 1 run.)
3. **Geometria de RAM do QEMU i440fx:** modelos p/ loader ficam na janela high
   `0x100000000..+RAM_above_hole`; `-m 4G` → ~1 GiB high. FALCON3 (989 MiB) cabe; PIPER+
   estouram e são honest-skipped. Não confundir "loader beyond RAM" com "models off".
4. **Label do slot ≠ nome do arquivo:** o kernel reporta o nome display hardcoded
   (`llama8b.bin`) para o slot QEMU@4G in-place; validar carga pelo **size** e pela
   presença da linha, nunca pelo rótulo.

## 8. Artefatos

- `logs\p2p4c_v2_20261011_003307.out` (launcher) · `logs\mesh_{A,B}_20261011_*.args.txt`
- `logs\boot_mesh_a.txt` / `boot_mesh_b.txt` · `logs\mesh4c_v2_verdict.txt`
- Monitor: `C:\Users\msrov\AppData\Local\Temp\opencode\watch_mesh4c_v2.ps1`
- Discriminador: `C:\Users\msrov\AppData\Local\Temp\opencode\extract_kernel_sha.py`
- Fix: `run-qemu-p2p-mesh.ps1` (imgDir flip + `-Force`)
