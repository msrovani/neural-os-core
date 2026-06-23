pub mod agent;
pub mod executor;

use core::task::{RawWaker, RawWakerVTable, Waker};

const DUMMY_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |_| dummy_raw_waker(),
    |_| {},
    |_| {},
    |_| {},
);

fn dummy_raw_waker() -> RawWaker {
    RawWaker::new(core::ptr::null::<()>(), &DUMMY_VTABLE)
}

pub fn dummy_waker() -> Waker {
    unsafe { Waker::from_raw(dummy_raw_waker()) }
}
