//! neural-logwriter — UEFI stage-0 no mesmo stick (Trilha A / SESSION_169).
//!
//! 1. Lê ramlog físico do boot anterior (`0x1000_0000`, magic `NEURLOG!` + CRC24).
//! 2. Grava em `BOOT.LOG` via `SimpleFileSystem` do firmware.
//! 3. Marca `NEURDONE` e chain-loada `\EFI\neural\limine.efi`.
//!
//! Layout do header **deve** espelhar `k_nano::boot_ramlog` (sem depender da crate).

#![no_main]
#![no_std]

extern crate alloc;

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use log::{error, info, warn};
use uefi::boot::{self, LoadImageSource, SearchType};
use uefi::prelude::*;
use uefi::proto::device_path::build::DevicePathBuilder;
use uefi::proto::device_path::{build, DevicePath, DeviceType};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::BootPolicy;
use uefi::CStr16;

/// Espelha `k_nano::boot_ramlog::BOOT_RAMLOG_PHYS`.
const RAMLOG_PHYS: usize = 0x1000_0000;
const RAMLOG_CAP: usize = 256 * 1024;
const HDR_SIZE: usize = 16;
const MAGIC_NEED: u64 = u64::from_le_bytes(*b"NEURLOG!");
const MAGIC_DONE: u64 = u64::from_le_bytes(*b"NEURDONE");
const BOOT_LOG_CAP: usize = 256 * 1024;

#[repr(C)]
struct RamLogHeader {
    magic: u64,
    len: u32,
    crc_and_ckpt: u32,
}

fn crc32_24(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (!(crc & 1)).wrapping_add(1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    (!crc) & 0x00FF_FFFF
}

fn pack_crc_ckpt(crc: u32, ckpt: u8) -> u32 {
    (crc & 0x00FF_FFFF) | ((ckpt as u32) << 24)
}

fn unpack_ckpt(v: u32) -> u8 {
    (v >> 24) as u8
}

unsafe fn take_pending() -> Option<(Vec<u8>, u8)> {
    let hdr = &*(RAMLOG_PHYS as *const RamLogHeader);
    if hdr.magic != MAGIC_NEED {
        return None;
    }
    let len = (hdr.len as usize).min(RAMLOG_CAP.saturating_sub(HDR_SIZE));
    if len == 0 {
        return None;
    }
    let data = core::slice::from_raw_parts((RAMLOG_PHYS + HDR_SIZE) as *const u8, len);
    let want = hdr.crc_and_ckpt & 0x00FF_FFFF;
    let got = crc32_24(data);
    // Fail-closed: CRC 0 = selo incompleto / lixo — não gravar (bughunt MED).
    if want == 0 {
        warn!("ramlog CRC=0 (selo incompleto) — skip write");
        return None;
    }
    if want != got {
        warn!("ramlog CRC fail want={want:#x} got={got:#x} — skip write");
        return None;
    }
    let ckpt = unpack_ckpt(hdr.crc_and_ckpt);
    Some((data.to_vec(), ckpt))
}

unsafe fn mark_done(ckpt: u8) {
    let hdr = &mut *(RAMLOG_PHYS as *mut RamLogHeader);
    hdr.magic = MAGIC_DONE;
    hdr.crc_and_ckpt = pack_crc_ckpt(0, ckpt);
}

fn c16<'a>(s: &str, buf: &'a mut [u16]) -> Option<&'a CStr16> {
    let mut i = 0;
    for ch in s.encode_utf16() {
        if i + 1 >= buf.len() {
            return None;
        }
        buf[i] = ch;
        i += 1;
    }
    buf[i] = 0;
    CStr16::from_u16_with_nul(&buf[..=i]).ok()
}

fn try_write_regular(
    root: &mut impl File,
    path: &CStr16,
    payload: &[u8],
    mode: FileMode,
) -> bool {
    let Ok(handle) = root.open(path, mode, FileAttribute::empty()) else {
        return false;
    };
    let Ok(FileType::Regular(mut f)) = handle.into_type() else {
        return false;
    };
    let _ = f.set_position(0);
    if f.write(payload).is_err() {
        return false;
    }
    let _ = f.flush();
    true
}

fn write_bootlog(payload: &[u8]) -> Result<usize, &'static str> {
    let handles = boot::locate_handle_buffer(SearchType::from_proto::<SimpleFileSystem>())
        .map_err(|_| "no SimpleFileSystem")?;

    let mut path_buf = [0u16; 64];
    let boot_log = c16("BOOT.LOG", &mut path_buf).ok_or("cstr")?;

    for &handle in handles.iter() {
        let Ok(mut sfs) = boot::open_protocol_exclusive::<SimpleFileSystem>(handle) else {
            continue;
        };
        let Ok(mut root) = sfs.open_volume() else {
            continue;
        };
        // Volume de dados: BOOT.LOG pré-alocado — ReadWrite (não Create).
        if try_write_regular(&mut root, boot_log, payload, FileMode::ReadWrite) {
            return Ok(payload.len());
        }
    }

    write_bootlog_on_esp(payload)
}

