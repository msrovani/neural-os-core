#!/usr/bin/env python3
"""Generate synthetic TQ2_0 GGUF test file with correct GGUFValueType IDs.
GGUFValueType: 0=UINT8 1=INT8 2=UINT16 3=INT16 4=UINT32 5=INT32
              6=FLOAT32 7=BOOL 8=STRING 9=ARRAY 10=FLOAT64
"""
import struct, sys, os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def write_u32(arr, v):
    arr += struct.pack('<I', v)

def write_u64(arr, v):
    arr += struct.pack('<Q', v)

def write_string(arr, s):
    b = s.encode('utf-8')
    write_u64(arr, len(b))
    arr += b

def f16_encode(v):
    """Convert float to IEEE 754 half-precision."""
    import math
    if v == 0:
        return 0
    sign = 0 if v >= 0 else 1
    v = abs(v)
    exp = math.floor(math.log2(v)) if v > 0 else 0
    mant = v / (2.0 ** exp) - 1.0
    biased_exp = exp + 15
    mantissa = int(mant * 1024) & 0x3FF
    return (sign << 15) | (biased_exp << 10) | mantissa

data = bytearray()

# GGUF Header
data += b'GGUF'
write_u32(data, 3)   # version=3
write_u64(data, 1)   # tensor_count=1
write_u64(data, 2)   # metadata_kv_count=2

# Metadata[0]: general.architecture = "llama" (type 8=STRING)
write_string(data, 'general.architecture')
write_u32(data, 8)   # STRING
write_string(data, 'llama')

# Metadata[1]: general.type = "model" (type 8=STRING)
write_string(data, 'general.type')
write_u32(data, 8)   # STRING
write_string(data, 'model')

# Tensor info: blk.0.attn_q.weight [4,4] TQ2_0(type=25)
write_string(data, 'blk.0.attn_q.weight')
write_u32(data, 2)   # n_dims=2
write_u64(data, 4)   # dim0
write_u64(data, 4)   # dim1
write_u32(data, 25)  # tensor_type=TQ2_0
write_u64(data, 0)   # offset=0 (relative to data_start)

# Align to 32 bytes
while len(data) % 32 != 0:
    data += b'\x00'

# Tensor data: one TQ2_0 block (24 bytes for 32 elements)
# Weights: +1,+1,-1,-1, 0,0,+1,-1, +1,0,-1,+1, 0,+1,-1,0 (16 values)
# Encoding: 00=0, 01=+1, 10=-1
# byte0: w0=+1(01) w1=+1(01) w2=-1(10) w3=-1(10) = 0b10_10_01_01 = 0xA5
# byte1: w4=0(00)  w5=0(00)  w6=+1(01) w7=-1(10) = 0b10_01_00_00 = 0x90
# byte2: w8=+1(01) w9=0(00)  w10=-1(10) w11=+1(01) = 0b01_10_00_01 = 0x41
# byte3: w12=0(00) w13=+1(01) w14=-1(10) w15=0(00) = 0b00_10_01_00 = 0x24

scale = 1.5
f16_scale = f16_encode(scale)
data += struct.pack('<H', f16_scale)
data += bytes([0xA5, 0x90, 0x61, 0x24])  # 0x61 not 0x41: w10=-1(10) at bits[5:4]
data += bytes(18)  # pad to 24 bytes

out_path = os.path.join(ROOT, 'target', 'test_tq2_0.gguf')
os.makedirs(os.path.dirname(out_path), exist_ok=True)
with open(out_path, 'wb') as f:
    f.write(data)
print(f'Generated {out_path} ({len(data)} bytes)')
print(f'f16(1.5) = 0x{f16_scale:04X}')

# Verify
with open(out_path, 'rb') as f:
    magic = f.read(4)
    ver = struct.unpack('<I', f.read(4))[0]
    tc = struct.unpack('<Q', f.read(8))[0]
    mkv = struct.unpack('<Q', f.read(8))[0]
    print(f'Verify: magic={magic} ver={ver} tensors={tc} meta={mkv}')
    for i in range(mkv):
        klen = struct.unpack('<Q', f.read(8))[0]
        key = f.read(klen).decode()
        vt = struct.unpack('<I', f.read(4))[0]
        if vt == 8:  # STRING
            slen = struct.unpack('<Q', f.read(8))[0]
            val = f.read(slen).decode()
            print(f'  meta: {key} = {val!r} (type={vt})')
        else:
            print(f'  meta: {key} type={vt}')
    for i in range(tc):
        nlen = struct.unpack('<Q', f.read(8))[0]
        name = f.read(nlen).decode()
        nd = struct.unpack('<I', f.read(4))[0]
        dims = [struct.unpack('<Q', f.read(8))[0] for _ in range(nd)]
        tt = struct.unpack('<I', f.read(4))[0]
        off = struct.unpack('<Q', f.read(8))[0]
        print(f'  tensor: {name} dims={dims} type={tt} offset={off}')
print('OK')
