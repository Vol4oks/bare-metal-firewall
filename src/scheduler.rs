//! Здесь данные и правила «кто следующий». Переключение контекста — 0.2,
//! атомарные события и IRQ — 0.4.
#![allow(dead_code)] // TODO(0.4): снять, когда планировщик станет основным циклом
use core::arch::global_asm;

global_asm!(include_str!("sched_switch.s"));

unsafe extern "C" {
    fn sched_switch(from: *mut TaskContext, to: *const TaskContext);
    fn sched_start(to: *const TaskContext) -> !;
}

static mut SCHED: Scheduler = Scheduler::new();

pub fn spawn(
    entry: extern "C" fn() -> !,
    name: &'static str,
    prio: Prio,
    stack_top: usize,
) -> Option<usize> {
    unsafe { (*core::ptr::addr_of_mut!(SCHED)).spawn(entry, name, prio, stack_top) }
}

pub fn yield_now() {
    let sched = unsafe { &mut *core::ptr::addr_of_mut!(SCHED) };

    let from = match sched.current() {
        Some(id) => id,
        None => return,
    };
    let next = match sched.yield_current() {
        Some(id) => id,
        None => return,
    };
    if next == from {
        return; // переключать нечего
    }

    let base = sched.tasks.as_mut_ptr();
    let from_ctx = unsafe { core::ptr::addr_of_mut!((*base.add(from)).ctx) };
    let to_ctx = unsafe { core::ptr::addr_of!((*base.add(next)).ctx) };
    unsafe { sched_switch(from_ctx, to_ctx) };
}

pub fn exit() -> ! {
    let sched = unsafe { &mut *core::ptr::addr_of_mut!(SCHED) };
    let base = sched.tasks.as_mut_ptr();

    match sched.exit_current() {
        Some(next) => {
            let ctx = unsafe { core::ptr::addr_of!((*base.add(next)).ctx) };
            unsafe { sched_start(ctx) }
        }
        None => loop {
            unsafe { core::arch::asm!("wfi") };
        },
    }
}

pub fn run() -> ! {
    let sched = unsafe { &mut *core::ptr::addr_of_mut!(SCHED) };
    let base = sched.tasks.as_mut_ptr();

    let first = match sched.start() {
        Some(id) => id,
        None => loop {
            unsafe { core::arch::asm!("wfi") };
        },
    };

    let ctx = unsafe { core::ptr::addr_of!((*base.add(first)).ctx) };
    unsafe { sched_start(ctx) }
}

pub const MAX_TASKS: usize = 8;

pub type EventSet = u32;

/// Меньше значение — выше приоритет (как у GIC).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Prio(u8);

impl Prio {
    pub const DP: Self = Self(0);
    pub const CP: Self = Self(10);
    pub const IDLE: Self = Self(255);

    pub const fn new(raw: u8) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskState {
    Unused,
    Ready,
    Running,
    Blocked,
    Exited,
}

/// Раскладка для `sched_switch`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TaskContext {
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64,
    pub x30: u64,
    pub sp: u64,

    // callee-saved SIMD (AAPCS64: v8–v15)
    pub q8: u128,
    pub q9: u128,
    pub q10: u128,
    pub q11: u128,
    pub q12: u128,
    pub q13: u128,
    pub q14: u128,
    pub q15: u128,
}

const _: () = assert!(core::mem::offset_of!(TaskContext, sp) == 96);
const _: () = assert!(core::mem::offset_of!(TaskContext, q8) == 112);
const _: () = assert!(core::mem::offset_of!(TaskContext, q15) == 224);
const _: () = assert!(core::mem::size_of::<TaskContext>() == 240);

pub struct Task {
    pub ctx: TaskContext,
    pub state: TaskState,
    pub prio: Prio,
    pub wait_mask: EventSet,
    pub pending: EventSet,
    pub entry: Option<extern "C" fn() -> !>,
    pub stack_top: usize,
    pub name: &'static str,
}

impl Task {
    const UNUSED: Self = Self {
        ctx: TaskContext {
            x19: 0,
            x20: 0,
            x21: 0,
            x22: 0,
            x23: 0,
            x24: 0,
            x25: 0,
            x26: 0,
            x27: 0,
            x28: 0,
            x29: 0,
            x30: 0,
            sp: 0,
            q8: 0,
            q9: 0,
            q10: 0,
            q11: 0,
            q12: 0,
            q13: 0,
            q14: 0,
            q15: 0,
        },
        state: TaskState::Unused,
        prio: Prio::IDLE,
        wait_mask: 0,
        pending: 0,
        entry: None,
        stack_top: 0,
        name: "",
    };
}

#[derive(Debug, PartialEq)]
pub enum BlockResult {
    /// Сон принят; `next` — кого запускать (`None` = idle).
    Blocked { next: Option<usize> },
    /// Не Running или событие уже залатчено.
    NotBlocked,
}

