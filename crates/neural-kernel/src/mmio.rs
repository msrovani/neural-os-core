//! Typed MMIO registers — Tock-inspired.
//! Substitui unsafe write_volatile/read_volatile por Register<T>.
use core::ptr::{read_volatile, write_volatile};

#[repr(transparent)]
pub struct Register<T: Copy> { ptr: *mut T }

impl<T: Copy> Register<T> {
    pub unsafe fn new(addr: *mut T) -> Self { Register { ptr: addr } }
    pub unsafe fn read(&self) -> T { read_volatile(self.ptr) }
    pub unsafe fn write(&self, val: T) { write_volatile(self.ptr, val); }
    pub fn as_ptr(&self) -> *mut T { self.ptr }
}

pub struct RegisterArray<T: Copy, const N: usize> { base: *mut T }

impl<T: Copy, const N: usize> RegisterArray<T, N> {
    pub unsafe fn new(base: *mut T) -> Self { RegisterArray { base } }
    pub unsafe fn get(&self, idx: usize) -> Register<T> {
        assert!(idx < N);
        Register::new(self.base.add(idx))
    }
}

pub struct RegisterField<const OFFSET: u8, const WIDTH: u8>;

impl<const OFFSET: u8, const WIDTH: u8> RegisterField<OFFSET, WIDTH> {
    pub fn read(&self, reg: u32) -> u32 { (reg >> OFFSET) & ((1 << WIDTH) - 1) }
    pub fn write(&self, reg: &mut u32, val: u32) {
        let mask = (1 << WIDTH) - 1;
        *reg = (*reg & !(mask << OFFSET)) | ((val & mask) << OFFSET);
    }
}

// Exemplo de uso (descomente para testar):
// let vendor = Register::<u16>::new(mmio_base as *mut u16);
// println!("Vendor: {:#x}", vendor.read());

