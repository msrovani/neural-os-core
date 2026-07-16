#!/usr/bin/env python3
"""Probe bitnet_2B.bitnet layout vs load_model expectations."""
import struct
from pathlib import Path

path = Path(__file__).resolve().parents[1] / "target" / "bitnet_2B.bitnet"
data = path.read_bytes()
print(f"file={path} bytes={len(data)}")

off = 0
magic, ver = struct.unpack_from("<IH", data, off); off += 6
params, hidden, layers, heads = struct.unpack_from("<IHHH", data, off); off += 10
vocab, maxseq = struct.unpack_from("<IH", data, off); off += 6
inter, kv, q_dim = struct.unpack_from("<HHH", data, off); off += 6
medusa = struct.unpack_from("<I", data, off)[0]; off += 4
tied = data[off:off+4]; off += 4
tok_type = data[off]; off += 1
tok_len = struct.unpack_from("<I", data, off)[0]; off += 4
tok = data[off:off+tok_len]; off += tok_len
feat = data[off]; off += 1
print(f"ver={ver} h={hidden} L={layers} heads={heads} vocab={vocab}")
print(f"inter={inter} kv={kv} q_dim={q_dim} medusa={medusa} tied={tied!r} feat=0x{feat:02x}")
print(f"tok={tok!r} data_off={off}")

# peek first 16 bytes of payload
print("payload16:", data[off:off+16].hex())

def tern(r, c):
    return (r * c + 3) // 4

embed = tern(hidden, vocab)
print(f"embed_packed={embed} ({embed/1024:.0f}KB)")

def layer_cost(qd, basic=True, inner=False, ffn=False):
    kdim = kv * (qd // heads)
    ffng = inter * qd // hidden
    down = qd
    t = (tern(hidden, qd) + 2*tern(hidden, kdim) + tern(qd, hidden)
         + 2*tern(hidden, ffng) + tern(inter, down))
    if basic:
        t += hidden * 8
    if inner:
        t += (qd // heads) * heads * 4
    if ffn:
        t += inter * 4
    return t, kdim, ffng

for qd in (2560, 640):
    print(f"\n=== q_dim={qd} ===")
    for basic, inner, ffn_ln in [
        (True, True, True),
        (True, False, False),
        (False, False, False),
        (True, True, False),
        (False, True, True),
    ]:
        per, kdim, ffng = layer_cost(qd, basic, inner, ffn_ln)
        need = embed + per * layers
        rem = len(data) - off
        delta = abs(rem - per * layers)
        # also allow final rms + no unembed
        print(f"  rms={int(basic)} inner={int(inner)} ffn={int(ffn_ln)} "
              f"per={per} need_layers={per*layers} rem={rem} d={delta} "
              f"total~{need} file={len(data)} fit={'YES' if need <= len(data) else 'NO'}")

# Simulate old failure: q_dim=2560 basic rms, no layout search
per2560, _, _ = layer_cost(2560, True, False, False)
# with feat 0x07 -> inner+ffn+rope claimed
per2560_full, _, _ = layer_cost(2560, True, True, True)
off_sim = off + embed
print(f"\nsim old q2560 basic: layers_fit={(len(data)-off_sim)//per2560}")
print(f"sim old q2560 fullfeat: layers_fit={(len(data)-off_sim)//per2560_full}")
per640, _, _ = layer_cost(640, False, False, False)
print(f"sim q640 tern-only: layers_fit={(len(data)-off_sim)//per640} need={per640*layers} rem={len(data)-off_sim}")
per640b, _, _ = layer_cost(640, True, False, False)
print(f"sim q640 +basic: layers_fit={(len(data)-off_sim)//per640b} need={per640b*layers} rem={len(data)-off_sim} Δ={abs(len(data)-off_sim-per640b*layers)}")
per640f, _, _ = layer_cost(640, True, True, True)
print(f"sim q640 +allrms: need={per640f*layers} rem={len(data)-off_sim} Δ={abs(len(data)-off_sim-per640f*layers)}")

# Check if length-prefixed format (convert_*.py style)
c0, p0 = struct.unpack_from("<II", data, off)
print(f"\nif length-prefix: count0={c0} pad0={p0} packed0={(c0+3)//4}")