pub struct Scheduler {
    tasks: [Task; MAX_TASKS],
    current: Option<usize>,
    cursor: usize,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            tasks: [const { Task::UNUSED }; MAX_TASKS],
            current: None,
            cursor: 0,
        }
    }

    /// Занять первый `Unused`-слот. `Exited` не переиспользуется.
    pub fn spawn(
        &mut self,
        entry: extern "C" fn() -> !,
        name: &'static str,
        prio: Prio,
        stack_top: usize,
    ) -> Option<usize> {
        assert_ne!(stack_top, 0, "stack_top must be non-zero");
        assert_eq!(stack_top % 16, 0, "stack_top must be 16-byte aligned");

        let id = self
            .tasks
            .iter()
            .position(|task| task.state == TaskState::Unused)?;

        let ctx = TaskContext {
            sp: stack_top as u64,
            x30: entry as usize as u64,
            ..Default::default()
        };

        self.tasks[id] = Task {
            ctx,
            state: TaskState::Ready,
            prio,
            wait_mask: 0,
            pending: 0,
            entry: Some(entry),
            stack_top,
            name,
        };

        Some(id)
    }

    pub fn start(&mut self) -> Option<usize> {
        if let Some(id) = self.current {
            return Some(id);
        }

        let id = self.pick_next()?;
        self.tasks[id].state = TaskState::Running;
        self.current = Some(id);
        Some(id)
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn state(&self, id: usize) -> TaskState {
        self.tasks[id].state
    }

    pub fn yield_current(&mut self) -> Option<usize> {
        let current = self.current?;
        debug_assert_eq!(self.tasks[current].state, TaskState::Running);

        self.tasks[current].state = TaskState::Ready;

        let next = self.pick_next().expect("current was just made Ready");

        self.tasks[next].state = TaskState::Running;
        self.current = Some(next);
        Some(next)
    }

    pub fn exit_current(&mut self) -> Option<usize> {
        let current = self.current?;
        debug_assert_eq!(self.tasks[current].state, TaskState::Running);

        self.tasks[current].state = TaskState::Exited;
        self.tasks[current].entry = None;
        self.tasks[current].pending = 0;
        self.tasks[current].wait_mask = 0;

        match self.pick_next() {
            Some(next) => {
                self.tasks[next].state = TaskState::Running;
                self.current = Some(next);
                Some(next)
            }
            None => {
                self.current = None;
                None
            }
        }
    }

    /// Снять и вернуть только биты из `mask`.
    pub fn try_wait_current(&mut self, mask: EventSet) -> Option<EventSet> {
        let current = self.current?;
        if self.tasks[current].state != TaskState::Running {
            return None;
        }
        let hit = self.tasks[current].pending & mask;
        if hit == 0 {
            return None;
        }

        self.tasks[current].pending &= !hit;
        Some(hit)
    }

    /// `Running -> Blocked` + сразу выбрать следующую.
    pub fn block_current(&mut self, mask: EventSet) -> BlockResult {
        debug_assert_ne!(mask, 0, "blocking with empty mask would never wake");

        let Some(current) = self.current else {
            return BlockResult::NotBlocked;
        };

        if self.tasks[current].state != TaskState::Running {
            return BlockResult::NotBlocked;
        }

        if (self.tasks[current].pending & mask) != 0 {
            return BlockResult::NotBlocked;
        }

        self.tasks[current].wait_mask = mask;
        self.tasks[current].state = TaskState::Blocked;

        let next = self.pick_next();
        match next {
            Some(next) => {
                self.tasks[next].state = TaskState::Running;
                self.current = Some(next);
            }
            None => self.current = None,
        }

        BlockResult::Blocked { next }
    }

    /// Залатчить биты; `Blocked` + пересечение с `wait_mask` -> `Ready`.
    pub fn wake(&mut self, id: usize, bits: EventSet) -> bool {
        let task = &mut self.tasks[id];
        if matches!(task.state, TaskState::Unused | TaskState::Exited) {
            return false;
        }

        task.pending |= bits;

        if task.state == TaskState::Blocked && (task.pending & task.wait_mask) != 0 {
            task.state = TaskState::Ready;
            task.wait_mask = 0;
            return true;
        }

        false
    }

    fn pick_next(&mut self) -> Option<usize> {
        // Поиск минимального приоритета среди готовых
        let best_prio = self
            .tasks
            .iter()
            .filter(|task| task.state == TaskState::Ready)
            .map(|task| task.prio)
            .min()?;

        for offset in 0..MAX_TASKS {
            let id = (self.cursor + offset) % MAX_TASKS;
            if self.tasks[id].state == TaskState::Ready && self.tasks[id].prio == best_prio {
                self.cursor = (id + 1) % MAX_TASKS;
                return Some(id);
            }
        }

        None
    }

    /// Тестовый помощник. В горячем пути не вызывать.
    pub fn assert_invariants(&self) {
        let mut running = 0usize;

        for (id, task) in self.tasks.iter().enumerate() {
            match task.state {
                TaskState::Unused => {
                    assert!(task.entry.is_none(), "Unused[{id}] has entry");
                    assert_eq!(task.stack_top, 0, "Unused[{id}] has stack");
                    assert_eq!(task.pending, 0, "Unused[{id}] has pending");
                }
                TaskState::Ready | TaskState::Running | TaskState::Blocked => {
                    assert!(task.entry.is_some(), "{:?}[{id}] has no entry", task.state);
                    assert_ne!(task.stack_top, 0, "{:?}[{id}] has no stack", task.state);
                    assert_eq!(
                        task.stack_top % 16,
                        0,
                        "{:?}[{id}] stack misaligned",
                        task.state
                    );

                    if task.state == TaskState::Blocked {
                        assert_ne!(task.wait_mask, 0, "Blocked[{id}] without mask");
                    } else {
                        assert_eq!(task.wait_mask, 0, "{:?}[{id}] has wait_mask", task.state);
                    }

                    if task.state == TaskState::Running {
                        running += 1;
                        assert_eq!(self.current, Some(id), "Running[{id}] is not current");
                    }
                }
                TaskState::Exited => {
                    assert!(task.entry.is_none(), "Exited[{id}] has entry");
                    assert_eq!(task.pending, 0, "Exited[{id}] has pending");
                    assert_eq!(task.wait_mask, 0, "Exited[{id}] has wait_mask");
                }
            }
        }

        assert!(running <= 1, "more than one Running");
        if let Some(id) = self.current {
            assert_eq!(
                self.tasks[id].state,
                TaskState::Running,
                "current not Running"
            );
        }
    }
}
