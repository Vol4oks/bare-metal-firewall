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
    pub const fn new() -> Self;
    pub fn spawn(&mut self) -> Option<usize>;
    pub fn start(&mut self) -> Option<usize>;
    pub fn current(&self) -> Option<usize>;
    pub fn state(&self, id: usize) -> TaskState;
    pub fn yield_current(&mut self) -> Option<usize>;
    pub fn exit_current(&mut self) -> Option<usize>;
}
