#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

use crate::memory_descriptor::UefiMemoryDescriptor;
use bootloader_api::info::FrameBufferInfo;
use bootloader_boot_config::BootConfig;
use bootloader_x86_64_common::{
    Kernel, RawFrameBufferInfo, SystemInfo, legacy_memory_region::LegacyFrameAllocator,
};
use core::{
    cell::UnsafeCell,
    fmt::Write,
    ops::{Deref, DerefMut},
    ptr, slice,
};
use uefi::{
    CStr8, CStr16,
    prelude::{Boot, Handle, Status, SystemTable, entry},
    proto::{
        ProtocolPointer,
        console::gop::{GraphicsOutput, PixelFormat},
        device_path::DevicePath,
        loaded_image::LoadedImage,
        media::{
            file::{File, FileAttribute, FileInfo, FileMode},
            fs::SimpleFileSystem,
        },
        network::{
            IpAddress,
            pxe::{BaseCode, DhcpV4Packet},
        },
    },
    table::boot::{
        AllocateType, MemoryType, OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol,
        SearchType,
    },
};
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
};

mod fb_pick;
mod memory_descriptor;

static SYSTEM_TABLE: RacyCell<Option<SystemTable<Boot>>> = RacyCell::new(None);

struct RacyCell<T>(UnsafeCell<T>);

impl<T> RacyCell<T> {
    const fn new(v: T) -> Self {
        Self(UnsafeCell::new(v))
    }
}

unsafe impl<T> Sync for RacyCell<T> {}

