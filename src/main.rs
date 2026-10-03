#![no_std] // Не используем стандартную библиотеку
#![no_main] // Не используем стандартную точку входа

use core::arch::global_asm;
use core::panic::PanicInfo;

mod uart;

// Подключаем ассемблерный файл
global_asm!(include_str!("start.s"));

#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    println!("Hello from QEMU virt!");
    // panic!("test panic");
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
