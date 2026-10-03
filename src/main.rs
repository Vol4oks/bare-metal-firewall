#![no_std] // Не используем стандартную библиотеку
#![no_main] // Не используем стандартную точку входа

extern crate alloc;

use alloc::boxed::Box;
use core::arch::global_asm;
use core::panic::PanicInfo;

mod heap;
mod mmu;
mod uart;

// Подключаем ассемблерный файл
global_asm!(include_str!("start.s"));

unsafe extern "C" {
    static __bss_start: u8;
    static __bss_end: u8;
    static stack_top: u8;
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    println!("Hello from QEMU virt!");
    let addr1 = &raw const __bss_start as usize;
    println!("__bss_start: {:#018x}", addr1);
    let addr2 = &raw const __bss_end as usize;
    println!("__bss_end:   {:#018x}", addr2);
    let addr3 = &raw const stack_top as usize;
    println!("stack_top:   {:#018x}", addr3);
    println!("bss size: {}", addr2 - addr1);
    println!("stack gap: {:#x}", addr3 - addr2);

    println!("__heap_start: {:#018x}", heap::heap_start());
    println!("__heap_end: {:#018x}", heap::heap_end());
    unsafe {
        mmu::init_identity_map();
    }

    unsafe {
        mmu::enable(mmu::l0_pa());
    }
    println!("MMU on!");
    heap::init();
    let b = Box::new(24u64);
    println!("heap ptr: {:#018x}, val: {}", &*b as *const u64 as usize, b);

    loop {} // Бесконечный цикл
}

// Обязательный обработчик паники для no_std
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(location) = info.location() {
        println!("PANIC: {} at {}", info.message(), location,);
    } else {
        println!("PANIC: {}", info.message());
    };
    loop {
        core::hint::spin_loop();
    }
}
