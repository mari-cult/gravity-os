SECTIONS
{
    . = 0x3FFFF000; /* Match load_offset in kernel */

    .text : {
        KEEP(*(.text.entry))
        *(.text*)
    }

    .rodata : {
        *(.rodata*)
    }

    .data : {
        *(.data*)
    }

    .bss : {
        *(.bss*)
        *(COMMON)
    }

    /DISCARD/ : {
        *(.comment)
        *(.note*)
    }
}
