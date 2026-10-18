use core::arch::global_asm;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::println;

static TICKS: AtomicU32 = AtomicU32::new(0);

pub fn ticks() -> u32 {
    TICKS.load(Ordering::Relaxed)
}

global_asm!(include_str!("irq_vectors.s"));

#[unsafe(no_mangle)]
extern "C" fn unexpected() -> ! {
    println!("Unexpected exception!");
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

    let readbeck: usize;
    unsafe {
        core::arch::asm!(
            "msr vbar_el1, {v}", // пишем базу таблицы
            "isb",
            "mrs {r}, vbar_el1", // читаем обратно
            v = in(reg) table, // вход: адрес таблицы
            r = out(reg) readbeck, // выход: значение из регистра
        );
    }

    readbeck
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

// GICD
const GICD_BASE: usize = 0x0800_0000;
const GICD_CTLR: usize = GICD_BASE + 0x0000;
const GICD_CTLR_ENABLE_GRP1: u32 = 1 << 1;
const GICD_CTLR_ARE: u32 = 1 << 4;
const GICD_CTLR_RWP: u32 = 1 << 31;

// GICR
const GICR_RD_BASE: usize = GICD_BASE + 0xA_0000;
const GICR_SGI_BASE: usize = GICR_RD_BASE + 0x1_0000;

const GICR_WAKER: usize = GICR_RD_BASE + 0x0014;
const GICR_WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;

const GICR_IGROUPR0: usize = GICR_SGI_BASE + 0x0080;
const GICR_ISENABLER0: usize = GICR_SGI_BASE + 0x0100;
const GICR_ICFGR1: usize = GICR_SGI_BASE + 0x0C04;
const GICR_IPRIORITYR: usize = GICR_SGI_BASE + 0x0400;

const TIMER_INTID: u32 = 27;
const TIMER_PRIORITY: u8 = 0xA0;
const GICR_ICFGR1_PPI27_MASK: u32 = 0b11 << 22;

// CPU interface
const ICC_SRE_ENABLE: u64 = 1 << 0;
const ICC_CTLR_EOIMODE: u64 = 1 << 1;
const ICC_PMR_ALL: u64 = 0xFF;
const ICC_IGRPEN1_ENABLE: u64 = 1 << 0;

/// Инициализирует GICv3 (GICD + GICR ядра 0) и CPU-интерфейс.
///
/// # Safety
///
/// - MMU включена, регион GIC `0x0800_0000..0x081F_FFFF` отображён как Device.
/// - Вызывается один раз, на ядре 0, после `irq::init()` (VBAR_EL1).
/// - Прерывания на CPU ещё замаскированы (`PSTATE.I = 1`), т.е. до `daifclr`.
/// - Нет конкурентного доступа к GICD/GICR.
pub unsafe fn init_gic() {
    // Включение Group 1 на дистрибьюторе
    let ctrl = unsafe { (GICD_BASE as *const u32).read_volatile() };
    unsafe {
        (GICD_BASE as *mut u32).write_volatile(ctrl | GICD_CTLR_ENABLE_GRP1 | GICD_CTLR_ARE);
    }

    let mut tries = 1_000_000;
    while unsafe { (GICD_BASE as *const u32).read_volatile() } & (1 << 12) != 0 {
        tries -= 1;
        if tries == 0 {
            panic!("GICD_CTLR.RWP stuck");
        }
        core::hint::spin_loop();
    }

    // разбудить редистрибьютор
    let waker = unsafe { ((GICR_WAKER) as *const u32).read_volatile() };
    unsafe {
        ((GICR_WAKER) as *mut u32).write_volatile(waker & !GICR_WAKER_PROCESSOR_SLEEP);
        core::arch::asm!("dsb sy");
    }
    let mut tries = 1_000_000;
    while unsafe { (GICR_WAKER as *const u32).read_volatile() } & (GICR_WAKER_CHILDREN_ASLEEP) != 0
    {
        tries -= 1;
        if tries == 0 {
            panic!("GICR_WAKER: ChildrenAsleep not free");
        }
        core::hint::spin_loop();
    }

    // INTID 27 в Group 1
    unsafe {
        // PPI 27: bits [23:22] = 0 → leve
        (GICR_ICFGR1 as *mut u32)
            .write_volatile((GICR_ICFGR1 as *const u32).read_volatile() & !GICR_ICFGR1_PPI27_MASK);

        // IGROUPR0
        (GICR_IGROUPR0 as *mut u32)
            .write_volatile((GICR_IGROUPR0 as *const u32).read_volatile() | (1 << TIMER_INTID));

        // IPRIORIYR[27]
        ((GICR_IPRIORITYR + TIMER_INTID as usize) as *mut u8).write_volatile(TIMER_PRIORITY);

        //ISENABLER0
        (GICR_ISENABLER0 as *mut u32).write_volatile(1 << TIMER_INTID);
    }

    // CPU-интерфейс
    // SRE
    let mut sre: u64;
    unsafe { core::arch::asm!("mrs {}, icc_sre_el1", out(reg) sre) };
    if sre & ICC_SRE_ENABLE == 0 {
        unsafe {
            core::arch::asm!(
                "msr icc_sre_el1, {v}",
                "isb",
                v = in(reg) sre |  ICC_SRE_ENABLE
            );
        }
    }

    // EOImode
    let ctrl: u64;
    unsafe { core::arch::asm!("mrs {}, icc_ctlr_el1", out(reg) ctrl) };
    assert!(
        ctrl & ICC_CTLR_EOIMODE == 0,
        "ICC_CTLR_EL1.EOImode=1: нужен ICC_DIR_EL1"
    );

    // PMR + Group 1
    unsafe {
        core::arch::asm!(
            "msr icc_pmr_el1, {pmr}",
            "msr icc_igrpen1_el1, {en}",
            "isb",
            pmr = in(reg) ICC_PMR_ALL,
            en  = in(reg) ICC_IGRPEN1_ENABLE,
        );
        core::arch::asm!("dsb sy", "isb");
    }
}

pub fn cntfrq() -> u64 {
    let v;
    unsafe { core::arch::asm!("mrs {}, cntfrq_el0", out(reg) v) };
    v
}

pub unsafe fn start_timer(ticks: u32) {
    unsafe {
        core::arch::asm!(
            "msr cntv_tval_el0, {t}",
            "isb",
            "msr cntv_ctl_el0, {en}",
            "isb",
            t = in(reg) ticks as u64,
            en = in(reg) 1u64, // ENABLE=1, IMASK=0
        );
    }
}

#[unsafe(no_mangle)]
extern "C" fn irq_handler() {
    let iar = unsafe { read_icc_iar1_el1() };
    let intid = (iar & 0xFFFFFF) as u32;

    if intid == 1023 {
        return; // spurious, EOI не пишем
    }

    if intid == TIMER_INTID {
        // сначала гасим источник
        unsafe { core::arch::asm!("msr cntv_tval_el0, {}", "isb", in(reg) (cntfrq() / 4)) };
        TICKS.fetch_add(1, Ordering::Relaxed);
    }

    // потом EOI
    unsafe { core::arch::asm!("msr icc_eoir1_el1, {}", in(reg) iar) };
}

unsafe fn read_icc_iar1_el1() -> u64 {
    let v;
    unsafe { core::arch::asm!("mrs {}, icc_iar1_el1", out(reg) v) };
    v
}
