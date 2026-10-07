# SESSION 460 — Power cascade (S5/CF9) + oráculo USB + instrumento PRE-HCRST — 2026-10-07

## 1. Pendrive (D:/E:) pós-reteste: ainda zerado
- `D:\BOOT.LOG` + `E:\BOOT.LOG` 256KB zeros; só stage-0 EFI. Sem USB, sem log.
  → USB continua morto; power hang virou 2ª frente.

## 2. Power: hang de shutdown/reboot MAPEADO e CORRIGIDO (@explorer + @fixer)
- Causa: `begin_orderly_shutdown` e `begin_orderly_reboot` faziam a MESMA coisa:
  `out 0x64,0xFE` cego + `loop{hlt}`. EC de notebook ignora 8042 sem poll de
  IBF → sem reset → halt mudo ("hang sem efeito"). S5 fora do path vivo
  (`power_off_cascade` dead_code); sem FADT RESET_REG, sem CF9, sem triple-fault.
- Fix (check 0 erros, provado no ELF): cascata S5 (só shutdown, se pm1a!=0,
  RMW + poll SCI_EN + back-to-back) → 0x604 só-QEMU → CF9 0x06→0x0E →
  8042 com poll IBF → triple-fault → park OBSERVÁVEL (nunca `hlt` mudo).
  `power_off_cascade` morta atualizada junto. Cada tentativa loga ok/timeout/skip.
- Arquivos: k_nano acpi.rs (+RMW/poll/back-to-back) · hal.rs (IBF poll, CF9,
  triple_fault, 0x604 gated) · neural-kernel shutdown.rs (cascata + park).

## 3. Oráculo USB (ora-1): "parem de resetar, comecem a preservar"
- Tese: stage-0 EFI prova que o UEFI tinha o stick treinado; nosso HCRST
  destruiu o único link que funcionava (pós: PLS=5 RxDetect, CCS=0).
  Reset em porta vazia nunca completa por design (SS sem link não gera WRC).
- Ranking: H1 65% HCRST destrutivo+alvo errado (ccs=3 = câmera/BT internos) ·
  H2 40% mux companion SS/USB2 (stick USB2 na gêmea; WPR em SS vazia = no-op) ·
  H3 25% hub (hub-first sem GET_DESCRIPTOR/route/TT = teatro) · H4 15% timing
  (esgotado) · H5 <5% anéis.
- Ordem: 1º companion USB2 (barato, alto valor-informação) · 2º soft-takeover
  skip-HCRST-se-RS=1 (aposta, ~1 sprint, 50-60%) · 3º retrain-longo NÃO ·
  4º full-kexec NÃO (use-after-free).
- Red flags nos nossos fixes: FIX1 queima budget resetando portas vazias (até
  16×TIMEOUT) e pode resetar os 2 HCs; FIX2 tem sucesso mentiroso
  (`return ped || reset_change`) e máscara warm às cegas.
- Exigência: dump PRÉ-HCRST dos 2 HCs com protocolo — sem ele, tiro no escuro.
  `PRE HC=.. RS=.. port=.. proto=.. PORTSC=.. CCS/PED/PLS/SP`: se algum PRE tem
  PED=1 e o pós tem 0 → destruímos (vai takeover); se nenhum → porta/HC/hub.

## 4. Instrumento PRE-HCRST (@fixer, check 0 erros, provado no ELF)
- `xhci/mod.rs::init_xhci_select`: antes do 1º w32 (halt/HCRST/Run), 1 linha por
  porta por HC (00:0d.0 e 00:14.0): `PRE HC=.. RS=.. port=.. proto=SS|USB2
  PORTSC=.. CCS=.. PED=.. PLS=.. SP=..` → ramlog (dump `--- USB ramlog ---` no
  FB, fotografável) + slog. Só leitura MMIO, zero mudança de comportamento.
- Bônus: `proto=` nas 3 linhas de reset FAIL.
- Imagem `usb_hw_1b3b.img` regenerada (ESP sha=`9c935337` match; DATA 1B+3B).

## Reteste pedido no HW (2 fotos)
1. Regravar (Rufus DD; `usb_hw.img` velho travado pelo Rufus — usar o `1b3b`).
2. Foto 1: linhas `PRE HC=...` no `--- USB ramlog ---` (decide takeover vs companion).
3. Foto 2: HUD após boot (linha `usb` + `bootlog` — novo label `usb_ok=`).
4. Testar Desligar e Reiniciar pelo Jarbas: devem agir de verdade agora
   (S5/CF9); se pararem em park com heartbeat, fotografar a última linha.
