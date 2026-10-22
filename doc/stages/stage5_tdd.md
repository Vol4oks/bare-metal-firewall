---
tags: [этап-5, планировщик, тесты, tdd]
aliases: [sched tests, stage5 tdd]
stage: 5
---

# Этап 5: тесты до реализации

> [!abstract] Суть
> Планировщик тестируем на двух уровнях: **policy** (кто следующий) — host-тесты без asm; **mechanism** (context switch) — selftest в QEMU. Сначала пишем policy-тесты и доводим их до зелёного, затем mechanism-тесты и только потом preemption.

## Зачем разделять

- **Policy** — чистая логика состояний и round-robin. Её можно гонять на хосте, без железа и asm.
- **Mechanism** — `sched_switch`, стеки, регистры. Это проверяется только в QEMU.
- Если смешать их, любая ошибка в asm ломает и логику, и тесты; TDD становится невозможным.

## Policy API

```rust
pub const MAX_TASKS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskState {
    Unused,
    Ready,
    Running,
    Exited,
}

pub struct Scheduler {
    states: [TaskState; MAX_TASKS],
    current: Option<usize>,
    cursor: usize,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            states: [TaskState::Unused; MAX_TASKS],
            current: None,
            cursor: 0,
        }
    }

    /// Занимает первый Unused-слот и переводит его в Ready.
    pub fn spawn(&mut self) -> Option<usize> {
        for (id, state) in self.states.iter_mut().enumerate() {
            if *state == TaskState::Unused {
                *state = TaskState::Ready;
                return Some(id);
            }
        }
        None
    }

    pub fn start(&mut self) -> Option<usize> {
        let next = self.pick_next()?;
        self.states[next] = TaskState::Running;
        self.current = Some(next);
        Some(next)
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn state(&self, id: usize) -> TaskState {
        self.states[id]
    }

    /// Running -> Ready, выбрать следующую Ready.
    pub fn yield_current(&mut self) -> Option<usize> {
        let current = self.current?;
        self.states[current] = TaskState::Ready;

        let next = self.pick_next()?;
        self.states[next] = TaskState::Running;
        self.current = Some(next);
        Some(next)
    }

    /// Running -> Exited. Если Ready пуст — current = None (idle).
    pub fn exit_current(&mut self) -> Option<usize> {
        let current = self.current?;
        self.states[current] = TaskState::Exited;

        match self.pick_next() {
            Some(next) => {
                self.states[next] = TaskState::Running;
                self.current = Some(next);
                Some(next)
            }
            None => {
                self.current = None;
                None
            }
        }
    }

    fn pick_next(&mut self) -> Option<usize> {
        for offset in 0..MAX_TASKS {
            let id = (self.cursor + offset) % MAX_TASKS;
            if self.states[id] == TaskState::Ready {
                self.cursor = (id + 1) % MAX_TASKS;
                return Some(id);
            }
        }
        None
    }

    pub fn assert_invariants(&self) {
        let running = self
            .states
            .iter()
            .filter(|s| **s == TaskState::Running)
            .count();

        match self.current {
            Some(id) => {
                assert_eq!(running, 1, "must be exactly one Running");
                assert_eq!(self.states[id], TaskState::Running);
            }
            None => assert_eq!(running, 0, "no current => no Running"),
        }
    }
}
```

## Host-тесты policy

`Cargo.toml` отдельного `no_std`-крейта (например, `sched-policy`):

```rust
#![cfg_attr(not(test), no_std)]
```

Тесты:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let mut s = Scheduler::new();
        assert_eq!(s.current(), None);
        assert_eq!(s.start(), None);
        assert_eq!(s.yield_current(), None);
        assert_eq!(s.exit_current(), None);
        s.assert_invariants();
    }

    #[test]
    fn spawn_marks_ready() {
        let mut s = Scheduler::new();
        let a = s.spawn().unwrap();
        let b = s.spawn().unwrap();

        assert_ne!(a, b);
        assert_eq!(s.state(a), TaskState::Ready);
        assert_eq!(s.state(b), TaskState::Ready);
        s.assert_invariants();
    }

    #[test]
    fn start_picks_first_ready() {
        let mut s = Scheduler::new();
        let a = s.spawn().unwrap();
        let _b = s.spawn().unwrap();

        assert_eq!(s.start(), Some(a));
        assert_eq!(s.current(), Some(a));
        assert_eq!(s.state(a), TaskState::Running);
        s.assert_invariants();
    }

    #[test]
    fn yield_round_robins() {
        let mut s = Scheduler::new();
        let a = s.spawn().unwrap();
        let b = s.spawn().unwrap();
        let c = s.spawn().unwrap();

        assert_eq!(s.start(), Some(a));
        assert_eq!(s.yield_current(), Some(b));
        assert_eq!(s.yield_current(), Some(c));
        assert_eq!(s.yield_current(), Some(a));
        assert_eq!(s.current(), Some(a));
        s.assert_invariants();
    }

    #[test]
    fn single_task_yields_to_itself() {
        let mut s = Scheduler::new();
        let a = s.spawn().unwrap();

        assert_eq!(s.start(), Some(a));
        assert_eq!(s.yield_current(), Some(a));
        assert_eq!(s.state(a), TaskState::Running);
        s.assert_invariants();
    }

    #[test]
    fn exit_falls_to_next_then_idle() {
        let mut s = Scheduler::new();
        let a = s.spawn().unwrap();
        let b = s.spawn().unwrap();

        assert_eq!(s.start(), Some(a));
        assert_eq!(s.exit_current(), Some(b));
        assert_eq!(s.state(a), TaskState::Exited);
        assert_eq!(s.current(), Some(b));

        assert_eq!(s.exit_current(), None);
        assert_eq!(s.current(), None);
        s.assert_invariants();
    }

    #[test]
    fn spawn_fails_when_full() {
        let mut s = Scheduler::new();
        for _ in 0..MAX_TASKS {
            assert!(s.spawn().is_some());
        }
        assert_eq!(s.spawn(), None);
    }
}
```

Запуск на хосте:

```bash
cargo test -p sched-policy
```

Это ваш первый красный/зелёный цикл. До реализации методы можно оставить `todo!()` — тесты должны упасть, а не «не собраться».

## QEMU-тесты mechanism

Только после зелёной policy. Feature `schedtest`:

```toml
[features]
schedtest = []
```

`src/schedtest/mod.rs`:

```rust
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::{println, sched};

