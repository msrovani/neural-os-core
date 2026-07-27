import os, json, struct
d = 'C:/DEV/neural-os-core/models'
for fn in sorted(os.listdir(d)):
    path = os.path.join(d, fn)
    if os.path.isdir(path) or fn.startswith('.') or fn in ['.gitignore']:
        continue
    sz = os.path.getsize(path)
    ext = os.path.splitext(fn)[1].lower()
    print(f'\n=== {fn} ({sz/1024/1024:.0f} MB) ===')
    try:
        with open(path, 'rb') as f:
            if ext == '.safetensors':
                hdr_len = struct.unpack('<Q', f.read(8))[0]
                hdr = json.loads(f.read(hdr_len).decode('utf-8'))
                keys = list(hdr.keys())
                print(f'  Tipo: safetensors, {len(keys)} tensores')
                # Check architecture
                k_str = ' '.join(keys[:20]).lower()
                if 'bert' in k_str or 'xlm' in k_str:
                    print(f'  -> BERT/XLM embedding model')
                elif 'q_proj' in k_str:
                    print(f'  -> BitNet/GPT LLM')
                elif 'encoder' in k_str:
                    print(f'  -> Encoder model')
                print(f'  Ex: {keys[0][:50]}, {keys[-1][:50]}')
            elif ext == '.bin':
                # Try reading as pickle/torch
                magic = f.read(4)
                f.seek(0)
                if magic[:2] == b'PK':
                    print(f'  -> ZIP/Pickle (PyTorch)')
                elif magic[:3] == b'\x80\x02\x8a':
                    print(f'  -> Torch pickle format')
                else:
                    # Read first few strings
                    raw = f.read(1000)
                    # Try to find tensor info
                    text = raw.decode('latin-1')
                    if 'weight' in text.lower():
                        # Find a tensor name
                        import re
                        names = re.findall(r'[a-z_]+\.[a-z_]+\.[a-z]+', text)
                        if names:
                            print(f'  Tensors encontrados: {names[:5]}')
                            is_bert = any('bert' in n for n in names)
                            is_bitnet = any('q_proj' in n for n in names)
                            print(f'  BERT: {is_bert}, BitNet: {is_bitnet}')
                        else:
                            print(f'  RAW bytes (primeiros 64): {raw[:64].hex()[:40]}')
                    else:
                        print(f'  RAW (latin-1): {text[:80]}')
    except Exception as e:
        print(f'  ERRO: {e}')