impl<T> core::ops::Deref for RacyCell<T> {
    type Target = UnsafeCell<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[entry]
fn efi_main(image: Handle, st: SystemTable<Boot>) -> Status {
    main_inner(image, st)
}

fn main_inner(image: Handle, mut st: SystemTable<Boot>) -> Status {
    // temporarily clone the y table for printing panics
    unsafe {
        *SYSTEM_TABLE.get() = Some(st.unsafe_clone());
    }

    // Soft-reboot do kernel: grava BOOT.LOG no volume FAT32 de dados ANTES de carregar o kernel.
    flush_boot_ramlog_if_needed(image, &mut st);

    let mut boot_mode = BootMode::Disk;

    let mut kernel = load_kernel(image, &mut st, boot_mode);
    if kernel.is_none() {
        // Try TFTP boot
        boot_mode = BootMode::Tftp;
        kernel = load_kernel(image, &mut st, boot_mode);
    }
    let kernel = kernel.expect("Failed to load kernel");

    let config_file = load_config_file(image, &mut st, boot_mode);
    let mut error_loading_config: Option<serde_json_core::de::Error> = None;
    let mut config: BootConfig = match config_file
        .as_deref()
        .map(serde_json_core::from_slice)
        .transpose()
    {
        Ok(data) => data.unwrap_or_default().0,
        Err(err) => {
            error_loading_config = Some(err);
            Default::default()
        }
    };

    #[allow(deprecated)]
    if config.frame_buffer.minimum_framebuffer_height.is_none() {
        config.frame_buffer.minimum_framebuffer_height =
            kernel.config.frame_buffer.minimum_framebuffer_height;
    }
    #[allow(deprecated)]
    if config.frame_buffer.minimum_framebuffer_width.is_none() {
        config.frame_buffer.minimum_framebuffer_width =
            kernel.config.frame_buffer.minimum_framebuffer_width;
    }
    let framebuffer = init_logger(image, &st, &config);

    unsafe {
        *SYSTEM_TABLE.get() = None;
    }

    log::info!("UEFI bootloader started");

    if let Some(framebuffer) = framebuffer {
        log::info!("Using framebuffer at {:#x}", framebuffer.addr);
    }

    if let Some(err) = error_loading_config {
        log::warn!("Failed to deserialize the config file {:?}", err);
    } else {
        log::info!("Reading configuration from disk was successful");
    }

    log::info!("Trying to load ramdisk via {:?}", boot_mode);
    // Ramdisk must load from same source, or not at all.
    let ramdisk = load_ramdisk(image, &mut st, boot_mode);

    log::info!(
        "{}",
        match ramdisk {
            Some(_) => "Loaded ramdisk",
            None => "Ramdisk not found.",
        }
    );

    log::trace!("exiting boot services");
    let (system_table, mut memory_map) = st.exit_boot_services();

    memory_map.sort();

    let mut frame_allocator =
        LegacyFrameAllocator::new(memory_map.entries().copied().map(UefiMemoryDescriptor));

    let max_phys_addr = frame_allocator.max_phys_addr();
    let page_tables = create_page_tables(&mut frame_allocator, max_phys_addr, framebuffer.as_ref());
    let mut ramdisk_len = 0u64;
    let ramdisk_addr = if let Some(rd) = ramdisk {
        ramdisk_len = rd.len() as u64;
        Some(rd.as_ptr() as usize as u64)
    } else {
        None
    };
    let system_info = SystemInfo {
        framebuffer,
        rsdp_addr: {
            use uefi::table::cfg;
            let mut config_entries = system_table.config_table().iter();
            // look for an ACPI2 RSDP first
            let acpi2_rsdp = config_entries.find(|entry| matches!(entry.guid, cfg::ACPI2_GUID));
            // if no ACPI2 RSDP is found, look for a ACPI1 RSDP
            let rsdp = acpi2_rsdp
                .or_else(|| config_entries.find(|entry| matches!(entry.guid, cfg::ACPI_GUID)));
            rsdp.map(|entry| PhysAddr::new(entry.address as u64))
        },
        ramdisk_addr,
        ramdisk_len,
    };

    bootloader_x86_64_common::load_and_switch_to_kernel(
        kernel,
        config,
        frame_allocator,
        page_tables,
        system_info,
    );
}

#[derive(Clone, Copy, Debug)]
pub enum BootMode {
    Disk,
    Tftp,
}

// ── BOOT.LOG via soft-reboot (kernel → RAM @ 256MiB → UEFI SFS) ─────────────
// Deve bater com k_nano::boot_ramlog (PHYS + magics).
const BOOT_RAMLOG_PHYS: u64 = 0x1000_0000;
const MAGIC_NEED_FLUSH: u64 = u64::from_le_bytes(*b"NEURLOG!");
const MAGIC_FLUSHED: u64 = u64::from_le_bytes(*b"NEURDONE");
const BOOT_RAMLOG_HDR: usize = 16;
const BOOT_LOG_CAP: usize = 256 * 1024;

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

fn flush_boot_ramlog_if_needed(image: Handle, st: &mut SystemTable<Boot>) {
    // Cold boot apos HALT: se RLOG.OK existe no ESP, avisa kernel pular soft-reboot.
    if consume_rlog_ok(image, st) {
        unsafe {
            core::ptr::write_volatile(BOOT_RAMLOG_PHYS as *mut u64, MAGIC_FLUSHED);
            core::ptr::write_volatile((BOOT_RAMLOG_PHYS + 8) as *mut u32, 0u32);
            core::ptr::write_volatile((BOOT_RAMLOG_PHYS + 12) as *mut u32, 0u32);
        }
        let _ = writeln!(st.stdout(), "[RAMLOG] RLOG.OK — skip soft-reboot neste boot");
    }

    let magic = unsafe { core::ptr::read_volatile(BOOT_RAMLOG_PHYS as *const u64) };
    if magic != MAGIC_NEED_FLUSH {
        return;
    }
    let len = unsafe { core::ptr::read_volatile((BOOT_RAMLOG_PHYS + 8) as *const u32) } as usize;
    let crc_ckpt = unsafe { core::ptr::read_volatile((BOOT_RAMLOG_PHYS + 12) as *const u32) };
    let expect_crc = crc_ckpt & 0x00FF_FFFF;
    let last_k = (crc_ckpt >> 24) as u8;
    let data_cap = BOOT_LOG_CAP.saturating_sub(BOOT_RAMLOG_HDR);
    let len = len.min(data_cap);
    let data =
        unsafe { slice::from_raw_parts((BOOT_RAMLOG_PHYS + BOOT_RAMLOG_HDR as u64) as *const u8, len) };
    let got_crc = crc32_24(data);
    let ram_ok = len > 0 && got_crc == expect_crc;

    let _ = writeln!(
        st.stdout(),
        "[RAMLOG] magic OK last=K{} len={} crc_ok={} — gravando BOOT.LOG...",
        last_k,
        len,
        ram_ok
    );

    let mut payload = [0u8; BOOT_LOG_CAP];
    let n = if ram_ok {
        payload[..len].copy_from_slice(data);
        len
    } else {
        let stub = alloc_stub_msg(last_k);
        let mut real = stub.len();
        while real > 0 && stub[real - 1] == 0 {
            real -= 1;
        }
        payload[..real].copy_from_slice(&stub[..real]);
        real.max(64)
    };
    let write_slice = &payload[..n.min(BOOT_LOG_CAP)];

    let mut ok = false;
    if let Ok(handles) = st
        .boot_services()
        .locate_handle_buffer(SearchType::from_proto::<SimpleFileSystem>())
    {
        for &h in handles.handles() {
            if try_write_boot_log_on_sfs(image, st, h, write_slice) {
                ok = true;
            }
        }
    }
    if let Some(mut sfs) = locate_and_open_protocol::<SimpleFileSystem>(image, st) {
        if write_boot_log_volume(sfs.deref_mut(), write_slice) {
            ok = true;
        }
    }

    if ok {
        let _ = write_rlog_ok(image, st);
        let _ = writeln!(
            st.stdout(),
            "[RAMLOG] BOOT.LOG OK (K{}). DESLIGUE e leia E:\\BOOT.LOG — HALT.",
            last_k
        );
        unsafe {
            core::ptr::write_volatile(BOOT_RAMLOG_PHYS as *mut u64, MAGIC_FLUSHED);
        }
    } else {
        let _ = writeln!(
            st.stdout(),
            "[RAMLOG] FALHA SFS — last=K{} (tire foto desta tela)",
            last_k
        );
        unsafe {
            core::ptr::write_volatile(BOOT_RAMLOG_PHYS as *mut u64, 0u64);
        }
    }

    loop {
        let _ = writeln!(st.stdout(), "[RAMLOG] HALT — remova USB / leia BOOT.LOG");
        for _ in 0..50_000_000 {
            core::hint::spin_loop();
        }
    }
}

fn write_rlog_ok(image: Handle, st: &SystemTable<Boot>) -> bool {
    let Some(mut sfs) = locate_and_open_protocol::<SimpleFileSystem>(image, st) else {
        return false;
    };
    let Ok(mut root) = sfs.open_volume() else {
        return false;
    };
    let mut name_buf = [0u16; 16];
    let Ok(fname) = CStr16::from_str_with_buf("RLOG.OK", &mut name_buf) else {
        return false;
    };
    let Ok(fh) = root.open(fname, FileMode::CreateReadWrite, FileAttribute::empty()) else {
        return false;
    };
    let mut file = match fh.into_type() {
        Ok(uefi::proto::media::file::FileType::Regular(f)) => f,
        _ => return false,
    };
    let _ = file.write(b"1\n");
    let _ = file.flush();
    true
}

fn consume_rlog_ok(image: Handle, st: &SystemTable<Boot>) -> bool {
    let Some(mut sfs) = locate_and_open_protocol::<SimpleFileSystem>(image, st) else {
        return false;
    };
    let Ok(mut root) = sfs.open_volume() else {
        return false;
    };
    let mut name_buf = [0u16; 16];
    let Ok(fname) = CStr16::from_str_with_buf("RLOG.OK", &mut name_buf) else {
        return false;
    };
    let Ok(fh) = root.open(fname, FileMode::ReadWrite, FileAttribute::empty()) else {
        return false;
    };
    let _ = fh.delete();
    true
}

fn alloc_stub_msg(last_k: u8) -> [u8; 192] {
    let mut out = [0u8; 192];
    let msg = b"[S] neural-os-core BOOT.LOG\n# RAM limpa no reset (CRC fail)\n# ultimo ckpt no header: K";
    let mut i = 0;
    for &b in msg {
        out[i] = b;
        i += 1;
    }
    if last_k >= 100 {
        out[i] = b'0' + last_k / 100;
        i += 1;
    }
    if last_k >= 10 {
        out[i] = b'0' + (last_k / 10) % 10;
        i += 1;
    }
    out[i] = b'0' + last_k % 10;
    i += 1;
    out[i] = b'\n';
    let _ = i;
    out
}

fn try_write_boot_log_on_sfs(
    image: Handle,
    st: &SystemTable<Boot>,
    handle: Handle,
    data: &[u8],
) -> bool {
    let opened = unsafe {
        st.boot_services().open_protocol::<SimpleFileSystem>(
            OpenProtocolParams {
                handle,
                agent: image,
                controller: None,
            },
            OpenProtocolAttributes::GetProtocol,
        )
    };
    let Ok(mut sfs) = opened else {
        return false;
    };
    write_boot_log_volume(sfs.deref_mut(), data)
}

fn write_boot_log_volume(fs: &mut SimpleFileSystem, data: &[u8]) -> bool {
    let Ok(mut root) = fs.open_volume() else {
        return false;
    };
    let mut name_buf = [0u16; 16];
    let Ok(fname) = CStr16::from_str_with_buf("BOOT.LOG", &mut name_buf) else {
        return false;
    };
    // Preferir arquivo pré-alocado (ReadWrite); senão criar.
    let file_handle = root
        .open(fname, FileMode::ReadWrite, FileAttribute::empty())
        .or_else(|_| {
            root.open(
                fname,
                FileMode::CreateReadWrite,
                FileAttribute::empty(),
            )
        });
    let Ok(file_handle) = file_handle else {
        return false;
    };
    let mut file = match file_handle.into_type() {
        Ok(uefi::proto::media::file::FileType::Regular(f)) => f,
        _ => return false,
    };
    if file.set_position(0).is_err() {
        return false;
    }
    // Sobrescreve os 256 KiB pré-alocados (resto fica zero).
    let mut payload = [0u8; BOOT_LOG_CAP];
    let n = data.len().min(BOOT_LOG_CAP);
    payload[..n].copy_from_slice(&data[..n]);
    if file.write(&payload).is_err() {
        return false;
    }
    let _ = file.flush();
    true
}

fn load_ramdisk(
    image: Handle,
    st: &mut SystemTable<Boot>,
    boot_mode: BootMode,
) -> Option<&'static mut [u8]> {
    load_file_from_boot_method(image, st, "ramdisk\0", boot_mode)
}

