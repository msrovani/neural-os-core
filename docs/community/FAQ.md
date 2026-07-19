# FAQ — Device LEGOs

**Posso mandar `.sys` / DriverPack?**  
Não. Só metadata HWID sanitizada.

**GSP NVIDIA no PR?**  
Não. Só em `target/firmware/` local.

**Instalei a recipe e não tem WiFi?**  
Install ≠ Ready. Veja stages UnlockDAG e serial (`VERDICT=`). Checklist em [NEURALFS_LAYOUT.md](../specs/device-lego/NEURALFS_LAYOUT.md).

**Unsigned pode fazer bind Auto?**  
Não. Draft Escalate; bind MMIO só com `verify_trusted`.

**SDIO substitui o driver?**  
Não. Só inventário/roteamento.

**QEMU valida ath10k?**  
Não há `168C:003E` típico. Aceite = Note / HW real.

**Tom's Hardware é contribuidor?**  
Cobertura = Supporter (media), não Contributor de código.

**Onde ficam os LEGOs no FS?**  
`/mnt/neural/ecosystem/devices/<name>/RECIPE.md` + blobs em `ecosystem/firmware/`.
