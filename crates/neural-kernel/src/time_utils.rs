use alloc::format;

pub fn datetime(unix_secs: u64) -> alloc::string::String {
    let days = unix_secs / 86400;
    let rem = unix_secs % 86400;
    let h = (rem / 3600) as u8;
    let m = ((rem % 3600) / 60) as u8;
    let s = (rem % 60) as u8;

    let mut d = days;
    let mut year = 1970u64;
    loop {
        let yr = if year % 400 == 0 || (year % 4 == 0 && year % 100 != 0) { 366 } else { 365 };
        if d < yr { break; }
        d -= yr;
        year += 1;
    }
    let leap = year % 400 == 0 || (year % 4 == 0 && year % 100 != 0);
    let month_days: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u8;
    for &md in &month_days {
        if d < md { break; }
        d -= md;
        month += 1;
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year as u16, month, (d + 1) as u8, h, m, s)
}
