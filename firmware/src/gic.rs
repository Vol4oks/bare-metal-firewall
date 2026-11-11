use core::arch::asm;

use crate::board;

//  GICD: смещения от base
const GICD_CTLR_OFFSET: usize = 0x0000;
const GICD_CTLR_ENABLE_GRP1: u32 = 1 << 1;
const GICD_CTLR_ARE: u32 = 1 << 4;
const GICD_CTLR_RWP: u32 = 1 << 31;

//  GICR: смещения от RD_base / SGI_base
const GICR_WAKER_OFFSET: usize = 0x0014;
const GICR_WAKER_PROCESSOR_SLEEP: u32 = 1 << 1;
const GICR_WAKER_CHILDREN_ASLEEP: u32 = 1 << 2;

const GICR_SGI_OFFSET: usize = 0x1_0000;
const GICR_IGROUPR0_OFFSET: usize = 0x0080;
const GICR_ISENABLER0_OFFSET: usize = 0x0100;
const GICR_ICFGR1_OFFSET: usize = 0x0C04;
const GICR_IPRIORITYR_OFFSET: usize = 0x0400;

//  CPU interface
const ICC_SRE_ENABLE: u64 = 1 << 0;
const ICC_CTLR_EOIMODE: u64 = 1 << 1;
const ICC_PMR_ALL: u64 = 0xFF;
const ICC_IGRPEN1_ENABLE: u64 = 1 << 0;

pub const SPURIOUS_INTID: u32 = 1023;

pub struct Gic {
    gicd: usize,
    gicr_rd: usize,
}

impl Gic {
    pub const fn new(gicd: usize, gicr_rd: usize) -> Self {
        Self { gicd, gicr_rd }
    }

    fn gicr_for_core(&self, core: usize) -> usize {
        self.gicr_rd + core * board::GICR_STRIDE
    }

    fn gicr_sgi(&self) -> usize {
        self.gicr_for_core(0) + GICR_SGI_OFFSET
    }

    pub unsafe fn init(&self) {
        unsafe {
            self.init_distributor();
            self.init_redistributor();
            self.init_cpu_interface();
        }
    }

    unsafe fn init_distributor(&self) {
        let addr = self.gicd + GICD_CTLR_OFFSET;
        let ctrl = unsafe { rd32(addr) };

        unsafe {
            wr32(addr, ctrl | GICD_CTLR_ENABLE_GRP1 | GICD_CTLR_ARE);
        }

        let mut tries = 1_000_000;
        while unsafe { rd32(addr) } & GICD_CTLR_RWP != 0 {
            tries -= 1;
            if tries == 0 {
                panic!("GICD_CTLR.RWP stuck");
            }
            core::hint::spin_loop();
        }
    }

    unsafe fn init_redistributor(&self) {
        let addr = self.gicr_for_core(0) + GICR_WAKER_OFFSET;
        let waker = unsafe { rd32(addr) };

        unsafe {
            wr32(addr, waker & !GICR_WAKER_PROCESSOR_SLEEP);
            asm!("dsb sy");
        }

        let mut tries = 1_000_000;
        while unsafe { rd32(addr) } & GICR_WAKER_CHILDREN_ASLEEP != 0 {
            tries -= 1;
            if tries == 0 {
                panic!("GICR_WAKER: ChildrenAsleep not free");
            }
            core::hint::spin_loop();
        }
    }

    unsafe fn init_cpu_interface(&self) {
        let mut sre: u64;
        unsafe { asm!("mrs {}, icc_sre_el1", out(reg) sre) };
        if sre & ICC_SRE_ENABLE == 0 {
            unsafe {
                asm!(
                    "msr icc_sre_el1, {v}",
                    "isb",
                    v = in(reg) sre | ICC_SRE_ENABLE,
                );
            }
        }

        let ctlr: u64;
        unsafe { asm!("mrs {}, icc_ctlr_el1", out(reg) ctlr) };
        assert!(ctlr & ICC_CTLR_EOIMODE == 0, "EOImode=1: нужен ICC_DIR_EL1");

        unsafe {
            asm!(
                "msr icc_pmr_el1, {pmr}",
                "msr icc_igrpen1_el1, {en}",
                "isb",
                pmr = in(reg) ICC_PMR_ALL,
                en  = in(reg) ICC_IGRPEN1_ENABLE,
            );
            asm!("dsb sy", "isb");
        }
    }

    pub unsafe fn enable_ppi(&self, intid: u32, priority: u8) {
        assert!((16..32).contains(&intid), "PPI INTID must be 16..31");

        let sgi = self.gicr_sgi();
        let shift = (intid - 16) * 2;

        // level-sensitive
        let cfg = unsafe { rd32(sgi + GICR_ICFGR1_OFFSET) };
        unsafe {
            wr32(sgi + GICR_ICFGR1_OFFSET, cfg & !(0b11 << shift));
        }

        // Group 1
        let group = unsafe { rd32(sgi + GICR_IGROUPR0_OFFSET) };
        unsafe {
            wr32(sgi + GICR_IGROUPR0_OFFSET, group | (1u32 << intid));
        }

        // priority
        unsafe {
            wr8(sgi + GICR_IPRIORITYR_OFFSET + intid as usize, priority);
        }

        // enable last
        unsafe {
            wr32(sgi + GICR_ISENABLER0_OFFSET, 1u32 << intid);
        }
    }

    pub unsafe fn ack(&self) -> u64 {
        let v;
        unsafe { asm!("mrs {}, icc_iar1_el1", out(reg) v) };
        v
    }

    pub unsafe fn eoi(&self, iar: u64) {
        unsafe { asm!("msr icc_eoir1_el1, {}", in(reg) iar) };
    }
}

pub static GIC: Gic = Gic::new(board::GICD_BASE, board::GICR_RD_BASE);

unsafe fn rd32(addr: usize) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

unsafe fn wr32(addr: usize, v: u32) {
    unsafe { (addr as *mut u32).write_volatile(v) }
}

unsafe fn wr8(addr: usize, v: u8) {
    unsafe { (addr as *mut u8).write_volatile(v) }
}
