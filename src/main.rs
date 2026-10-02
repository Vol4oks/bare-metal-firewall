#![no_std] // Не используем стандартную библиотеку
#![no_main] // Не используем стандартную точку входа

use core::arch::global_asm;
use core::panic::PanicInfo;

// Подключаем ассемблерный файл
global_asm!(include_str!("start.s"));

// Адрес UART0 (PL011) в QEMU virt
const UART0: *mut u8 = 0x0900_0000 as *mut u8;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    let msg = b"Hello from QEMU virt!\n";
    for &byte in msg {
        unsafe {
            core::ptr::write_volatile(UART0, byte);
        }
    }
    loop {} // Бесконечный цикл
}

// Обязательный обработчик паники для no_std
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
