ENTRY(_start)

SECTIONS
{
    . = 0x42000000;

    .text : { KEEP(*(.text._start)) *(.text .text.*) }
    .rodata : { *(.rodata .rodata.*) }
    .data : { *(.data .data.*) }
    .bss : {
        . = ALIGN(16);
        __bss_start = .;
        *(.bss .bss.*);
        __bss_end = .;
    }

    . = ALIGN(16);
    . = . + 0x10000; /* 64KB стек */
    stack_top = .;
}
