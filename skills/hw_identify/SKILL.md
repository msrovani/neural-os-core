---
name: hw_identify
description: Identifica dispositivos de hardware por PCI/USB ID e sugere driver
required_tokens: [1]
requires_network: false
---

# Hardware Identify Skill

Quando perguntado sobre identificacao de hardware, responda com vendor, dispositivo, classe e driver.

## Workflow

1. Extraia o vendor:device ID (formato `XXXX:XXXX`) ou USB class da pergunta do usuario
2. Consulte sua base de conhecimento PCI/USB treinada (HW Expert v3)
3. Responda: **vendor**, **nome do dispositivo**, **classe** (hex), e **skill/driver recomendado**
4. Se for USB, inclua a velocidade (Low/Full/High/Super)
5. Se nao encontrar o ID, informe "ID nao catalogado" e sugira como contribuir com a base

## Tratamento de Erros

- **Formato invalido**: IDs devem ser hex de 4 digitos (ex: `10ec:8139`). Se nao encaixar, explique o formato esperado
- **ID desconhecido**: "Device 1234:5678 nao encontrado na base. Pode ser muito recente ou especializado. Sugiro testar no Linux com `lspci -n`"
- **Classe USB generica**: responda com a descricao padrao da classe (ex: class 08 = Mass Storage)
- **Multiplos matches**: se o vendor:device bater com varios dispositivos, liste as variantes conhecidas

## Exemplos

Input: "o que e 10ec:8139?"
Output: "Realtek RTL8139 Fast Ethernet — classe 02/00 (Rede). Driver: rtl8139.rs. Skill: smoltcp"

Input: "identifique 1033:0194"
Output: "NEC uPD720200 — xHCI USB 3.0 controller, classe 0C/03. Skill: xhci_driver. Velocidade: SuperSpeed (5Gbps)"

Input: "USB class 08"
Output: "Mass Storage: armazenamento de arquivos. Pode ser HDD (USB 2.0, 480Mbps) ou NVMe (USB 3.0, 5Gbps). Driver: usb_storage compativel"

Input: "meu device 9999:0001 nao funciona"
Output: "ID 9999:0001 nao catalogado na base HW Expert. Tente buscar em https://pci-ids.ucw.cz ou contribuir com a base de dispositivos."

## Regras de Seguranca

- Nao invente IDs — se nao souber, diga que nao conhece
- Nao sugira drivers que nao existem no codigo (apenas: rtl8139, e1000, virtio-net, xhci, ahci, nvme, ata, hda, i915, iwlwifi)
- Nao confunda vendor:device com enderecos de barramento (00:1f.3 nao e um ID)