fn load_config_file(
    image: Handle,
    st: &mut SystemTable<Boot>,
    boot_mode: BootMode,
) -> Option<&'static mut [u8]> {
    load_file_from_boot_method(image, st, "boot.json\0", boot_mode)
}

fn load_kernel(
    image: Handle,
    st: &mut SystemTable<Boot>,
    boot_mode: BootMode,
) -> Option<Kernel<'static>> {
    let kernel_slice = load_file_from_boot_method(image, st, "kernel-x86_64\0", boot_mode)?;
    Some(Kernel::parse(kernel_slice))
}

fn load_file_from_boot_method(
    image: Handle,
    st: &mut SystemTable<Boot>,
    filename: &str,
    boot_mode: BootMode,
) -> Option<&'static mut [u8]> {
    match boot_mode {
        BootMode::Disk => load_file_from_disk(filename, image, st),
        BootMode::Tftp => load_file_from_tftp_boot_server(filename, image, st),
    }
}

fn open_device_path_protocol(
    image: Handle,
    st: &SystemTable<Boot>,
) -> Option<ScopedProtocol<DevicePath>> {
    let this = st.boot_services();
    let loaded_image = unsafe {
        this.open_protocol::<LoadedImage>(
            OpenProtocolParams {
                handle: image,
                agent: image,
                controller: None,
            },
            OpenProtocolAttributes::Exclusive,
        )
    };

    if loaded_image.is_err() {
        log::error!("Failed to open protocol LoadedImage");
        return None;
    }
    let loaded_image = loaded_image.unwrap();
    let loaded_image = loaded_image.deref();

    let device_handle = loaded_image.device();

    let device_path = unsafe {
        this.open_protocol::<DevicePath>(
            OpenProtocolParams {
                handle: device_handle,
                agent: image,
                controller: None,
            },
            OpenProtocolAttributes::Exclusive,
        )
    };
    if device_path.is_err() {
        log::error!("Failed to open protocol DevicePath");
        return None;
    }
    Some(device_path.unwrap())
}