const ROUNDS: u32 = 10;

static A_COUNT: AtomicU32 = AtomicU32::new(0);
static B_COUNT: AtomicU32 = AtomicU32::new(0);
static ERRORS: AtomicU32 = AtomicU32::new(0);
static A_DONE: AtomicBool = AtomicBool::new(false);
static B_DONE: AtomicBool = AtomicBool::new(false);

extern "C" fn task_a() -> ! {
    let mut buf = [0xA5u8; 64];

    for _ in 0..ROUNDS {
        if buf.iter().any(|&b| b != 0xA5) {
            ERRORS.fetch_add(1, Ordering::SeqCst);
        }
        A_COUNT.fetch_add(1, Ordering::SeqCst);
        sched::yield_now();
    }

    A_DONE.store(true, Ordering::SeqCst);
    sched::exit()
}

extern "C" fn task_b() -> ! {
    let mut buf = [0x5Au8; 64];

    for _ in 0..ROUNDS {
        if buf.iter().any(|&b| b != 0x5A) {
            ERRORS.fetch_add(1, Ordering::SeqCst);
        }
        B_COUNT.fetch_add(1, Ordering::SeqCst);
        sched::yield_now();
    }

    B_DONE.store(true, Ordering::SeqCst);
    sched::exit()
}

extern "C" fn monitor() -> ! {
    loop {
        if A_DONE.load(Ordering::SeqCst) && B_DONE.load(Ordering::SeqCst) {
            if ERRORS.load(Ordering::SeqCst) == 0
                && A_COUNT.load(Ordering::SeqCst) == ROUNDS
                && B_COUNT.load(Ordering::SeqCst) == ROUNDS
            {
                println!("sched: switch ok");
            } else {
                println!("sched: FAIL");
            }
            sched::exit();
        }
        sched::yield_now();
    }
}

pub fn run() -> ! {
    sched::init();
    let _a = sched::spawn(task_a);
    let _b = sched::spawn(task_b);
    let _m = sched::spawn(monitor);
    sched::start()
}
```

Ожидаемый вывод:

```text
sched: switch ok
```

Если маркеры в локальных буферах испортились — `sched: FAIL`: значит, стеки/контекст сохраняются неверно.

### Почему такой тест ловит ошибки

- `buf` живёт на стеке задачи; после `yield` он должен остаться целым.
- `A_COUNT`/`B_COUNT` проверяют, что обе задачи реально выполнялись.
- `monitor` проверяет результат после завершения A и B.
- `sched::exit()` проверяет переход Exited → следующая задача/idle.

## TDD-порядок

1. Написать policy API с `todo!()` и тесты; `cargo test -p sched-policy` — красный.
2. Реализовать policy; тесты зелёные.
3. Добавить `sched_switch`, TCB, стеки.
4. Добавить `schedtest`; `cargo run --features schedtest` — `switch ok`.
5. Добавить idle и тест «после exit — idle».
6. Только потом preemptive: timer quantum и switch на выходе из IRQ.

## Подводные камни тестов

> [!warning]
> - Policy-тесты не проверяют asm; mechanism-тесты не проверяют fairness.
> - В QEMU-тесте задачи не должны печатать в цикле — иначе не увидите результат.
> - `monitor` тоже задача и участвует в round-robin; учитывайте это.
> - Не тестируйте preemptive до cooperative: ошибки контекста будет невозможно локализовать.
> - Для sequence-тестов используйте `Ordering::SeqCst`; `Relaxed` может скрыть ошибку.
> - После `exit` всех задач нужен idle или halt, иначе `start`/switch уйдёт в никуда.

## Связанные заметки
- [[stage5_scheduler]], [[task_and_scheduler]], [[context_switch]], [[no_std_testing]], [[spinlock]]
