// RDI: a pointer pointer to the new stack, replaced with the new stack.
// RSI: a pointer to a function to call.
// RDX: an extra data argument to pass to the function.
asm_func!(
    "inko_context_init",
    "
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    // Swap the stack pointers
    mov r8, [rdi]
    mov [rdi], rsp
    mov rsp, r8

    mov rdi, rdx
    call rsi
    "
);

// RDI: a pointer pointer to a stack to restore.
asm_func!(
    "inko_context_switch",
    "
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    // Swap the stack pointers
    mov r8, [rdi]
    mov [rdi], rsp
    mov rsp, r8

    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp

    ret
    "
);