fn locate_and_open_protocol<P: ProtocolPointer>(
    image: Handle,
    st: &SystemTable<Boot>,
) -> Option<ScopedProtocol<P>> {
    let this = st.boot_services();
    let device_path = open_device_path_protocol(image, st)?;
    let mut device_path = device_path.deref();

    let fs_handle = this.locate_device_path::<P>(&mut device_path);
    if fs_handle.is_err() {
        log::error!("Failed to open device path");
        return None;
    }

    let fs_handle = fs_handle.unwrap();

    let opened_handle = unsafe {
        this.open_protocol::<P>(
            OpenProtocolParams {
                handle: fs_handle,
                agent: image,
                controller: None,
            },
            OpenProtocolAttributes::Exclusive,
        )
    };

    if opened_handle.is_err() {
        log::error!("Failed to open protocol {}", core::any::type_name::<P>());
        return None;
    }
    Some(opened_handle.unwrap())
}

fn load_file_from_disk(
    name: &str,
    image: Handle,
    st: &SystemTable<Boot>,
) -> Option<&'static mut [u8]> {
    let mut file_system_raw = locate_and_open_protocol::<SimpleFileSystem>(image, st)?;
    let file_system = file_system_raw.deref_mut();

    let mut root = file_system.open_volume().unwrap();
    let mut buf = [0u16; 256];
    assert!(name.len() < 256);
    let filename = CStr16::from_str_with_buf(name.trim_end_matches('\0'), &mut buf)
        .expect("Failed to convert string to utf16");

    let file_handle_result = root.open(filename, FileMode::Read, FileAttribute::empty());

    let file_handle = file_handle_result.ok()?;

    let mut file = match file_handle.into_type().unwrap() {
        uefi::proto::media::file::FileType::Regular(f) => f,
        uefi::proto::media::file::FileType::Dir(_) => panic!(),
    };

    let mut buf = [0; 500];
    let file_info: &mut FileInfo = file.get_info(&mut buf).unwrap();
    let file_size = usize::try_from(file_info.file_size()).unwrap();

    let file_ptr = st
        .boot_services()
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            ((file_size - 1) / 4096) + 1,
        )
        .unwrap() as *mut u8;
    unsafe { ptr::write_bytes(file_ptr, 0, file_size) };
    let file_slice = unsafe { slice::from_raw_parts_mut(file_ptr, file_size) };
    file.read(file_slice).unwrap();

    Some(file_slice)
}

