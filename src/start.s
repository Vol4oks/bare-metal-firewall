.section .text._start
.global _start
_start:
    // 0. Парковка вторичных ядер: работаем только на ядре 0 (MPIDR Aff0 == 0)
    mrs x0, mpidr_el1
    and x0, x0, #3
    cbnz x0, park

    // 0.1 Маскируем исключения: состояние после boot stub / U-Boot go не гарантировано
    msr daifset, #0xf

    // 1. Настройка стека
    ldr x0, =stack_top
    mov sp, x0

    // 2. Обнуление .bss (размер кратен 16 — гарантирует ALIGN в linker.ld)
    ldr x0, =__bss_start
    ldr x1, =__bss_end
    cmp x0, x1
    b.hs 2f
1:
    stp xzr, xzr, [x0], #16
    cmp x0, x1
    b.lo 1b
2:
    // 3. Вызов Rust-функции main
    bl main

park:
3:
    wfi
    b 3b
