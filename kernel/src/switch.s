.global __switch_to
.global kernel_thread_starter

__switch_to:
    /* x0 = prev_ctx, x1 = next_ctx */
    /* Save Callee-saved registers */
    str x19, [x0, #0]
    str x20, [x0, #8]
    str x21, [x0, #16]
    str x22, [x0, #24]
    str x23, [x0, #32]
    str x24, [x0, #40]
    str x25, [x0, #48]
    str x26, [x0, #56]
    str x27, [x0, #64]
    str x28, [x0, #72]
    str x29, [x0, #80]
    str x30, [x0, #88] /* LR */
    mov x19, sp
    str x19, [x0, #96] /* SP */

    /* Restore next context */
    ldr x19, [x1, #0]
    ldr x20, [x1, #8]
    ldr x21, [x1, #16]
    ldr x22, [x1, #24]
    ldr x23, [x1, #32]
    ldr x24, [x1, #40]
    ldr x25, [x1, #48]
    ldr x26, [x1, #56]
    ldr x27, [x1, #64]
    ldr x28, [x1, #72]
    ldr x29, [x1, #80]
    ldr x30, [x1, #88]
    ldr x9,  [x1, #96]
    mov sp, x9

    ret

/* 
 * void kernel_thread_starter(u64 user_entry, u64 user_stack);
 * This is where a new process starts execution in Kernel Mode after switch_to.
 * x19 = user_entry (saved in Process::new)
 * x20 = user_stack (saved in Process::new)
 */
kernel_thread_starter:
    msr elr_el1, x19
    msr sp_el0, x20
    
    /* Set argument x0 = PID (from x21) */
    mov x0, x21
    
    /* SPSR_EL1: EL0t, IRQ/FIQ masked */
    mov x1, #0x3c0
    msr spsr_el1, x1
    
    eret
