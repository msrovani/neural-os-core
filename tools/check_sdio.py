import os
d = 'models/SDIO'
files = sorted(os.listdir(d))
print(f'Total files: {len(files)}')
total = sum(os.path.getsize(os.path.join(d, f)) for f in files)
print(f'Total size: {total/1024/1024:.1f} MB')
# Header do primeiro
first = os.path.join(d, files[0])
sz = os.path.getsize(first)
print(f'First: {files[0]} ({sz} bytes)')
with open(first, 'rb') as f:
    hdr = f.read(32)
    print('Header:', ' '.join(f'{b:02x}' for b in hdr[:16]))
# Check for HWID rows (look for 'hwid' or 'VID' text)
txt = open(first, 'rb').read().decode('latin-1')
print(f'Contains hwid: {"hwid" in txt[:2000]}')
print(f'Contains VID: {"VID" in txt[:2000]}')
print(f'Contains PCI: {"PCI" in txt[:2000]}')
print(f'Contains USB: {"USB" in txt[:2000]}')
# Count JSON files
json_files = [f for f in files if f.endswith('.json')]
print(f'JSON files: {len(json_files)}')
print(f'Sample JSON names: {json_files[:5]}')
by_ext = {}
for f in files:
    ext = f.rsplit('.', 1)[-1].lower() if '.' in f else '(none)'
    by_ext[ext] = by_ext.get(ext, 0) + 1
print('Files by type:', by_ext)
