pub const RAM_BASE: usize = 0x4000_0000;
pub const GICD_BASE: usize = 0x0800_0000;
pub const GICR_RD_BASE: usize = 0x080A_0000;
pub const UART_BASE: usize = 0x0900_0000;
pub const GICR_STRIDE: usize = 0x2_0000; // 128 КиБ на ядро, QEMU virt
