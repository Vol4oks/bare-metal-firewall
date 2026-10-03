#[derive(Clone, Copy)]
enum RegionSize {
    Size1G,
    Size2M,
}

#[derive(Clone, Copy)]
enum RegionType {
    Normal,
    Device,
}

struct Region {
    va: usize,
    size: RegionSize,
    kind: RegionType,
}

const REGIONS: [Region; 2] = [
    Region {
        va: 0x4000_0000,
        size: RegionSize::Size1G,
        kind: RegionType::Normal,
    },
    Region {
        va: 0x0900_0000,
        size: RegionSize::Size2M,
        kind: RegionType::Device,
    },
];

#[repr(align(4096))]
pub struct PageTable(pub [u64; 512]);

pub static mut L0_TABLE: PageTable = PageTable([0; 512]);
pub static mut L1_TABLE: PageTable = PageTable([0; 512]);
pub static mut L2_TABLE: PageTable = PageTable([0; 512]);

pub unsafe fn init_identity_map() {
    let l0 = unsafe { &mut *(&raw mut L0_TABLE) };
    let l1 = unsafe { &mut *(&raw mut L1_TABLE) };
    let l2 = unsafe { &mut *(&raw mut L2_TABLE) };

    let l1_pa = &raw const L1_TABLE as usize;
    let l2_pa = &raw const L2_TABLE as usize;

    l0.0[0] = (l1_pa as u64) | 0b11;

    for r in REGIONS.iter() {
        match r.size {
            RegionSize::Size1G => {
                let idx = r.va >> 30;
                l1.0[idx] = (r.va as u64) | attrs(r.kind);
            }
            RegionSize::Size2M => {
                let l1_idx = r.va >> 30;
                let l2_idx = (r.va >> 21) & 0x1FF;
                l1.0[l1_idx] = (l2_pa as u64) | 0b11;
                l2.0[l2_idx] = (r.va as u64) | attrs(r.kind);
            }
        }
    }
}

fn attrs(kind: RegionType) -> u64 {
    match kind {
        RegionType::Normal => (0 << 2) | (0b00 << 6) | (1 << 10) | 0b01, // AttrIndx=0, AP=00, AF, Block
        RegionType::Device => (1 << 2) | (0b00 << 6) | (1 << 10) | 0b01, // AttrIndx=1, AP=00, AF, Block
    }
}

pub fn l0_pa() -> usize {
    &raw const L0_TABLE as usize
}

pub unsafe fn enable(l0_pa: usize) {
    let mair_value: u64 = 0x04FF; // Attr0=0xFF (Normal), Attr1=0x04 (Device) в байтах 1
    let tcr_value: u64 = (16)                    // T0SZ=16 → 48-бит VA
        | (0b01 << 8) | (0b01 << 10)       // IRGN0, ORGN0 — page tables WB
        | (0b11 << 12)                     // SH0 — inner shareable
        | (0b00 << 14); // TG0 = 4KB
    unsafe {
        core::arch::asm!(
            "msr mair_el1, {mair}",
            "msr tcr_el1, {tcr}",
            "msr ttbr0_el1, {ttbr}",
            "dsb sy",
            "isb",
            "mrs {tmp}, sctlr_el1",
            "orr {tmp}, {tmp}, #1",            // M = 1
            "msr sctlr_el1, {tmp}",
            "isb",
            mair = in(reg) mair_value,
            tcr = in(reg) tcr_value,
            ttbr = in(reg) l0_pa,
            tmp = out(reg) _,
        );
    }
}
