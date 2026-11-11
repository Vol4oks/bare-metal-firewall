use core::fmt::Write;

use crate::{board, sync::SpinLock};

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

pub struct Uart {
    inner: SpinLock<Pl011>,
}

pub static UART: Uart = Uart {
    inner: SpinLock::new(Pl011::new(board::UART_BASE)),
};

impl Uart {
    ///  Обычный вывод: ждёт освобождения UART.
    pub fn with<R>(&self, f: impl FnOnce(&mut Pl011) -> R) -> R {
        let mut guard = self.inner.lock();
        f(&mut guard)
    }

    /// Для паники/фатальных исключений: не блокируется.
    pub fn try_with<R>(&self, f: impl FnOnce(&mut Pl011) -> R) -> Option<R> {
        let mut guard = self.inner.try_lock()?;
        Some(f(&mut guard))
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

    pub fn try_print_args(&self, args: core::fmt::Arguments<'_>) -> bool {
        self.try_with(|p| {
            let _ = core::fmt::write(p, args);
        })
        .is_some()
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
