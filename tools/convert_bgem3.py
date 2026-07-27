import torch, json, os, sys
sys.path.insert(0, os.path.dirname(__file__))
from convert_safetensors_to_bitnet import convert_safetensors_to_bitnet

path = '../models/pytorch_model (1).bin'
print(f'Carregando BGE-m3 ({os.path.getsize(path)/1024/1024:.0f} MB)...', flush=True)
state = torch.load(path, map_location='cpu', weights_only=True)
print(f'{len(state)} tensores carregados', flush=True)

# Save as safetensors
tmp = '../target/bgem3_pt.safetensors'
from safetensors.torch import save_file as sf
sf(state, tmp)
sz = os.path.getsize(tmp)
print(f'Safetensors salvo: {sz/1024/1024:.0f} MB', flush=True)

# Config
cfg = {
    'architectures': ['XLMRobertaModel'],
    'hidden_size': 1024,
    'intermediate_size': 4096,
    'num_attention_heads': 16,
    'num_hidden_layers': 24,
    'vocab_size': 250002,
    'max_position_embeddings': 8194,
    'model_type': 'xlm-roberta',
    'torch_dtype': 'float32'
}
cfg_path = '../target/bgem3_config.json'
json.dump(cfg, open(cfg_path, 'w'))

# Convert to .BIN
convert_safetensors_to_bitnet(tmp, cfg_path, '../target/BGE_M3.BIN')
