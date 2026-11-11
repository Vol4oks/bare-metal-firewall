use crate::{events, scheduler, timer};

const STACK_SIZE: usize = 8 * 1024;

#[allow(dead_code)] // поле читается только через адрес в stack_top()
#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

static mut STACK: Stack = Stack([0; STACK_SIZE]);

pub fn stack_top() -> usize {
    core::ptr::addr_of_mut!(STACK) as usize + core::mem::size_of::<Stack>()
}

pub extern "C" fn task() -> ! {
    let mut last = timer::ticks();

    loop {
        scheduler::wait_current(events::TICK);

        let now = timer::ticks();
        let delta = now - last;
        last = now;

        crate::println!("ticks = {} (+{})", now, delta);
    }
}
