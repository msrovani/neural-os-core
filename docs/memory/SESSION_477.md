# SESSION_477 — USB BOOT.LOG em HW real: EP0 não correlacionava (tr_pa stale) + ring sem enqueue/cycle → fix A+B+C runtime-VERIFIED

**Data:** 2026-10-15 · **Estado:** v1.9.99-s477 TEST · **Branch:** `main`
**Fórum:** posts BUFFY-0011..0016 (arco completo: problema → experimento → diagnóstico → fix).
**Contexto:** o BOOT.LOG via USB é a única via de log no metal (serial é invisível no
notebook). O caminho MSC/BOT existia no kernel mas **nunca era exercitado**: o launcher
QEMU só anexava tablet/kbd ao xHCI. A validação USB em HW real estava rotulada
`AWAITING_HW` havia dezenas de sessões.

---

## 1. Discussão no fórum (BUFFY-0011 → 0016)

- **BUFFY-0011 (problema):** leitura do tree confirma que `persist_now` (boot_logger.rs),
  `usb_msc.rs` (BOT CBW/CSW), `verify_written_chain` e a política hub→MSC
  (`k_hal::usb::hub_msc`) existem — mas o QEMU launcher não anexava device MSC ao xHCI,
  então o caminho nunca rodava em dev. Pergunta ao fórum: onde está o bloqueador?
- **BUFFY-0014 (experimento):** harness `tools/run_boot_evidence.ps1 -UsbMsc` (device
  `usb-storage` + xHCI). Resultado PARCIAL: xHCI detectado (1b36:000d, porta 1
  `PORTSC=0x1203 CCS=1 PED=1 PLS=0 SP=4`) **mas `EP0 slot=1 timeout (sem Status)`** →
  `MSC bringup FAIL` → zero CBW/CSW → fallback silencioso
  `init_after_usb BOOT.LOG ok=true msc=false ata=true`. Gaps extras: P24a reivindicava
  teclado com o único device sendo usb-storage; MHI dizia `sda tier=UsbMsc` enquanto a
  Topology dizia `PATA 0GB` (telemetria mentirosa).
- **BUFFY-0015 (diagnóstico, via @oracle):** **dois bugs empilhados** no ciclo de vida do
  anel EP0; event ring, doorbell e encoding de TD estão **corretos** (hipóteses
  descartadas com file:line). A prova em runtime estava dentro do próprio log: entre o
  grant do token e o 1º timeout, a linha `sev desconhecida 'isoc' — total=2` — o sub
  `isoc` só existe em `route_isoc_event`, o Transfer Event do slot=1/ep=1 **chegou e foi
  descartado** exatamente no 1º GET_DESCRIPTOR.
- **BUFFY-0016 (fix):** runtime-VERIFIED — ver §3.

## 2. Causa-raiz — dois bugs no ciclo de vida do anel EP0

- **Bug A (primário, regressão s465 commit `e2d79925`):** `st.tr_pa` nunca é atualizado
  quando o anel EP0 é trocado. `address_device_loc` aloca anel novo e atualiza **só**
  `st.tr_va` (bringup.rs:1810-1816); `tr_pa` só era escrito em `mod.rs:1021` (anel do
  init). `ep0_ring()` devolvia o par dessincronizado (`tr_va`=novo, `tr_pa`=velho): os
  TRBs iam para o anel novo (o controlador lia certo) mas a espera correlacionava contra
  o PA **velho** (`bringup.rs:118` roteava o evento real para `route_isoc_event` e
  descartava; `bringup.rs:1028` `wait_transfer_event` com `status_pa` do anel velho). O
  controlador **completa** a transferência; quem perde o evento é a correlação.
- **Bug B (secundário, latente, mascara o Fix A sozinho):** anel EP0 reutilizado sem
  tracking de enqueue/cycle. Os 3 writers EP0 escreviam TD em offsets fixos 0/16/32 com
  `| 1` fixo (`ep0_control_in` 999-1018, `ep0_class_no_data` 907-916,
  `ep0_set_configuration` 2060-2071). Após o TD#1 o dequeue do controlador avança para o
  offset 48; o TD#2 reescrevia região já consumida com o mesmo cycle bit — o controlador
  espera TRB válido em 48 (memória zerada, cycle=0), nunca faz fetch, nunca gera evento.
  Evidência de classe: em **nenhum** log do repo existiam 2 transferências EP0
  sequenciais bem-sucedidas (zero `hub=OK`, zero `MSC desc cfg=`, zero `UAC OK`).

## 3. Fix (4 arquivos, +104/−44) e prova de runtime

- **Fix A:** `ep0_ring()` apagada; nova `ep0_reserve(n_trbs)` deriva
  `tr_pa = tr_va − pmoff` do invariante (fonte única) e o campo `tr_pa` foi **apagado**
  de `XhciState` (mod.rs:317 decl + mod.rs:1021 escrita). Campo derivável = campo
  duplicado.
