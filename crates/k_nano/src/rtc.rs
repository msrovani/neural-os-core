//! RTC — CMOS Real-Time Clock driver (MC146818).
//! Acesso via ports 0x70 (index) e 0x71 (data).
//!
//! Leitura segura com loop wait-snapshot-verify contra RTC update in progress.

use core::arch::asm;
use core::fmt;

/// Data/hora lida do RTC CMOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtcDateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl fmt::Display for RtcDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Lê um byte do CMOS. Escrita/leitura num único bloco asm para atomicidade.
fn cmos_read(reg: u8) -> u8 {
    let mut val: u8 = reg;
    unsafe {
        asm!(
            "out 0x70, al",
            "in al, 0x71",
            inout("al") val,
            options(nomem, nostack),
        );
    }
    val
}

/// Converte BCD para binário.
fn bcd_to_bin(bcd: u8) -> u8 {
    (bcd & 0x0f) + ((bcd >> 4) * 10)
}

/// Lê data/hora atual do RTC CMOS com loop de consistência.
///
/// Estratégia wait-snapshot-verify: espera UIP=0, captura todos os registros,
/// verifica se o RTC iniciou um novo update durante a captura.
pub fn read_rtc() -> RtcDateTime {
    let (second, minute, hour, day, month, year, century, status_b) = loop {
        // 1. Aguarda fim de qualquer update em progresso (UIP bit 7 do Reg A)
        while (cmos_read(0x0A) & 0x80) != 0 {}

        // 2. Snapshot — leitura de todos os registros de uma vez
        let second = cmos_read(0x00);
        let minute = cmos_read(0x02);
        let hour = cmos_read(0x04);
        let day = cmos_read(0x07);
        let month = cmos_read(0x08);
        let year = cmos_read(0x09);
        let century = cmos_read(0x32);
        let status_b = cmos_read(0x0B);

        // 3. Verifica se um novo update começou durante a captura
        if (cmos_read(0x0A) & 0x80) == 0 {
            break (second, minute, hour, day, month, year, century, status_b);
        }
        // senão, repete
    };

    let is_bcd = (status_b & 0x04) == 0;

    let sec = if is_bcd { bcd_to_bin(second) } else { second };
    let min = if is_bcd { bcd_to_bin(minute) } else { minute };
    let hr = if is_bcd { bcd_to_bin(hour) } else { hour };
    let d = if is_bcd { bcd_to_bin(day) } else { day };
    let mon = if is_bcd { bcd_to_bin(month) } else { month };
    let yr = if is_bcd { bcd_to_bin(year) } else { year };
    let cent = if is_bcd && century != 0 {
        bcd_to_bin(century)
    } else if century != 0 {
        century
    } else {
        20
    };

    let full_year = if cent > 0 {
        (cent as u16) * 100 + yr as u16
    } else if yr > 80 {
        1900u16 + yr as u16
    } else {
        2000u16 + yr as u16
    };

    RtcDateTime {
        year: full_year,
        month: mon,
        day: d,
        hour: hr,
        minute: min,
        second: sec,
    }
}

/// Formata a data/hora para string (alloc).
pub fn format_rtc(dt: &RtcDateTime) -> alloc::string::String {
    alloc::format!("{}", dt)
}

/// Self-test básico.
pub fn demo() -> bool {
    let dt = read_rtc();
    // Ano deve ser razoável (2020-2100)
    dt.year >= 2020
        && dt.year <= 2100
        && dt.month >= 1
        && dt.month <= 12
        && dt.day >= 1
        && dt.day <= 31
        && dt.hour <= 23
        && dt.minute <= 59
        && dt.second <= 59
}
