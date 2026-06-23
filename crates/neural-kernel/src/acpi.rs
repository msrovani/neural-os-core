use crate::{println, serial_println};
use core::ptr::read_volatile;
use x86_64::VirtAddr;

#[repr(C, packed)]
struct RsdpDescriptor {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct AcpiInfo {
    pub lapic_base: u64,
    pub ioapic_base: u64,
    pub lapic_count: u8,
    pub ioapic_count: u8,
    pub has_x2apic: bool,
    pub phys_mem_offset: u64,
}

fn checksum_valid(data: &[u8]) -> bool {
    data.iter().fold(0u8, |a, b| a.wrapping_add(*b)) == 0
}

unsafe fn find_rsdp(physical_memory_offset: u64) -> Option<u64> {
    let ebda_start = VirtAddr::new(physical_memory_offset + 0x0008_0000);
    let ebda_end = VirtAddr::new(physical_memory_offset + 0x000A_0000);
    let bios_start = VirtAddr::new(physical_memory_offset + 0x000E_0000);
    let bios_end = VirtAddr::new(physical_memory_offset + 0x0010_0000);

    let mut addr = ebda_start.as_u64();
    while addr < ebda_end.as_u64() {
        let ptr = addr as *const u8;
        if read_volatile(ptr.add(0)) == b'R'
            && read_volatile(ptr.add(1)) == b'S'
            && read_volatile(ptr.add(2)) == b'D'
            && read_volatile(ptr.add(3)) == b' '
            && read_volatile(ptr.add(4)) == b'P'
            && read_volatile(ptr.add(5)) == b'T'
            && read_volatile(ptr.add(6)) == b'R'
            && read_volatile(ptr.add(7)) == b' '
        {
            let rsdp = &*(addr as *const RsdpDescriptor);
            let len = if rsdp.revision >= 2 { 36usize } else { 20usize };
            let raw = core::slice::from_raw_parts(addr as *const u8, len);
            if checksum_valid(raw) {
                return Some(addr - physical_memory_offset);
            }
        }
        addr += 16;
    }

    addr = bios_start.as_u64();
    while addr < bios_end.as_u64() {
        let ptr = addr as *const u8;
        if read_volatile(ptr.add(0)) == b'R'
            && read_volatile(ptr.add(1)) == b'S'
            && read_volatile(ptr.add(2)) == b'D'
            && read_volatile(ptr.add(3)) == b' '
            && read_volatile(ptr.add(4)) == b'P'
            && read_volatile(ptr.add(5)) == b'T'
            && read_volatile(ptr.add(6)) == b'R'
            && read_volatile(ptr.add(7)) == b' '
        {
            let rsdp = &*(addr as *const RsdpDescriptor);
            let len = if rsdp.revision >= 2 { 36usize } else { 20usize };
            let raw = core::slice::from_raw_parts(addr as *const u8, len);
            if checksum_valid(raw) {
                return Some(addr - physical_memory_offset);
            }
        }
        addr += 16;
    }
    None
}

pub unsafe fn init_acpi(physical_memory_offset: u64) -> Option<AcpiInfo> {
    let rsdp_phys = find_rsdp(physical_memory_offset)?;
    let rsdp_virt = VirtAddr::new(physical_memory_offset + rsdp_phys);
    let rsdp = &*(rsdp_virt.as_u64() as *const RsdpDescriptor);

    let revision = rsdp.revision;
    let rsdt_phys = if revision >= 2 && rsdp.xsdt_address != 0 {
        rsdp.xsdt_address
    } else {
        rsdp.rsdt_address as u64
    };

    serial_println!(
        "[ACPI] RSDP encontrado em 0x{:x}. Revisao: {}. RSDT/XSDT em 0x{:x}",
        rsdp_phys, revision, rsdt_phys
    );
    println!(
        "[ACPI] RSDP encontrado. Revisao: {}. RSDT em 0x{:x}",
        revision, rsdt_phys
    );

    let rsdt_virt = VirtAddr::new(physical_memory_offset + rsdt_phys);
    let rsdt_ptr = rsdt_virt.as_u64() as *const u32;
    let rsdt_signature_ptr = rsdt_virt.as_u64() as *const u8;

    let mut sig = [0u8; 4];
    for i in 0..4 {
        sig[i] = read_volatile(rsdt_signature_ptr.add(i));
    }

    if &sig != b"RSDT" && &sig != b"XSDT" {
        serial_println!("[ACPI] Assinatura invalida: {:?}", core::str::from_utf8(&sig));
        println!("[ACPI] Assinatura invalida: {:?}", core::str::from_utf8(&sig));
        return None;
    }

    let rsdt_len_raw = rsdt_virt.as_u64() as *const u32;
    let rsdt_len = read_volatile(rsdt_len_raw.add(1)) as usize;
    let entry_count = (rsdt_len - 36) / 4;

    serial_println!("[ACPI] Tabela RSDT/XSDT: {} bytes, {} entradas.", rsdt_len, entry_count);
    println!("[ACPI] Tabela: {} bytes, {} entradas.", rsdt_len, entry_count);

    let mut lapic_base = 0xFEE0_0000u64;
    let mut ioapic_base = 0xFEC0_0000u64;
    let mut lapic_count = 0u8;
    let mut ioapic_count = 0u8;
    let mut has_x2apic = false;

    for i in 0..entry_count {
        let entry_ptr = rsdt_ptr.add(2 + i);
        let table_phys = read_volatile(entry_ptr) as u64;
        let table_virt = VirtAddr::new(physical_memory_offset + table_phys);
        let table_ptr = table_virt.as_u64() as *const u8;

        let mut table_sig = [0u8; 4];
        for j in 0..4 {
            table_sig[j] = read_volatile(table_ptr.add(j));
        }

        match &table_sig {
            b"APIC" => {
                serial_println!("[ACPI] MADT encontrado em 0x{:x}", table_phys);
                let madt_len_raw = table_ptr.add(4) as *const u32;
                let madt_len = read_volatile(madt_len_raw) as usize;
                let madt_lapic_addr_raw = table_ptr.add(0x24) as *const u32;
                let madt_lapic_addr = read_volatile(madt_lapic_addr_raw) as u64;
                if madt_lapic_addr != 0 {
                    lapic_base = madt_lapic_addr;
                }

                let mut offset = 0x2Cu32 as usize;
                while offset < madt_len {
                    let entry_type_ptr = table_ptr.add(offset) as *const u8;
                    let entry_len_ptr = table_ptr.add(offset + 1) as *const u8;
                    let entry_type = read_volatile(entry_type_ptr);
                    let entry_len = read_volatile(entry_len_ptr) as usize;

                    match entry_type {
                        0 => {
                            lapic_count += 1;
                        }
                        1 => {
                            let ioapic_id = read_volatile(table_ptr.add(offset + 2) as *const u8);
                            let ioapic_addr_raw2 = table_ptr.add(offset + 4) as *const u32;
                            let ioapic_addr = read_volatile(ioapic_addr_raw2) as u64;
                            ioapic_base = ioapic_addr;
                            ioapic_count += 1;
                            serial_println!(
                                "[ACPI] IOAPIC ID {} em 0x{:x}",
                                ioapic_id, ioapic_addr
                            );
                        }
                        2 => {
                            has_x2apic = true;
                        }
                        5 => {
                            lapic_count += 1;
                        }
                        _ => {}
                    }
                    offset += entry_len;
                }

                serial_println!(
                    "[ACPI] MADT: LAPIC base 0x{:x}, IOAPIC base 0x{:x}, LAPICs: {}, IOAPICs: {}",
                    lapic_base, ioapic_base, lapic_count, ioapic_count
                );
                println!(
                    "[ACPI] MADT: LAPICs: {}, IOAPICs: {}",
                    lapic_count, ioapic_count
                );
            }
            _ => {}
        }
    }

    Some(AcpiInfo {
        lapic_base,
        ioapic_base,
        lapic_count,
        ioapic_count,
        has_x2apic,
        phys_mem_offset: physical_memory_offset,
    })
}