/// Try to load a kernel from a TFTP boot server.
fn load_file_from_tftp_boot_server(
    name: &str,
    image: Handle,
    st: &SystemTable<Boot>,
) -> Option<&'static mut [u8]> {
    let mut base_code_raw = locate_and_open_protocol::<BaseCode>(image, st)?;
    let base_code = base_code_raw.deref_mut();

    // Find the TFTP boot server.
    let mode = base_code.mode();
    assert!(mode.dhcp_ack_received);
    let dhcpv4: &DhcpV4Packet = mode.dhcp_ack.as_ref();
    let server_ip = IpAddress::new_v4(dhcpv4.bootp_si_addr);
    assert!(name.len() < 256);

    let filename = CStr8::from_bytes_with_nul(name.as_bytes()).unwrap();

    // Determine the kernel file size.
    let file_size = base_code.tftp_get_file_size(&server_ip, filename).ok()?;
    let kernel_size = usize::try_from(file_size).expect("The file size should fit into usize");

    // Allocate some memory for the kernel file.
    let ptr = st
        .boot_services()
        .allocate_pages(
            AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            ((kernel_size - 1) / 4096) + 1,
        )
        .expect("Failed to allocate memory for the file") as *mut u8;
    let slice = unsafe { slice::from_raw_parts_mut(ptr, kernel_size) };

    // Load the kernel file.
    base_code
        .tftp_read_file(&server_ip, filename, Some(slice))
        .expect("Failed to read kernel file from the TFTP boot server");

    Some(slice)
}