- **Fix B:** cursor `ep0_enqueue: u16` + `ep0_cycle: bool` no `XhciState` (zerados na
  criação e **re-zerados** em `address_device_loc` quando o anel é trocado); os 3 writers
  postam no offset `enqueue*16` com o cycle corrente e avançam; `status_pa` do wait vira
  `tr_pa + (idx_post + n_trb_td − 1)*16` (era +16/+32 fixo, só válido para o TD no
  offset 0). Overflow >256 TRBs = recusa honesta com warn (`ponytail:` sem Link TRB o
  anel não cicla; teto comentado).
- **Fix C (honestidade P24a):** `unwrap_or(0)` e o ramo best-effort **deletados** —
  config descriptor ausente/falha de transporte = skip com log
  `sem HID boot: config desc ausente ou EP0 falhou`; exige `proto == want`.
- **Opcional:** `"isoc" => Sev::Warn` em `Sev::from_sub` (`slog.rs`) — eventos roteados
  ficam visíveis em vez de "sev desconhecida".
- **Gates:** `cargo check --release` 0 erros; `k-nano` serializado **385 pass / 1 fail
  pré-existente** (`allocator_routing_tests`, provado com stash — lição SESSION_418);
  escopo dos fixes verde: xhci 14/14, slog 6/6, usb 2/2. Imagem rebuildada
  (`cargo build --release -p boot`, `uefi.img` fresco).
- **Boot de validação (WHPX de 1ª, 420s, usb-storage no xHCI):**
  `logs/boot_usb_msc_fix_20261015_115605.txt` (120.505 B, sha256
  `A8EBC9837B218C9601C57ED54ADD1C1EBCC6F41B89D831F6122AC5CB4ECA85CE`).
  - **zero** `EP0 slot=N timeout (sem Status)` (antes: 7+) e **zero**
    `sev desconhecida 'isoc'`.
  - MSC enumerado: `MSC desc cfg=1 ep_in=0x81 ep_out=0x2 mps=1024 route=0x0` +
    `bringup OK slot=1 port=1 speed=4 mps=1024 try=0` + `MSC installed via k_hal::usb`
    + `storage plan n=2 [0]=usb-msc [1]=ata-pio`.
  - **BOOT.LOG no disco USB:** `init_after_usb BOOT.LOG ok=true msc=true ata=true` +
    `bootlog: OK 20008 bytes em BOOT.LOG (LBA 2048)` + `storage ok bus=usb-msc`.
  - P24a honesto: `P24a: nenhum HID boot em root ports (skip MSC=1 kb=0 mouse=0)`.
  - **Prova no artefato (readback externo do `disk_qemu.raw` pós-boot):** dirent na raiz
    FAT32 `BOOT.LOG size=262137 cluster=4693477` e o conteúdo contém as linhas **deste**
    boot (`session=BOOT.LOG channel=dev tick=225`, `deferred HID bringup start (msc=1)`,
    `P24a: nenhum HID boot em root ports (skip MSC=1 kb=0 mouse=0)`).
  - Nota honesta: 0 linhas CBW/CSW no serial = **gap de log** (a camada não loga por
    transferência), não de caminho — o write só alcançou o disco via BOT (READ/WRITE(10)
    sobre bulk); `verify_written_chain` roda dentro de `overwrite_boot_log` e é silencioso
    no sucesso por design; o readback externo do artefato é a verificação de meio.
    `panic=1` no log = registro cross-boot do boot **anterior** (SELF leu o storm do
    experimento anterior do ramlog), não é panic deste boot.
- **Classificação:** fix = **runtime-VERIFIED** (QEMU WHPX + artefato); metal = UNKNOWN
  (o contrato xHCI é idêntico, então o metal vira confirmação — mas QEMU PASS ≠ METAL PASS).

## 4. Lições

- **Anel trocado ≠ anel correlacionado:** trocar o produtor (`tr_va`) sem trocar a chave
  de correlação (`tr_pa`) faz o evento real chegar e ser descartado. Sinal no log: `sev
  desconhecida` para um sub que existe só no roteador de descarte.
- **Transfer ring reutilizado exige cursor de enqueue + cycle bit:** offset fixo +
  cycle=1 fixo só vale para o 1º TD. Guardar `enqueue/cycle` no state e re-zerar na troca
  de anel.
- **Device emulável no QEMU = bug dev-testável:** o rótulo `AWAITING_HW` do USB-MSC morreu
  ao anexar `usb-storage` — antes de aceitar AWAITING_HW, perguntar se o QEMU emula o
  device.
- **Claim sem transporte funcional = falso-positivo:** P24a reivindicava teclado com 5
  timeouts EP0 e o único device sendo usb-storage (gates duros só no command ring, passos
  EP0 com `let _ =`). Falha de transporte não reivindica porta.

## 5. Fórum (consenso / pendências)

- Arco fechado por BUFFY-0016. Commit pedido explicitamente pelo mantenedor nesta sessão.
- Pendências herdadas (inalteradas): mesh re-run com kernel atual (CURAIX-0060); F1 1h
  8c/8GB (UNKNOWN); #PF storm pós-PostRuntime (watchpoint, classe #630/F1); teste em metal
  (foto PRE/POST 00:14.0, TODO s467) — agora **confirmação**, não primeira descoberta.
