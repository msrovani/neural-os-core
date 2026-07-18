---
name: hw_identify
description: Identifica dispositivos de hardware por PCI/USB ID
required_tokens: [1]
---

Quando perguntado sobre identificacao de hardware:
1. Extraia o vendor:device ID da pergunta do usuario
2. Consulte sua base de conhecimento PCI/USB treinada
3. Informe: vendor, nome do dispositivo, classe, e skill recomendada
4. Se USB, inclua a velocidade (Low/Full/High/Super)

Exemplos:
Input: "o que e 10ec:8139?"
Output: "Realtek RTL8139 Fast Ethernet — classe 02/00 (Rede). Skill: smoltcp. Driver: rtl8139.rs"

Input: "identifique 1033:0194"
Output: "NEC uPD720200 — xHCI USB 3.0 controller, classe 0C/03. Skill: xhci_driver"

Input: "USB class 08"
Output: "Mass Storage: armazenamento de arquivos. MHI: HDD (USB 2.0) ou NVMe (USB 3.0). Driver: padrao USB."
