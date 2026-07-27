import os, json
d = 'C:/DEV/neural-os-core/models'
for fn in sorted(os.listdir(d)):
    path = os.path.join(d, fn)
    if os.path.isdir(path) or fn.startswith('.'):
        continue
    sz = os.path.getsize(path)
    label = '?'
    try:
        with open(path, 'rb') as f:
            if fn.endswith('.safetensors'):
                hdr_len = int.from_bytes(f.read(8), 'little')
                hdr = json.loads(f.read(hdr_len).decode('utf-8'))
                keys = list(hdr.keys())
                if any('bert' in k.lower() for k in keys):
                    label = 'BERT-embedding'
                elif any('q_proj' in k for k in keys):
                    label = 'BitNet-LLM'
                elif any('embed' in k.lower() for k in keys):
                    label = 'Embedding'
                else:
                    label = f'{len(keys)} tensors, ex: {keys[0][:30]}'
    except:
        label = '(unreadable)'
    print(f'{fn}: {sz/1024/1024:.0f} MB -> {label}')
