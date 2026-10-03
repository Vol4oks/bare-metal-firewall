---
tags: [adr, решение, uart, вывод]
stage: 2
---

# ADR-0001: инфраструктура вывода до MMU

> [!abstract] Суть
> Порядок: сначала UART-модуль + `print!`/`println!` + печатающий `panic_handler`, затем этап 2 (MMU/heap). Причина: отладка таблиц страниц без печати — «тихие зависания»; вывод — мультипликатор скорости всех последующих этапов.

## Контекст
Hello world работает в QEMU; плата недоступна ([[qemu_adaptation]]). Альтернативы: сразу MMU/heap (B) или исключения/GIC (C). Отклонены: без `println!` обе темы отлаживаются вслепую.

## Решение
1. `src/uart.rs`: `struct Pl011`, опросный вывод через `volatile`, `impl core::fmt::Write`.
2. Макросы `print!`/`println!` в `main.rs`.
3. `panic_handler`: печать message + location, затем `wfi`.
4. API — под будущий SpinLock: [[multicore_path]]; trait-граница — под [[board_abstraction]].

## Последствия
- + быстрая отладка этапов 2–5;
- + раннее освоение no_std-приёмов (volatile, трейты, макросы);
- − этап 2 сдвигается на ~1 вечер;
- − UART без блокировок до этапа 4 (допустимо при [[multicore_path]]).

## Связанные заметки
- [[qemu_adaptation]], [[multicore_path]], [[board_abstraction]]
