use core::{cell::UnsafeCell, fmt::Write};

// Адрес UART0 (PL011) в QEMU virt
const UART_ADDR: usize = 0x0900_0000;
const MASK_TXFF: u8 = 1 << 5;

pub struct Pl011 {
    base: usize,
}
impl Pl011 {
    pub const fn new(base: usize) -> Self {
        Self { base }
    }
    pub fn write_byte(&mut self, byte: u8) {
        // адрес FR: арифметика usize + приведение
        let fr_ptr = (self.base + 0x18) as *const u8;
        // адрес DR: *const — читать, *mut — писать
        let dr_ptr = self.base as *mut u8;
        unsafe {
            while fr_ptr.read_volatile() & MASK_TXFF != 0 {
                core::hint::spin_loop();
            }
            dr_ptr.write_volatile(byte);
        }
    }
}

impl core::fmt::Write for Pl011 {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

/// Безопасно: доступ только с ядра 0, IRQ замаскированы в _start ([[multicore_path]]).
/// TODO: заменим на SpinLock, API не изменится.
pub struct Uart {
    inner: UnsafeCell<Pl011>,
}

unsafe impl Sync for Uart {}

pub static UART: Uart = Uart {
    inner: UnsafeCell::new(Pl011::new(UART_ADDR)),
};

impl Uart {
    pub fn with<R>(&self, f: impl FnOnce(&mut Pl011) -> R) -> R {
        unsafe { f(&mut *self.inner.get()) }
    }

    #[allow(dead_code)]
    pub fn print_str(&self, s: &str) {
        self.with(|p| {
            let _ = p.write_str(s);
        });
    }

    pub fn print_args(&self, args: core::fmt::Arguments<'_>) {
        self.with(|p| {
            let _ = core::fmt::write(p, args);
        });
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::uart::UART.print_args(format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($fmt:expr) => {
        $crate::print!(concat!($fmt, "\n"));
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::print!(concat!($fmt, "\n"), $($arg)*);
    };
}