fn write_bootlog_on_esp(payload: &[u8]) -> Result<usize, &'static str> {
    let image = boot::image_handle();
    let loaded = boot::open_protocol_exclusive::<LoadedImage>(image).map_err(|_| "LoadedImage")?;
    let device = loaded.device().ok_or("no device")?;
    let mut sfs =
        boot::open_protocol_exclusive::<SimpleFileSystem>(device).map_err(|_| "SFS on ESP")?;
    let mut root = sfs.open_volume().map_err(|_| "open volume")?;

    let mut dir_buf = [0u16; 32];
    let neural = c16("NEURAL", &mut dir_buf).ok_or("cstr")?;
    let _ = root.open(
        neural,
        FileMode::CreateReadWrite,
        FileAttribute::DIRECTORY,
    );

    let mut path_buf = [0u16; 64];
    let path = c16("NEURAL\\BOOT.LOG", &mut path_buf).ok_or("cstr")?;
    if try_write_regular(&mut root, path, payload, FileMode::CreateReadWrite) {
        Ok(payload.len())
    } else {
        Err("create NEURAL\\BOOT.LOG")
    }
}

/// Chainload Limine via **FromDevicePath** (path completo do volume).
///
/// `FromBuffer` sem `file_path` funciona no OVMF, mas em firmware real o Limine
/// perde o dispositivo de origem → `boot():/kernel.elf` falha → tela preta após
/// o menu (ou durante o load). Path canônico:
/// `HD(...)/\\EFI\\neural\\limine.efi`
fn chainload_limine() -> Result<(), &'static str> {
    let image = boot::image_handle();
    let loaded = boot::open_protocol_exclusive::<LoadedImage>(image).map_err(|_| "LoadedImage")?;
    let device = loaded.device().ok_or("no device")?;

    // Device path do volume (sem FilePath) — nós até END.
    let device_path =
        boot::open_protocol_exclusive::<DevicePath>(device).map_err(|_| "DevicePath")?;

    let mut path_buf = [0u16; 80];
    let limine_path = c16("\\EFI\\neural\\limine.efi", &mut path_buf).ok_or("cstr")?;

    let mut v: Vec<u8> = Vec::with_capacity(512);
    let mut builder = DevicePathBuilder::with_vec(&mut v);
    for node in device_path.node_iter() {
        if node.device_type() == DeviceType::END {
            break;
        }
        builder = builder.push(&node).map_err(|_| "dp push")?;
    }
    builder = builder
        .push(&build::media::FilePath {
            path_name: limine_path,
        })
        .map_err(|_| "dp file")?;
    let full_path = builder.finalize().map_err(|_| "dp finalize")?;

    info!("chainload FromDevicePath \\EFI\\neural\\limine.efi");
    let handle = boot::load_image(
        image,
        LoadImageSource::FromDevicePath {
            device_path: full_path,
            boot_policy: BootPolicy::ExactMatch,
        },
    )
    .map_err(|_| "LoadImage FromDevicePath")?;

    // Não retorna se Limine/kernel tomar o controle.
    let _ = boot::start_image(handle);
    Ok(())
}

/// Marca no ESP que o stage-0 rodou (evidência metal sem serial).
fn write_stage0_marker() {
    let marker = b"[S] neural-logwriter stage-0 OK (chainload next)\n";
    let _ = write_bootlog_on_esp(marker);
}

fn build_payload(body: &[u8], ckpt: u8) -> Vec<u8> {
    let mut out: Vec<u8> = vec![0xEF, 0xBB, 0xBF];
    let hdr = format!(
        "[S] neural-os-core BOOT.LOG via logwriter-efi ckpt=K{ckpt} bytes={}\n",
        body.len()
    );
    out.extend_from_slice(hdr.as_bytes());
    out.extend_from_slice(body);
    if out.len() > BOOT_LOG_CAP {
        out.truncate(BOOT_LOG_CAP);
    }
    out
}

#[entry]
fn main() -> Status {
    if uefi::helpers::init().is_err() {
        // Segue sem logger.
    }

    info!("neural-logwriter: boot stage-0");
    // Evidência metal (ESP) mesmo sem serial: NEURAL\BOOT.LOG.
    write_stage0_marker();

    match unsafe { take_pending() } {
        Some((body, ckpt)) => {
            let payload = build_payload(&body, ckpt);
            match write_bootlog(&payload) {
                Ok(n) => {
                    info!("BOOT.LOG gravado ({n} bytes, ckpt=K{ckpt})");
                    unsafe { mark_done(ckpt) };
                }
                Err(e) => {
                    error!("BOOT.LOG write FAIL: {e}");
                }
            }
        }
        None => {
            info!("ramlog ausente/vazio — 1o boot ou ja consumido");
        }
    }

    match chainload_limine() {
        Ok(()) => Status::SUCCESS,
        Err(e) => {
            error!("chainload limine FAIL: {e}");
            boot::stall(5_000_000);
            Status::LOAD_ERROR
        }
    }
}
