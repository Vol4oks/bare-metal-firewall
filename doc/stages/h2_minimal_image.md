---
tags: [stage-1, bringup, h2, uart, el2, минимальный-образ]
aliases: [H2, минимальный образ, probe]
stage: 1
---

# H2: минимальный образ — EL2 → EL1 и байт в UART2

> [!abstract] Суть
> Цель H2 — изолировать три неизвестных сразу: адрес загрузки, уровень исключений и UART. Образ не делает ничего, кроме `E2` → `H1` → `G` в консоль и `wfi`: никакой MMU, GIC, кучи и планировщика. Если видно `E2H1G`, то база линковки верна, EL2 понижается, UART2 пишется, а `ICC_*_EL1` больше не ловит trap.

## Что доказывает каждый маркер

| Маркер | Что означает | Если его нет |
|---|---|---|
| `E2` | стартовали на EL2 (как и обещал BL31) | `E1` — уже EL1; `E3` — неожиданный EL3; тишина — адрес/UART/objcopy |
| `H1` | `eret` сработал, мы на EL1 | увидели только `E2` — ошибка в `HCR_EL2`/`SPSR_EL2`/`ELR_EL2` |
| `G` | `mrs ICC_SRE_EL1` не трапится | `H1` без `G` — понижение формально прошло, но sysreg недоступен |

## Отдельный крейт, а не feature в основном

H2 — одноразовый зонд. Смешивать его с этапом 5 (MMU, GIC, планировщик) значит тащить в образ половину проекта и рисковать QEMU-сборкой.

```text
probe/
├── Cargo.toml
├── linker-rk3566.ld
└── src/
    ├── main.rs
    └── start.s
```

`probe/Cargo.toml`:

```toml
[package]
name = "probe-h2"
version = "0.1.0"
edition = "2024"

[workspace]          # пустая таблица: крейт не часть воркспейса

[profile.dev]
panic = "abort"
[profile.release]
panic = "abort"

[dependencies]
```

## Линкер

`probe/linker-rk3566.ld`:

```ld
ENTRY(_start)

SECTIONS
{
    . = 0x02000000;                 /* kernel_addr_r из env U-Boot */

    .text : { KEEP(*(.text._start)) *(.text .text.*) }
    .rodata : { *(.rodata .rodata.*) }
    .data : { *(.data .data.*) }
    .bss : {
        . = ALIGN(16);
        *(.bss .bss.*)
        *(COMMON)
    }

    . = ALIGN(16);
    . = . + 0x4000;                 /* 16 КиБ стек */
    __stack_top = .;
}
```

## `start.s`

```asm
    .section .text._start, "ax"
    .global _start
_start:
    msr  daifset, #0xf              /* прерывания и debug — в маску */
    isb
    ldr  x0, =__stack_top           /* свой стек: SP от U-Boot нам не принадлежит */
    mov  sp, x0

    mrs  x19, currentel
    lsr  x19, x19, #2               /* 1, 2 или 3 */

    mov  w0, #0x45                  /* 'E' */
    bl   uart_putc
    add  w0, w19, #0x30             /* '0' + EL */
    bl   uart_putc

    cmp  x19, #2
    b.ne 1f                         /* уже EL1 — прыгаем печатать H */

    mov  x0, #(1 << 31)             /* HCR_EL2.RW = 1: EL1 — AArch64 */
    msr  hcr_el2, x0
    mov  x0, #0x3c5                 /* SPSR_EL2: EL1h + DAIF masked */
    msr  spsr_el2, x0
    adr  x0, 1f
    msr  elr_el2, x0
    isb
    eret

1:
    mov  w0, #0x48                  /* 'H' */
    bl   uart_putc
    mrs  x0, currentel
    lsr  x0, x0, #2
    add  w0, x0, #0x30
    bl   uart_putc

    mrs  x0, S3_0_C12_C12_5         /* ICC_SRE_EL1: на EL2 дал бы trap */
    mov  w0, #0x47                 /* 'G' */
    bl   uart_putc
    mov  w0, #0x0d                  /* '\r' */
    bl   uart_putc
    mov  w0, #0x0a                  /* '\n' */
    bl   uart_putc

2:  wfi
    b    2b
```

Три ловушки, которые здесь закрыты намеренно:

- **SP выставляется до первого `bl`.** Иначе пролог функции пишет в стек U-Boot — в его зарезервированную область сверху.
- **EL держится в `x19`.** Это callee-saved, `uart_putc` его не испортит; перечитывать `CurrentEL` до понижения было бы лишним риском.
- **`SPSR_EL2 = 0x3c5`, а не `0x3c9`.** `0x3c9` — это EL2h (как у BL31); нам нужен EL1h: `M[3:0] = 0b0101` + `DAIF = 1111`.

## `main.rs`

```rust
#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

global_asm!(include_str!("start.s"));

/// UART2 на RK3566 = DesignWare 16550, `reg-shift = 2`, доступ 32-битный.
const UART: usize = 0xFE66_0000;
const THR: usize = 0x00; // регистр 0
const LSR: usize = 0x14; // регистр 5, сдвинутый на 2
const THRE: u32 = 1 << 5;

#[unsafe(no_mangle)]
pub extern "C" fn uart_putc(c: u32) {
    unsafe {
        let lsr = (UART + LSR) as *const u32;
        let thr = (UART + THR) as *mut u32;
        while (lsr.read_volatile() & THRE) == 0 {}
        thr.write_volatile(c & 0xff);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
```

