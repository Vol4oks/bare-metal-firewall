use core::arch::global_asm;

use crate::{gic, timer};

global_asm!(include_str!("irq_vectors.s"));

#[unsafe(no_mangle)]
extern "C" fn unexpected() -> ! {
    let _ = crate::uart::UART.try_print_args(format_args!("Unexpected exception!\n"));
    loop {
        core::hint::spin_loop();
    }
}

unsafe extern "C" {
    static irq_vectors: u8;
}

pub fn init() -> usize {
    let table = &raw const irq_vectors as usize;
    assert!(table.is_multiple_of(0x800));

    let readback: usize;
    unsafe {
        core::arch::asm!(
            "msr vbar_el1, {v}", // пишем базу таблицы
            "isb",
            "mrs {r}, vbar_el1", // читаем обратно
            v = in(reg) table, // вход: адрес таблицы
            r = out(reg) readback, // выход: значение из регистра
        );
    }

    readback
}

#[unsafe(no_mangle)]
extern "C" fn sync_handler(esr: usize, far: usize, elr: usize) -> ! {
    let ec = (esr >> 26) & 0x3f; // EC: класс исключения
    let iss = esr & 0x1FF_FFFF; // ISS: детали
    panic!(
        "SYNC: EC={:#x} ISS={:#x} FAR={:#x} ELR={:#x}",
        ec, iss, far, elr
    );
}

#[unsafe(no_mangle)]
extern "C" fn irq_handler() {
    let iar = unsafe { gic::GIC.ack() };
    let intid = (iar & 0xFFFFFF) as u32;

    if intid == gic::SPURIOUS_INTID {
        return; // EOI не пишем: прерывания не было
    }

    if intid == timer::INTID {
        unsafe { timer::on_irq() };
    }

    // потом EOI
    unsafe { gic::GIC.eoi(iar) };
}
