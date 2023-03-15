// See the following resources for more details:
//
// - https://probablydance.com/2013/02/20/handmade-coroutines-for-windows
// - https://github.com/slembcke/Tina/blob/c0aad745d43b5265df952e89602e1a687a78ee65/extras/win-asm/win64-swap.S

// RCX: a pointer pointer to the low address of the new stack
// RDX: a pointer pointer to the high address of the new stack
// R8: a pointer to a function to call
// R9: an extra data argument to pass to the function.
asm_func!(
    "inko_context_init",
    "
    push qword ptr gs:[0x8] // Stack high address
    push qword ptr gs:[0x10] // Stack low address
    push qword ptr gs:[0x1478] // Deallocation stack

    push rbp
    push rbx
    push rdi
    push rsi
    push r12
    push r13
    push r14
    push r15

	sub rsp, 160
	movaps [rsp + 0x90], xmm6
	movaps [rsp + 0x80], xmm7
	movaps [rsp + 0x70], xmm8
	movaps [rsp + 0x60], xmm9
	movaps [rsp + 0x50], xmm10
	movaps [rsp + 0x40], xmm11
	movaps [rsp + 0x30], xmm12
	movaps [rsp + 0x20], xmm13
	movaps [rsp + 0x10], xmm14
	movaps [rsp + 0x00], xmm15

    // Swap the stack pointers
    mov r10, [rdx]
    mov [rdx], rsp
    mov rsp, r10

    mov qword ptr gs:[0x8], rdx
    mov qword ptr gs:[0x10], rcx
    mov qword ptr gs:[0x1478], rcx

    // Reserve shadow space
    sub rsp, 0x20

    mov rcx, r9
    call r8
    "
);

// RCX: a pointer pointer to a stack to restore.
asm_func!(
    "inko_context_switch",
    "
    push qword ptr gs:[0x8]
    push qword ptr gs:[0x10]
    push qword ptr gs:[0x1478]
    push rbp
    push rbx
    push rdi
    push rsi
    push r12
    push r13
    push r14
    push r15

	sub rsp, 160
	movaps [rsp + 0x90], xmm6
	movaps [rsp + 0x80], xmm7
	movaps [rsp + 0x70], xmm8
	movaps [rsp + 0x60], xmm9
	movaps [rsp + 0x50], xmm10
	movaps [rsp + 0x40], xmm11
	movaps [rsp + 0x30], xmm12
	movaps [rsp + 0x20], xmm13
	movaps [rsp + 0x10], xmm14
	movaps [rsp + 0x00], xmm15

    // Swap the stack pointers
    mov r8, [rcx]
    mov [rcx], rsp
    mov rsp, r8

	movaps xmm6, [rsp + 0x90]
	movaps xmm7, [rsp + 0x80]
	movaps xmm8, [rsp + 0x70]
	movaps xmm9, [rsp + 0x60]
	movaps xmm10, [rsp + 0x50]
	movaps xmm11, [rsp + 0x40]
	movaps xmm12, [rsp + 0x30]
	movaps xmm13, [rsp + 0x20]
	movaps xmm14, [rsp + 0x10]
	movaps xmm15, [rsp + 0x00]
    add rsp, 160

    pop r15
    pop r14
    pop r13
    pop r12
    pop rsi
    pop rdi
    pop rbx
    pop rbp

    pop qword ptr gs:[0x1478]
    pop qword ptr gs:[0x10]
    pop qword ptr gs:[0x8]

    ret
    "
);
