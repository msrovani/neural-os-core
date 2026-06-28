#!/usr/bin/env python3
import struct, sys, os, argparse

def create_fat12(total_sectors):
    """Cria filesystem FAT12 completo com 1 arquivo vazio (BOOT.LOG)."""
    bytes_ps = 512
    fats = 2
    root_entries = 16
    reserved = 1

    fat_sectors = ((total_sectors * 3 + 1) // 2 + bytes_ps - 1) // bytes_ps + 1
    root_sectors = (root_entries * 32 + bytes_ps - 1) // bytes_ps
    img = bytearray(total_sectors * bytes_ps)

    # BPB byte-by-byte (struct pack is error-prone for FAT BPB)
    bpb = bytearray(512)
    bpb[0:3] = b'\xEB\x3C\x90'
    bpb[3:11] = b'NEURALOS'
    struct.pack_into('<H', bpb, 11, 512)       # bytes per sector
    bpb[13] = 1                                  # sectors per cluster
    struct.pack_into('<H', bpb, 14, 1)           # reserved sectors
    bpb[16] = 2                                  # FAT count
    struct.pack_into('<H', bpb, 17, 16)          # root entries
    struct.pack_into('<H', bpb, 19, total_sectors) # total sectors
    bpb[21] = 0xF8                               # media
    struct.pack_into('<H', bpb, 22, fat_sectors) # sectors per FAT
    struct.pack_into('<H', bpb, 24, 0)           # sectors per track
    struct.pack_into('<H', bpb, 26, 0)           # heads
    struct.pack_into('<I', bpb, 28, 0)           # hidden sectors
    struct.pack_into('<I', bpb, 32, 0)           # total sectors (32)
    bpb[36] = 0x29                               # extended boot sig
    struct.pack_into('<I', bpb, 39, 0xDEADBEEF)  # volume ID
    bpb[43:54] = b'NEURALOS_LOG'
    bpb[54] = 0x00
    bpb[0x36:0x3E] = b'FAT12   '
    bpb[0x1FE:0x200] = b'\x55\xAA'              # boot signature
    img[:512] = bpb

    # FAT: fill all clusters with 0xFFF (EOC, allocated to 1 file)
    fat_data = bytearray(fat_sectors * 512)
    struct.pack_into('<H', fat_data, 0, 0xFF8)   # cluster 0: media
    struct.pack_into('<H', fat_data, 2, 0xFFF)   # cluster 1: EOC
    for i in range(2, min(data_sectors := total_sectors - reserved - fats * fat_sectors - root_sectors, len(fat_data) // 2)):
        off = i * 3 // 2
        if i % 2 == 0: struct.pack_into('<H', fat_data, off, 0xFFF)
        else:
            v = struct.unpack_from('<H', fat_data, off)[0]
            struct.pack_into('<H', fat_data, off, v | (0xFFF << 4))

    fat1_off = reserved * 512
    fat2_off = (reserved + fat_sectors) * 512
    img[fat1_off:fat1_off + len(fat_data)] = fat_data
    img[fat2_off:fat2_off + len(fat_data)] = fat_data

    # Root directory: BOOT.LOG, cluster=2, size=0
    root_off = (reserved + fats * fat_sectors) * 512
    entry = bytearray(32)
    entry[0:11] = b'BOOT    LOG'
    entry[11] = 0x20  # archive
    entry[26] = 0x02  # first cluster low
    entry[27] = 0x00  # first cluster high
    entry[28:32] = (0).to_bytes(4, 'little')  # size = 0
    img[root_off:root_off + 32] = entry

    return img

def patch_bootimage(bootimage_path, output_path, log_size_mb=2):
    with open(bootimage_path, 'rb') as f:
        data = bytearray(f.read())
    kernel_sectors = (len(data) + 511) // 512
    if kernel_sectors % 2 != 0: kernel_sectors += 1
    fat_sectors = log_size_mb * 2048
    fat_img = create_fat12(fat_sectors)

    out = bytearray(kernel_sectors * 512)
    out[:len(data)] = data
    out += fat_img

    # MBR partition table
    mbr = bytearray(512)
    struct.pack_into('<B', mbr, 0x1BE, 0x80)  # bootable
    mbr[0x1BF] = 0x00; mbr[0x1C0] = 0x02; mbr[0x1C1] = 0x00  # CHS
    mbr[0x1C2] = 0x01  # FAT12
    mbr[0x1C3] = 0xFF; mbr[0x1C4] = 0xFF; mbr[0x1C5] = 0xFF
    struct.pack_into('<I', mbr, 0x1C6, 0)           # LBA start = 0
    struct.pack_into('<I', mbr, 0x1CA, kernel_sectors)  # size

    struct.pack_into('<B', mbr, 0x1CE, 0)  # non-bootable
    mbr[0x1D2] = 0x01                       # FAT12
    struct.pack_into('<I', mbr, 0x1D6, kernel_sectors)  # LBA start
    struct.pack_into('<I', mbr, 0x1DA, fat_sectors)     # size
    mbr[0x1FE:0x200] = b'\x55\xAA'
    out[:512] = mbr

    with open(output_path, 'wb') as f:
        f.write(bytes(out))
    print(f"[PATCH] Kernel: {len(data)} bytes ({kernel_sectors} setores)")
    print(f"[PATCH] Particao FAT12: {fat_sectors} setores ({log_size_mb} MB)")
    print(f"[PATCH] Total: {len(out)} bytes -> {output_path}")

def main():
    parser = argparse.ArgumentParser(description='Patch bootimage with FAT12 log partition')
    parser.add_argument('input')
    parser.add_argument('--output', '-o', default=None)
    parser.add_argument('--size', '-s', type=int, default=2)
    args = parser.parse_args()
    if args.output is None:
        name, ext = os.path.splitext(args.input)
        args.output = f"{name}_patched{ext}"
    patch_bootimage(args.input, args.output, args.size)

if __name__ == '__main__':
    main()