/// Creates page table abstraction types for both the bootloader and kernel page tables.
fn create_page_tables(
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    max_phys_addr: PhysAddr,
    frame_buffer: Option<&RawFrameBufferInfo>,
) -> bootloader_x86_64_common::PageTables {
    // UEFI identity-maps all memory, so the offset between physical and virtual addresses is 0
    let phys_offset = VirtAddr::new(0);

    // copy the currently active level 4 page table, because it might be read-only
    log::trace!("switching to new level 4 table");
    let bootloader_page_table = {
        let old_table = {
            let frame = x86_64::registers::control::Cr3::read().0;
            let ptr: *const PageTable = (phys_offset + frame.start_address().as_u64()).as_ptr();
            unsafe { &*ptr }
        };
        let new_frame = frame_allocator
            .allocate_frame()
            .expect("Failed to allocate frame for new level 4 table");
        let new_table: &mut PageTable = {
            let ptr: *mut PageTable =
                (phys_offset + new_frame.start_address().as_u64()).as_mut_ptr();
            // create a new, empty page table
            unsafe {
                ptr.write(PageTable::new());
                &mut *ptr
            }
        };

        // copy the pml4 entries for all identity mapped memory.
        let end_addr = VirtAddr::new(max_phys_addr.as_u64() - 1);
        for p4 in 0..=usize::from(end_addr.p4_index()) {
            new_table[p4] = old_table[p4].clone();
        }

        // copy the pml4 entry for the frame buffer (the frame buffer is not
        // necessarily part of the identity mapping).
        if let Some(frame_buffer) = frame_buffer {
            let start_addr = VirtAddr::new(frame_buffer.addr.as_u64());
            let end_addr = start_addr + frame_buffer.info.byte_len as u64;
            for p4 in usize::from(start_addr.p4_index())..=usize::from(end_addr.p4_index()) {
                new_table[p4] = old_table[p4].clone();
            }
        }

        // the first level 4 table entry is now identical, so we can just load the new one
        unsafe {
            x86_64::registers::control::Cr3::write(
                new_frame,
                x86_64::registers::control::Cr3Flags::empty(),
            );
            OffsetPageTable::new(&mut *new_table, phys_offset)
        }
    };

    // create a new page table hierarchy for the kernel
    let (kernel_page_table, kernel_level_4_frame) = {
        // get an unused frame for new level 4 page table
        let frame: PhysFrame = frame_allocator.allocate_frame().expect("no unused frames");
        log::info!("New page table at: {:#?}", &frame);
        // get the corresponding virtual address
        let addr = phys_offset + frame.start_address().as_u64();
        // initialize a new page table
        let ptr = addr.as_mut_ptr();
        unsafe { *ptr = PageTable::new() };
        let level_4_table = unsafe { &mut *ptr };
        (
            unsafe { OffsetPageTable::new(level_4_table, phys_offset) },
            frame,
        )
    };

    bootloader_x86_64_common::PageTables {
        bootloader: bootloader_page_table,
        kernel: kernel_page_table,
        kernel_level_4_frame,
    }
}

