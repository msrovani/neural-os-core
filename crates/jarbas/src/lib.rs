#![cfg_attr(not(test), no_std)]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]

extern crate alloc;

// ─── jarvis: UI, Audio & GPU ───
// Display compositor, audio pipeline, GPU drivers, personality
// Depends on k_nano, cortex, and hermes.

pub mod audio;
pub mod clipboard_notify;
pub mod display;
pub mod gpu;
pub mod image_viewer;
pub mod jarvis;
pub mod screensaver;
pub mod cards;
pub mod uvc_driver;
pub mod vconsole;
pub mod virtio_gpu;
pub mod vision_agent;
