//! Vision encoder — SigLIP ViT-B/16 384px → embedding 768d
//! Carrega VISION.BIN (formato .bitnet v5) e executa forward pass.
//! Depende de k_nano (memória/alocação), cortex (ternary matmul), libm (soft-float).

#![allow(dead_code)]

pub mod vit;
pub mod ocr;

pub use vit::VisionEncoder;
pub use ocr::OcrEngine;
