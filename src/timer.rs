use core::sync::atomic::{AtomicU32, Ordering};

/// INTID виртуального таймера: PPI 27
pub const INTID: u32 = 27;
/// Приоритет PPI в GIC
pub const PRIORITY: u8 = 0xA0;

// Сколько прерываний в секунду хотим.
const HZ: u64 = 4;

// Кэш `CNTFRQ_EL0 / HZ`: пишется в init, читается в ISR.
static TVAL: AtomicU32 = AtomicU32::new(0);

// Счётчик срабатываний для main.
static TICKS: AtomicU32 = AtomicU32::new(0);

fn cntfrq() -> u64 {
    let v;
    unsafe { core::arch::asm!("mrs {}, cntfrq_el0", out(reg) v) };
    v
}

/// Запускает виртуальный таймер.
///
/// # Safety
///
/// - `CNTFRQ_EL0 != 0`.
/// - PPI 27 уже разрешён в GIC.
/// - Прерывания на CPU ещё замаскированы (до `daifclr`).
pub unsafe fn init() {
    let freq = cntfrq();
    assert!(freq != 0, "CNTFRQ_EL0 = 0");

    let tval = u32::try_from(freq / HZ).expect("TVAL does not fit u32");
    assert!(tval != 0, "TVAL = 0");
    TVAL.store(tval, Ordering::Relaxed);

    unsafe {
        core::arch::asm!(
            "msr cntv_tval_el0, {t}",
            "isb",
            "msr cntv_ctl_el0, {en}",
            "isb",
            t = in(reg) tval as u64,
            en = in(reg) 1u64, // ENABLE=1, IMASK=0
        );
    }
}

/// Вызывается из IRQ-обработчика при `intid == INTID`.
/// Гасит источник и увеличивает счётчик. EOI делает вызывающий.
///
/// # Safety
///
/// Только из IRQ-контекста, после `init()`.
pub unsafe fn on_irq() {
    let tval = TVAL.load(Ordering::Relaxed);
    unsafe {
        core::arch::asm!(
            "msr cntv_tval_el0, {t}",
            "isb",
            t = in(reg) tval as u64,
        );
    }
    TICKS.fetch_add(1, Ordering::Relaxed);
}

pub fn ticks() -> u32 {
    TICKS.load(Ordering::Relaxed)
}
