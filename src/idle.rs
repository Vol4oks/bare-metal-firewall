use crate::scheduler;

pub extern "C" fn task() -> ! {
    loop {
        scheduler::yield_now(); // drain до сна
        unsafe { core::arch::asm!("wfi") };
    }
}
