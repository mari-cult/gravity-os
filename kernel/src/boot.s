.section .text.boot
.global _start

_start:
    /* Read CPU ID, stop custom cores */
    mrs x0, mpidr_el1
    and x0, x0, #3
    cbz x0, master
    b   hang

master:
    /* Initialize BSS */
    ldr x0, =__bss_start
    ldr x1, =__bss_end
    sub x1, x1, x0
    cbz x1, run_kernel
    
    /* Zero out BSS */
zero_bss:
    str xzr, [x0], #8
    sub x1, x1, #8
    cbnz x1, zero_bss

run_kernel:
    /* Jump to Rust code */
    /* Set stack pointer before jump */
    ldr x0, =_start
    mov sp, x0
    bl  kmain

hang:
    wfe
    b   hang
