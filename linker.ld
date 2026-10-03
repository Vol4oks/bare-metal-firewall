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
    . = ALIGN(4096);
    __heap_start = .;
    . = . + 0x100000; /* 1 MB heap */
    __heap_end = .;
}
