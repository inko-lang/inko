// Taken from
// https://github.com/bytecodealliance/wasmtime/blob/95ecb7e4d440605165a74de607de5389508779be/crates/fiber/src/unix/aarch64.rs

// X0: a pointer pointer to the new stack, replaced with the new stack.
// X1: a pointer to a function to call.
// X2: an extra data argument to pass to the function.
asm_func!(
    "inko_context_init",
    "
    stp x29, x30, [sp, -16]!
    stp x20, x19, [sp, -16]!
    stp x22, x21, [sp, -16]!
    stp x24, x23, [sp, -16]!
    stp x26, x25, [sp, -16]!
    stp x28, x27, [sp, -16]!
    stp d9, d8, [sp, -16]!
    stp d11, d10, [sp, -16]!
    stp d13, d12, [sp, -16]!
    stp d15, d14, [sp, -16]!

    // Swap the stack pointers
    ldr x8, [x0]
    mov x9, sp
    str x9, [x0]
    mov sp, x8

    mov x0, x2
    blr x1
    "
);

// X0: a pointer pointer to a stack to restore.
asm_func!(
    "inko_context_switch",
    "
    stp x29, x30, [sp, -16]!
    stp x20, x19, [sp, -16]!
    stp x22, x21, [sp, -16]!
    stp x24, x23, [sp, -16]!
    stp x26, x25, [sp, -16]!
    stp x28, x27, [sp, -16]!
    stp d9, d8, [sp, -16]!
    stp d11, d10, [sp, -16]!
    stp d13, d12, [sp, -16]!
    stp d15, d14, [sp, -16]!

    // Swap the stack pointers
    ldr x8, [x0]
    mov x9, sp
    str x9, [x0]
    mov sp, x8

    ldp d15, d14, [sp], 16
    ldp d13, d12, [sp], 16
    ldp d11, d10, [sp], 16
    ldp d9, d8, [sp], 16
    ldp x28, x27, [sp], 16
    ldp x26, x25, [sp], 16
    ldp x24, x23, [sp], 16
    ldp x22, x21, [sp], 16
    ldp x20, x19, [sp], 16
    ldp x29, x30, [sp], 16

    ret
    "
);
