use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::scheduler;

const STACK_SIZE: usize = 16 * 1024;
const CANARY: u64 = 0xDEAD_BEEF_0001_0001;
const ROUNDS: u32 = 10;

#[allow(dead_code)]
// поле читается только через сырой указатель в init_stack/canary_ok
#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

static mut STACK_A: Stack = Stack([0; STACK_SIZE]);
static mut STACK_B: Stack = Stack([0; STACK_SIZE]);
static mut STACK_MON: Stack = Stack([0; STACK_SIZE]);

static A_COUNT: AtomicU32 = AtomicU32::new(0);
static B_COUNT: AtomicU32 = AtomicU32::new(0);
static ERRORS: AtomicU32 = AtomicU32::new(0);
static A_DONE: AtomicBool = AtomicBool::new(false);
static B_DONE: AtomicBool = AtomicBool::new(false);

static LAST: AtomicU32 = AtomicU32::new(u32::MAX); // MAX = «ещё никого не было»

fn note_run(id: u32) {
    let prev = LAST.swap(id, Ordering::SeqCst);
    if prev == id {
        // одна и та же задача два раза подряд => переключения не было
        ERRORS.fetch_add(1, Ordering::SeqCst);
    }
}

fn init_stack(stack: *mut Stack) -> usize {
    unsafe { (stack as *mut u64).write_volatile(CANARY) };
    stack as usize + core::mem::size_of::<Stack>()
}

extern "C" fn task_a() -> ! {
    let mut marker = [0xA5u8; 64];

    for _ in 0..ROUNDS {
        note_run(0);
        if marker.iter().any(|&b| b != 0xA5) {
            ERRORS.fetch_add(1, Ordering::SeqCst);
            marker = [0xA5; 64];
        }
        core::hint::black_box(&marker);
        A_COUNT.fetch_add(1, Ordering::SeqCst);
        scheduler::yield_now();
    }

    A_DONE.store(true, Ordering::SeqCst);
    scheduler::exit()
}

fn canary_ok(stack: *mut Stack) -> bool {
    unsafe { (stack as *const u64).read_volatile() == CANARY }
}

extern "C" fn task_b() -> ! {
    let mut marker = [0x5Au8; 64];

    for _ in 0..ROUNDS {
        note_run(1);
        if marker.iter().any(|&b| b != 0x5A) {
            ERRORS.fetch_add(1, Ordering::SeqCst);
            marker = [0x5A; 64];
        }
        core::hint::black_box(&marker);
        B_COUNT.fetch_add(1, Ordering::SeqCst);
        scheduler::yield_now();
    }

    B_DONE.store(true, Ordering::SeqCst);
    scheduler::exit()
}

extern "C" fn monitor() -> ! {
    let mut spins = 0u32;
    while !(A_DONE.load(Ordering::SeqCst) && B_DONE.load(Ordering::SeqCst)) {
        spins += 1;
        if spins > 10_000 {
            crate::println!("sched: FAIL timeout");
            scheduler::exit();
        }
        scheduler::yield_now();
    }

    let counts =
        A_COUNT.load(Ordering::SeqCst) == ROUNDS && B_COUNT.load(Ordering::SeqCst) == ROUNDS;
    let errors = ERRORS.load(Ordering::SeqCst);
    let canaries = canary_ok(core::ptr::addr_of_mut!(STACK_A))
        && canary_ok(core::ptr::addr_of_mut!(STACK_B))
        && canary_ok(core::ptr::addr_of_mut!(STACK_MON));

    if counts && errors == 0 && canaries {
        crate::println!("sched: switch ok");
        crate::println!("sched: canary ok");
    } else {
        crate::println!(
            "sched: FAIL counts={} errors={} canary={}",
            counts,
            errors,
            canaries
        );
    }

    scheduler::exit()
}

pub fn run() -> ! {
    let top_a = init_stack(core::ptr::addr_of_mut!(STACK_A));
    let top_b = init_stack(core::ptr::addr_of_mut!(STACK_B));
    let top_m = init_stack(core::ptr::addr_of_mut!(STACK_MON));

    scheduler::spawn(task_a, "a", scheduler::Prio::CP, top_a).expect("slot a");
    scheduler::spawn(task_b, "b", scheduler::Prio::CP, top_b).expect("slot b");
    scheduler::spawn(monitor, "mon", scheduler::Prio::CP, top_m).expect("slot mon");

    scheduler::run()
}
