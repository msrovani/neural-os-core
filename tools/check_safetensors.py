import os
d = 'C:/DEV/neural-os-core/models'
for fn in sorted(os.listdir(d)):
    if not fn.endswith('.safetensors'):
        continue
    path = os.path.join(d, fn)
    sz = os.path.getsize(path)
    with open(path, 'rb') as f:
        hdr_len = int.from_bytes(f.read(8), 'little')
        hdr = f.read(min(hdr_len, 2000)).decode('utf-8')
    # Count tensors
    tensors = hdr.count('"dtype"')
    # Check for key names
    has_wq = 'q_proj' in hdr or 'weight_scale' in hdr
    has_bert = 'bert' in hdr.lower() or 'sentence' in hdr.lower()
    has_config = 'model' in fn.lower() or 'bitnet' in fn.lower()
    first_key = ''
    if tensors > 0:
        for line in hdr.split(','):
            if '"dtype"' in line:
                parts = line.split('"')
                first_key = parts[0].strip(':{} ') if len(parts) > 0 else ''
                break
    print(f'{fn}: {sz/1024/1024:.0f}MB, tensors={tensors}')
    print(f'  first key: {first_key[:60]}')
    if has_wq: print(f'  -> BitNet (q_proj)')
    if has_bert: print(f'  -> BGE (bert)')
    if not has_wq and not has_bert: print(f'  -> Unknown')
