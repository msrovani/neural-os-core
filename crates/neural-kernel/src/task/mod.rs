pub mod agent;
pub mod executor;

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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

pub async fn yield_now() {
    struct YieldNow(bool);
    impl Future for YieldNow {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
    YieldNow(false).await
}
