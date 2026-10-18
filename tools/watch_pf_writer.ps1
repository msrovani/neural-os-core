# watch_pf_writer.ps1 - s488: captura o WRITER do #PF storm em vez de perseguir o IP.
#
# CONTEXTO (por que este harness existe)
#   O #PF storm de fundo (pre-existente desde s479) tem IP constante DENTRO de um
#   boot mas diferente entre boots. A simbolizacao historica falhava porque ela
#   usava o ELF da ARVORE, nao o ELF que bootou (prova: o fingerprint de bytes
#   `[PF_DBG] ip_bytes` do log de 17/10 nao casa com byte nenhum no ELF de hoje
#   no mesmo IP -> ELF errado, nao "IP migrante").
#
#   Busca por padrao de bytes (89 14 88 eb 81 48 8b 7c 24 70) no ELF ATUAL
#   localiza a store `mov %edx,(%rax,%rcx,4)` dentro de:
#       cortex::parallel_matmul::ternary_worker
#   com %rax = ctx.c_ptr (offset 0x10 de TernaryJobCtx) e %r8 = ctx.n (0x28).
#   cr2 pequeno (0x5f40/0x5f38/0x1ec0/0x2f38/0xfc0) + err=0x2 (write) => c_ptr
#   == 0 no momento da store.
#
#   c_ptr e escrito UMA vez (no Box::new do ctx). Se ele esta 0 em runtime,
#   ALGUEM escreveu 0 depois. Este harness poe um hardware watchpoint nesse
#   campo e captura quem escreveu (PC + backtrace + registradores).
#
# COMO FUNCIONA
#   1. QEMU sobe com gdbstub (-s) e pausado (-S), TCG (watchpoint precisa de TCG).
#   2. `tools/pf_ctx_watch.py` (engine gdb-Python) poe breakpoint em
#      `AtomicPtr<TernaryJobCtx>::store` (o publish, rsi = ctx).
#   3. A cada publish (rsi != 0): imprime ctx/c_ptr/k/n e arma um watchpoint de
#      ESCRITA em ctx+0x10 (c_ptr), mantendo os 3 ultimos ctxs vigiados -- o Box
#      e' reciclado em poucos enderecos e o bloco que falha pode ser de um
#      dispatch ANTERIOR (worker atrasado).
#   4. O watchpoint so retem o alvo quando o valor VIRA ZERO (o write que arma o
#      storm). Metadados do allocator (ex.: talc::register_gap gravando 0x38) e
#      o Box::new de um ctx reciclado sao logados e seguem.
#   5. Ao parar: dump do ctx/registradores + backtrace; o PC e' o writer.
#
# LIMITE HONESTO (s488): nas execucoes feitas o watchpoint capturou o allocator
# reciclando o bloco (talc::register_gap), mas NAO o write que zera o c_ptr do
# ctx que falhou -- a origem do 0 segue UNKNOWN (ver SESSION_488.md).
#
# NOTA (s488): "long" e 32-bit no gdb do MinGW (LLP64) -- usar sempre %llx
# com cast, senao o endereco impresso sai truncado (0x4000008c86f0 -> 0x8c86f0).
#
# IMPORTANTE: os enderecos absolutos abaixo valem para o ELF ATUAL
#   (target/limine-esp-tree/kernel.elf). Regenere-os apos rebuild:
#     nm -C --numeric-sort target/limine-esp-tree/kernel.elf | grep -i "parallel_matmul"
#     objdump -d --disassemble="_RNvNtCslelO4CrFVOc_6cortex15parallel_matmul23parallel_ternary_matmul" ...
#   O hbreak do publish tambem pode ser trocado por nome (mais robusto):
#     rbreak AtomicPtr.*TernaryJobCtx.*store
#
# Uso:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\watch_pf_writer.ps1
#   ... -WaitSec 900 -RamGB 5 -Smp 8 -GdbPort 1236
#
# ASCII-only (PS5 le UTF-8 sem BOM como CP1252).
param(
    [int]$WaitSec = 900,
    [int]$RamGB = 5,
    [int]$Smp = 8,
    [int]$GdbPort = 1236,
    [string]$PublishStore = "0xffffffff808f8100", # AtomicPtr<TernaryJobCtx>::store
    [switch]$KeepRunning
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$gdb = "gdb.exe"
$logDir = Join-Path $Root "logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd_HHmmss"
$serialLog = Join-Path $logDir "wpf_serial_$stamp.txt"
$gdbLog = Join-Path $logDir "wpf_gdb_$stamp.txt"
$gdbScript = Join-Path $logDir "wpf_$stamp.gdb"
$elf = Join-Path $Root "target\limine-esp-tree\kernel.elf"

if (!(Test-Path $qemu)) { Write-Host "ERRO: QEMU nao encontrado" -ForegroundColor Red; exit 1 }
if (!(Test-Path $elf)) { Write-Host "ERRO: $elf ausente (cargo build -p boot)" -ForegroundColor Red; exit 1 }
if (!(Get-Command $gdb -ErrorAction SilentlyContinue)) {
    Write-Host "ERRO: gdb nao esta no PATH (MinGW-W64). Instale/adicione ao PATH." -ForegroundColor Red
    exit 1
}

# bpe_vocab: target\ nao tem mais; target1\ e models\ tem.
$bpe = Join-Path $Root "target\bpe_vocab.bin"
foreach ($cand in @("target1\bpe_vocab.bin", "models\bpe_vocab.bin")) {
    if (Test-Path $bpe) { break }
    $p = Join-Path $Root $cand
    if (Test-Path $p) { $bpe = $p }
}
$files = @{
    "uefi.img" = (Join-Path $Root "target\uefi.img"); "disk_qemu.raw" = (Join-Path $Root "target\disk_qemu.raw")
    "ovmf_code.fd" = (Join-Path $Root "target\ovmf_code.fd"); "ovmf_vars.fd" = (Join-Path $Root "target\ovmf_vars.fd")
    "FALCON3.BIN" = (Join-Path $Root "target\FALCON3.BIN"); "bpe_vocab.bin" = $bpe
}
foreach ($k in $files.Keys) {
    if (!(Test-Path $files[$k])) {
        Write-Host "ERRO: $k ausente ($($files[$k]))" -ForegroundColor Red
        exit 1
    }
}

Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2

# gdb script: publish -> watchpoint em ctx+0x10; stop -> dump + bt.
$gdbBody = @"
set pagination off
set confirm off
set width 0
set height 0
file $($elf.Replace('\','/'))
target remote 127.0.0.1:$GdbPort
python
import sys
sys.path.insert(0, r"$($Root.Replace('\','/'))/tools")
import pf_ctx_watch
pf_ctx_watch.init($PublishStore, 0x10)
end
continue
printf "\n[WP] ==== STOP ====\n"
if `$ctx == 0
  printf "[WP] stop sem ctx (rip=0x%llx) - guest reset/reboot?\n", (unsigned long long)`$rip
  detach
  quit
end
printf "[WP] ctx=0x%llx c_ptr=0x%llx m=%lu k=%lu n=%lu rip=0x%llx\n", (unsigned long long)`$ctx, (unsigned long long)*(unsigned long*)(`$ctx+0x10), *(unsigned long*)(`$ctx+0x18), *(unsigned long*)(`$ctx+0x20), (unsigned long long)`$rip
printf "[WP] ctx[0..6] (w_ptr,x_ptr,c_ptr,m,k,n):\n"
x/6gx `$ctx
printf "[WP] regs:\n"
info registers rip rax rcx rdx rsi rdi r8 rsp rbp
printf "[WP] backtrace (quem escreveu no c_ptr):\n"
bt 20
printf "[WP] ==== END ====\n"
detach
quit
"@
Set-Content -Path $gdbScript -Value $gdbBody -Encoding ASCII

$qa = @(
    "-m", "${RamGB}G", "-smp", "$Smp", "-accel", "tcg", "-cpu", "Haswell",
    "-drive", "format=raw,file=$Root\target\uefi.img,if=ide,index=0",
    "-drive", "format=raw,file=$Root\target\disk_qemu.raw,if=ide,index=1",
    "-device", "loader,file=$Root\target\FALCON3.BIN,addr=0x100000000",
    "-device", "loader,file=$($files['bpe_vocab.bin']),addr=0x13DF00000",
    "-device", "loader,file=$Root\target\netmode.flag,addr=0x13E2E4F47",
    "-drive", "if=pflash,format=raw,file=$Root\target\ovmf_code.fd,readonly=on",
    "-drive", "if=pflash,format=raw,file=$Root\target\ovmf_vars.fd",
    "-serial", "file:$serialLog", "-serial", "null",
    "-netdev", "user,id=n0", "-device", "e1000,netdev=n0",
    "-audiodev", "none,id=snd0",
    "-device", "intel-hda,id=hda0",
    "-device", "hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0",
    "-device", "qemu-xhci,id=xhci", "-device", "usb-tablet,bus=xhci.0", "-device", "usb-kbd,bus=xhci.0",
    "-device", "virtio-gpu-pci,id=vgpu", "-vga", "std", "-display", "none",
    "-monitor", "none",
    "-S", "-gdb", "tcp:127.0.0.1:$GdbPort,server,nowait"
)
Start-Process -FilePath $qemu -ArgumentList $qa -WorkingDirectory $Root -WindowStyle Hidden -RedirectStandardError (Join-Path $logDir "wpf_qemu_$stamp.err") | Out-Null
Start-Sleep -Milliseconds 800
Start-Process -FilePath $gdb -ArgumentList @("-batch", "-x", $gdbScript) -WorkingDirectory $Root -WindowStyle Hidden -RedirectStandardOutput $gdbLog -RedirectStandardError (Join-Path $logDir "wpf_gdberr_$stamp.txt") | Out-Null

Write-Host "QEMU (tcg, -S) + gdb lancados." -ForegroundColor Green
Write-Host "  serial : $serialLog"
Write-Host "  gdb    : $gdbLog"
Write-Host "  script : $gdbScript"
Write-Host "Aguardando [WP] PUBLISH / STOP (ate ${WaitSec}s)..."

$deadline = (Get-Date).AddSeconds($WaitSec)
$verdict = "TIMEOUT"
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 5
    if (Test-Path $gdbLog) {
        $txt = Get-Content $gdbLog -Raw -ErrorAction SilentlyContinue
        if ($txt -match "WRITER CAPTURADO") { $verdict = "CAPTURED"; break }
        if ($txt -match "\[WP\] ==== END ====") { $verdict = "CAPTURED"; break }
        if ($txt -match "\[WP\] PUBLISH") { $verdict = "WATCH_ARMED" }
        if ($txt -match "Cannot insert|Could not insert|not supported") { $verdict = "WP_UNSUPPORTED"; break }
    }
    if (-not (Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue)) { $verdict = "QEMU_DIED"; break }
}

Write-Host ""
Write-Host "VEREDITO: $verdict"
if (Test-Path $gdbLog) {
    Write-Host "--- gdb (ultimas 40 linhas) ---"
    Get-Content $gdbLog -Tail 40
}
if (Test-Path $serialLog) {
    $pf = Select-String -Path $serialLog -Pattern "PF_DBG|PF storm|LLM LOADED|BOOT SCORE" -ErrorAction SilentlyContinue | Select-Object -Last 6
    if ($pf) { Write-Host "--- serial (marcos) ---"; $pf | ForEach-Object { $_.Line } }
}

if (-not $KeepRunning) {
    Get-Process qemu-system-x86_64 -ErrorAction SilentlyContinue | Stop-Process -Force
    Get-Process gdb -ErrorAction SilentlyContinue | Stop-Process -Force
}
if ($verdict -eq "CAPTURED") { exit 0 }
if ($verdict -eq "WATCH_ARMED") { exit 2 }
exit 1
