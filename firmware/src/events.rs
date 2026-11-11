#![allow(dead_code)] // TODO(этап 6): NET_RX/TX_DONE понадобятся драйверу

pub type EventSet = u32;
pub const TICK: EventSet = 1 << 0;
pub const NET_RX: EventSet = 1 << 1;
pub const TX_DONE: EventSet = 1 << 2;

pub fn bit_index(ev: EventSet) -> usize {
    debug_assert!(ev.is_power_of_two(), "event must be a single bit");
    ev.trailing_zeros() as usize
}
