use crate::{println, scheduler, timer};

pub extern "C" fn task() -> ! {
    loop {
        unsafe { core::arch::asm!("wfi") };
        println!("ticks = {}", timer::ticks());
        scheduler::yield_now(); // задел на 0.4: отдать CPU, если кто-то стал Ready
    }
}