> [!warning] Почему не трогаем CRU и пинмукс
> U-Boot уже настроил UART2 (24 МГц, 1500000 8N1, `shift=2`). Нам остаётся писать в THR. Если в будущем понадобится другой UART или скорость — придётся разбираться с CRU/GRF, но не на H2.

## Сборка и плоский бинарь

`go` не разбирает ELF: ему нужен плоский `bin`.

```bash
cd probe

# ВАЖНО: RUSTFLAGS отменяет rustflags из родительского .cargo/config.toml,
# иначе подставится QEMU-линкер с базой 0x42000000
RUSTFLAGS="-C link-arg=-Tlinker-rk3566.ld" \
    cargo build --release --target aarch64-unknown-none

# плоский образ
rust-objcopy -O binary \
    target/aarch64-unknown-none/release/probe-h2 fw.bin
# если rust-objcopy нет: llvm-objcopy, aarch64-linux-gnu-objcopy

ls -l fw.bin
```

Проверка перед отправкой:

```bash
llvm-objdump -h target/aarch64-unknown-none/release/probe-h2 | head -20   # .text на 0x02000000
xxd -l 16 fw.bin                                                          # не пусто
```

## Доставка: TFTP

Сначала — **какой порт GMAC**. Это одновременно закрывает открытый вопрос из [[h0_fact_sheet]]: в OpenWrt подключить кабель в один RJ45 и посмотреть `ip -br link` (или `dmesg | grep -i link`), затем в другой. `eth0` — это GMAC (`fe010000`), `eth1` — RTL8111H.

На хосте: любой TFTP-сервер с `fw.bin` в корне (например, `tftpd-hpa`, `dnsmasq --enable-tftp`), адрес хоста — `192.168.1.1`.

В U-Boot (env энергонезависимого нет, поэтому каждый раз):

```text
setenv ipaddr 192.168.1.10
setenv serverip 192.168.1.1
tftp 0x02000000 fw.bin
go 0x02000000
```

Ожидаемо: `E2H1G` и перевод строки, дальше плата стоит в `wfi`.

> [!tip] Если TFTP недоступен
> Запасной канал — USB-флешка с FAT (`usb start; fatload usb 0:1 0x02000000 fw.bin; go 0x02000000`), если в этом U-Boot собран USB-хост. Запись в SD не использовать: там корень OpenWrt.

## Таблица отказов

| Что видно | Диагноз | Что проверить |
|---|---|---|
| совсем тишина | не дошли до UART или образ не тот | `go`-адрес = база линковки; `objcopy` (не ELF ли отправили); `xxd` бинаря |
| `E3` | стартовали на EL3 | в этом U-Boot EL2 не гарантирован: нужен путь через `SCR_EL3` |
| `E2` и стоп | `eret` не состоялся | `SPSR_EL2` (должен быть `0x3c5`), `ELR_EL2` (адрес `1f`), `HCR_EL2.RW` |
| `E2H1` без `G` | понижение прошло, sysreg трапится | `HCR_EL2` перезаписан не целиком (не должно быть), проверить `ICC_SRE_EL1` |
| `E1...` | уже EL1 | нормально, понижение не нужно |
| мусор | не та скорость/база | 1500000, `0xFE66_0000`, запись 32-битная |
| зависание до первого байта | испорчен SP | `__stack_top` в линкере, `ldr =__stack_top` |

## Грабли

> [!warning]
> - **`go` не загружает ELF.** Только `objcopy -O binary`; иначе первые байты — ELF-заголовок, и падение до UART.
> - **Родительский `.cargo/config.toml` подставит `-Tlinker.ld`.** Без `RUSTFLAGS` образ уедет на `0x42000000`.
> - **U-Boot выключает MMU и кэши перед `go`** — начинаем с выключенными; `dc civac` не нужен.
> - **Возврат — только перезагрузкой.** Свой `wfi`-цикл не отдаёт управление U-Boot.
> - **`saveenv` не работает** (env «from nowhere»), поэтому `ipaddr`/`serverip` — каждый раз заново.
> - **Не включать MMU на этом шаге.** Сначала три маркера, потом по одному: кэши, GIC, таймер.

## DoD

- `E2H1G` в консоли на 1500000.
- После power-cycle плата снова грузит OpenWrt (ничего не испорчено).
- QEMU-сборка этапа 5 не затронута: `probe/` — отдельный крейт.

## Дальше

H3 (сознательный fault с печатью ESR/FAR/ELR) → H4 (GIC + `CNTV`, `ticks`) → H5 (MMU) → H6 (`schedtest`). Порядок — [[board_bringup]].

## Связанные заметки
- [[board_bringup]], [[memory_map_rk3566]], [[h0_fact_sheet]], [[serial_console]], [[sd_card_image]], [[a64_assembly_basics]], [[exceptions]], [[gic_v3]], [[arm_generic_timer]], [[load_address]]