fn init_logger(
    image_handle: Handle,
    st: &SystemTable<Boot>,
    config: &BootConfig,
) -> Option<RawFrameBufferInfo> {
    let gop_handle = st
        .boot_services()
        .get_handle_for_protocol::<GraphicsOutput>()
        .ok()?;
    let mut gop = unsafe {
        st.boot_services()
            .open_protocol::<GraphicsOutput>(
                OpenProtocolParams {
                    handle: gop_handle,
                    agent: image_handle,
                    controller: None,
                },
                OpenProtocolAttributes::Exclusive,
            )
            .ok()?
    };

    // Intel HD/UHD (ex.: 620) costuma iniciar em PixelFormat::BltOnly — sem FB linear.
    // Cascata: [QEMU~1280] → EDID → teto 1080p → uncapped (sem panic).
    let min_h = config
        .frame_buffer
        .minimum_framebuffer_height
        .map(|v| usize::try_from(v).unwrap());
    let min_w = config
        .frame_buffer
        .minimum_framebuffer_width
        .map(|v| usize::try_from(v).unwrap());

    let qemu = fb_pick::detect_qemu(st);
    let edid = fb_pick::read_edid_preferred(image_handle, st, gop_handle);
    let picked = fb_pick::pick_gop_mode(&gop, edid, min_w, min_h, qemu);
    let Some(picked) = picked else {
        // Sem modo linear: kernel sobe sem FB (VGA text / serial).
        return None;
    };
    let pick_reason = picked.reason;
    let pick_edid = picked.edid;
    if gop.set_mode(&picked.mode).is_err() {
        return None;
    }

    let mode_info = gop.current_mode_info();
    match mode_info.pixel_format() {
        PixelFormat::Rgb | PixelFormat::Bgr => {}
        PixelFormat::Bitmask | PixelFormat::BltOnly => {
            return None;
        }
    }

    let mut framebuffer = gop.frame_buffer();
    let slice = unsafe { slice::from_raw_parts_mut(framebuffer.as_mut_ptr(), framebuffer.size()) };
    let info = FrameBufferInfo {
        byte_len: framebuffer.size(),
        width: mode_info.resolution().0,
        height: mode_info.resolution().1,
        pixel_format: match mode_info.pixel_format() {
            PixelFormat::Rgb => bootloader_api::info::PixelFormat::Rgb,
            PixelFormat::Bgr => bootloader_api::info::PixelFormat::Bgr,
            PixelFormat::Bitmask | PixelFormat::BltOnly => {
                // Inatingivel apos o match acima; mantido por exaustividade.
                return None;
            }
        },
        bytes_per_pixel: 4,
        stride: mode_info.stride(),
    };

    bootloader_x86_64_common::init_logger(
        slice,
        info,
        config.log_level,
        config.frame_buffer_logging,
        config.serial_logging,
    );

    let (mw, mh) = mode_info.resolution();
    let (cap_w, cap_h) = if qemu {
        (fb_pick::QEMU_CAP_W, fb_pick::QEMU_CAP_H)
    } else {
        (fb_pick::CAP_W, fb_pick::CAP_H)
    };
    match pick_edid {
        Some(e) => log::info!(
            "FB pick reason={} mode={}x{} edid={}x{}@{}Hz cap={}x{} qemu={}",
            pick_reason,
            mw,
            mh,
            e.width,
            e.height,
            e.hz,
            cap_w,
            cap_h,
            qemu
        ),
        None => log::info!(
            "FB pick reason={} mode={}x{} edid=none cap={}x{} qemu={}",
            pick_reason,
            mw,
            mh,
            cap_w,
            cap_h,
            qemu
        ),
    }

    Some(RawFrameBufferInfo {
        addr: PhysAddr::new(framebuffer.as_mut_ptr() as u64),
        info,
    })
}

#[cfg(target_os = "uefi")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::arch::asm;
    use core::fmt::Write;

    if let Some(st) = unsafe { &mut *SYSTEM_TABLE.get() } {
        let _ = st.stdout().clear();
        let _ = writeln!(st.stdout(), "{}", info);
    }

    unsafe {
        bootloader_x86_64_common::logger::LOGGER
            .get()
            .map(|l| l.force_unlock())
    };
    log::error!("{}", info);

    loop {
        unsafe { asm!("cli; hlt") };
    }
}
