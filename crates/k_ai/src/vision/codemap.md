# k_ai/src/vision — codemap

**Responsibility:** on-device vision: SigLIP ViT-B/16 image encoder and text-region detection; the LLM "reads" text from detected regions via embeddings (no separate OCR model).

**Key symbols:**
- `vit.rs` — `VisionEncoder`: loads `VISION.BIN` (.bitnet v5, RTN+scale), 384×384 RGBA → 768-dim embedding; Conv patch embed (3→768, k16/s16, 576 patches) + CLS + 12 encoder layers (MHA 12h + FFN GELU); matmul via `cortex::{tensor, bitnet_sse}`.
- `ocr.rs` — `OcrEngine::detect_text(rgba, w, h)` → `Vec<TextRegion>` (x,y,w,h): grayscale → Otsu binarization → horizontal/vertical projection → line/word boxes, top-to-bottom left-to-right; crops are passed to VisionEncoder for LLM reading.

**Integration:** `pub use vit::VisionEncoder; pub use ocr::OcrEngine;` at module root; tensors/ternary matmul from cortex; soft-float math via libm.
