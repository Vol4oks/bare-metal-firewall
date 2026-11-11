.section .vectors, "ax"

.macro SLOT handler
     b \handler
     .space 124
.endm

.align 11
.global irq_vectors
irq_vectors:
    // --- current EL, SP0 (0x000–0x180): заглушки ---
    SLOT unexpected
    SLOT unexpected
    SLOT unexpected
    SLOT unexpected

    // --- current EL, SPx (0x200–0x380) ---
    SLOT sync_handler_asm      // 0x200 Sync
    SLOT irq_handler_asm       // 0x280 IRQ
    SLOT unexpected            // 0x300 FIQ
    SLOT unexpected            // 0x380 SError

    // --- нижние EL, AArch64 (0x400–0x5FF) ---
    SLOT unexpected
    SLOT unexpected
    SLOT unexpected
    SLOT unexpected

    // --- нижние EL, AArch32 (0x600–0x7FF) ---
    SLOT unexpected
    SLOT unexpected
    SLOT unexpected
    SLOT unexpected

// --- Trampoline: общий код Sync-обработчика ---
// Договор (инвариант): x25=ELR, x26=ESR, x27=FAR временно живут
// в этих регистрах от слота до trampoline. Прерванный код их
// не ждёт, Rust-обработчик не возвращает управление.
sync_handler_asm:
    mrs x25, elr_el1
    mrs x26, esr_el1
    mrs x27, far_el1

    mov x0, x26
    mov x1, x27
    mov x2, x25

    bl sync_handler            // Rust-функция, -> ! , сюда не вернёмся

// --- IRQ trampoline ---
irq_handler_asm:
    // 31 GPR + ELR/SPSR = 272 байта, кратно 16
    sub sp, sp, #272

    stp x0,  x1,  [sp, #0]
    stp x2,  x3,  [sp, #16]
    stp x4,  x5,  [sp, #32]
    stp x6,  x7,  [sp, #48]
    stp x8,  x9,  [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    stp x30, xzr, [sp, #240]

    mrs x0, elr_el1
    mrs x1, spsr_el1
    stp x0, x1, [sp, #256]

    bl irq_handler              // Rust-функция, возвращается

    ldp x0, x1, [sp, #256]
    msr elr_el1, x0
    msr spsr_el1, x1

    ldp x0,  x1,  [sp, #0]
    ldp x2,  x3,  [sp, #16]
    ldp x4,  x5,  [sp, #32]
    ldp x6,  x7,  [sp, #48]
    ldp x8,  x9,  [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldp x30, xzr, [sp, #240]

    add sp, sp, #272
    eret
