#!/bin/bash
# s490: falsificador da correcao epoch/barreira (SESSION_490).
# Baseline medido (mesmo setup, ANTES do fix): 4 de 7 boots com "#PF storm".
# Depois do fix: esperado 0 storms em 4 boots.
Q="/c/Program Files/qemu/qemu-system-x86_64.exe"
R="/c/DEV/neural-os-core-latest"
cd "$R" || exit 1
SECS=${1:-330}
N=${2:-4}
for n in $(seq 1 "$N"); do
  L="logs/epoch_boot_$n.txt"
  rm -f "$L"
  "$Q" -m 5G -smp 8 -accel tcg -cpu Haswell \
    -drive format=raw,file=target/uefi.img,if=ide,index=0 \
    -drive format=raw,file=target/disk_qemu.raw,if=ide,index=1 \
    -device loader,file=target/FALCON3.BIN,addr=0x100000000 \
    -device loader,file=target1/bpe_vocab.bin,addr=0x13DF00000 \
    -device loader,file=target/netmode.flag,addr=0x13E2E4F47 \
    -drive if=pflash,format=raw,file=target/ovmf_code.fd,readonly=on \
    -drive if=pflash,format=raw,file=target/ovmf_vars.fd \
    -serial file:"$L" -serial null \
    -netdev user,id=n0 -device e1000,netdev=n0 \
    -audiodev none,id=snd0 -device intel-hda,id=hda0 \
    -device hda-duplex,id=hda-codec,bus=hda0.0,cad=0,audiodev=snd0 \
    -device qemu-xhci,id=xhci -device usb-tablet,bus=xhci.0 -device usb-kbd,bus=xhci.0 \
    -device virtio-gpu-pci,id=vgpu -vga std -display none -monitor none \
    > "logs/epoch_qemu_$n.err" 2>&1 &
  pid=$!
  sleep "$SECS"
  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null
  storms=$(grep -c "PF storm" "$L" 2>/dev/null || true)
  pf=$(grep -c "PF_DBG" "$L" 2>/dev/null || true)
  matmul=$(grep -c "matmul enter" "$L" 2>/dev/null || true)
  bootscore=$(grep -c "BOOT SCORE" "$L" 2>/dev/null || true)
  echo "boot $n: storms=$storms pf_dbg=$pf matmul_enter=$matmul boot_score=$bootscore"
done
