use crate::scheduler::{BlockResult, MAX_TASKS, Prio, Scheduler, TaskState};

const EV_A: u32 = 1 << 0;
const EV_B: u32 = 1 << 1;

extern "C" fn dummy() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

fn stack_top(n: usize) -> usize {
    0x8000 + n * 0x1000 // кратно 16
}

pub fn run() {
    crate::println!("sched: start");

    states();
    crate::println!("  sched: states ok");

    round_robin();
    crate::println!("  sched: round-robin ok");

    priority();
    crate::println!("  sched: priority ok");

    events();
    crate::println!("  sched: events ok");

    model();
    crate::println!("  sched: model ok");

    crate::println!("sched: end");
}

fn states() {
    let mut s = Scheduler::new();
    assert_eq!(s.current(), None);
    assert_eq!(s.start(), None);
    assert_eq!(s.yield_current(), None);
    assert_eq!(s.exit_current(), None);

    let a = s.spawn(dummy, "a", Prio::CP, stack_top(0)).unwrap();
    let b = s.spawn(dummy, "b", Prio::CP, stack_top(1)).unwrap();
    let c = s.spawn(dummy, "c", Prio::CP, stack_top(2)).unwrap();
    assert_eq!((a, b, c), (0, 1, 2));
    assert_eq!(s.state(a), TaskState::Ready);

    assert_eq!(s.start(), Some(a));
    assert_eq!(s.exit_current(), Some(b));
    assert_eq!(s.exit_current(), Some(c));
    assert_eq!(s.exit_current(), None);
    assert_eq!(s.current(), None);

    for i in 3..MAX_TASKS {
        assert!(s.spawn(dummy, "t", Prio::CP, stack_top(i)).is_some());
    }
    assert_eq!(s.spawn(dummy, "overflow", Prio::CP, stack_top(0)), None);

    // Все слоты Exited — переиспользования нет.
    assert!(s.start().is_some());
    while s.exit_current().is_some() {}
    assert_eq!(s.spawn(dummy, "after-exit", Prio::CP, stack_top(0)), None);

    s.assert_invariants();
}

fn round_robin() {
    let mut s = Scheduler::new();
    let a = s.spawn(dummy, "a", Prio::CP, stack_top(0)).unwrap();
    let b = s.spawn(dummy, "b", Prio::CP, stack_top(1)).unwrap();
    let c = s.spawn(dummy, "c", Prio::CP, stack_top(2)).unwrap();

    assert_eq!(s.start(), Some(a));
    assert_eq!(s.yield_current(), Some(b));
    assert_eq!(s.yield_current(), Some(c));
    assert_eq!(s.yield_current(), Some(a));
    s.assert_invariants();

    let mut one = Scheduler::new();
    let only = one.spawn(dummy, "only", Prio::CP, stack_top(0)).unwrap();
    assert_eq!(one.start(), Some(only));
    assert_eq!(one.yield_current(), Some(only));
    one.assert_invariants();
}

fn priority() {
    // Строгий приоритет: пока DP готов, CP не получает CPU.
    let mut s = Scheduler::new();
    let cp = s.spawn(dummy, "cp", Prio::CP, stack_top(0)).unwrap();
    let dp = s.spawn(dummy, "dp", Prio::DP, stack_top(1)).unwrap();

    assert_eq!(s.start(), Some(dp));
    assert_eq!(s.state(cp), TaskState::Ready);
    assert_eq!(s.yield_current(), Some(dp));
    assert_eq!(s.exit_current(), Some(cp));

    // Спавн не вытесняет: приоритет решает только в safe point.
    let mut t = Scheduler::new();
    let cp = t.spawn(dummy, "cp", Prio::CP, stack_top(0)).unwrap();
    assert_eq!(t.start(), Some(cp));
    let dp = t.spawn(dummy, "dp", Prio::DP, stack_top(1)).unwrap();
    assert_eq!(t.current(), Some(cp));
    assert_eq!(t.yield_current(), Some(dp));

    t.assert_invariants();
}

fn events() {
    let mut s = Scheduler::new();
    let a = s.spawn(dummy, "a", Prio::CP, stack_top(0)).unwrap();
    let b = s.spawn(dummy, "b", Prio::CP, stack_top(1)).unwrap();
    assert_eq!(s.start(), Some(a));

    // Событие пришло до сна: block обязан отказаться.
    assert!(!s.wake(a, EV_A)); // Running: только латч
    assert_eq!(s.block_current(EV_A), BlockResult::NotBlocked);
    assert_eq!(s.state(a), TaskState::Running);
    assert_eq!(s.try_wait_current(EV_A), Some(EV_A));
    assert_eq!(s.try_wait_current(EV_A), None);

    // Сон и пробуждение.
    match s.block_current(EV_A) {
        BlockResult::Blocked { next } => assert_eq!(next, Some(b)),
        BlockResult::NotBlocked => panic!("must block"),
    }
    assert_eq!(s.state(a), TaskState::Blocked);

    assert!(!s.wake(a, EV_B)); // чужие биты: остаётся Blocked
    assert_eq!(s.state(a), TaskState::Blocked);
    assert!(s.wake(a, EV_A)); // свои биты: Ready
    assert_eq!(s.state(a), TaskState::Ready);

    assert_eq!(s.exit_current(), Some(a)); // b вышел, осталась только a
    assert_eq!(s.state(a), TaskState::Running);

    // try_wait снимает только маску.
    assert!(!s.wake(a, EV_A | EV_B));
    assert_eq!(s.try_wait_current(EV_A), Some(EV_A));
    assert_eq!(s.try_wait_current(EV_B), Some(EV_B));
    assert_eq!(s.try_wait_current(EV_A | EV_B), None);

    // Exited и Unused не латчат события.
    assert!(!s.wake(b, EV_A));
    let unused = (0..MAX_TASKS)
        .find(|&i| s.state(i) == TaskState::Unused)
        .unwrap();
    assert!(!s.wake(unused, EV_A));

    s.assert_invariants();
}

fn next_rand(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn model() {
    let mut s = Scheduler::new();
    let mut seed = 0x1234_5678_9abc_def0u64;

    for _ in 0..10_000 {
        let r = next_rand(&mut seed);
        let id = (r as usize >> 8) % MAX_TASKS;
        let bits = 1u32 << (id % 4);

        match (r >> 4) % 7 {
            0 => {
                let _ = s.spawn(dummy, "t", Prio::CP, stack_top(id));
            }
            1 => {
                let _ = s.start();
            }
            2 => {
                let _ = s.yield_current();
            }
            3 => {
                let _ = s.try_wait_current(bits);
            }
            4 => {
                let _ = s.block_current(bits);
            }
            5 => {
                let _ = s.wake(id, bits);
            }
            _ => {
                let _ = s.exit_current();
            }
        }

        s.assert_invariants();
    }
}
