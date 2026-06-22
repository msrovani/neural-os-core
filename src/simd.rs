use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};

pub fn enable_simd() {
    unsafe {
        Cr0::update(|flags| {
            flags.remove(Cr0Flags::EMULATE_COPROCESSOR);
            flags.insert(Cr0Flags::MONITOR_COPROCESSOR);
            flags.insert(Cr0Flags::NUMERIC_ERROR);
        });
    }
    unsafe {
        Cr4::update(|flags| {
            flags.insert(Cr4Flags::OSFXSR);
            flags.insert(Cr4Flags::OSXMMEXCPT_ENABLE);
        });
    }
}
