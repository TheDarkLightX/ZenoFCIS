.text
.p2align 4
.globl tier1_two_div
.type tier1_two_div,@function
tier1_two_div:
    movq %rdi, %rax
    xorl %edx, %edx
    divq %rsi
    movq %rax, %rcx
    movq %rdi, %rax
    xorl %edx, %edx
    divq %rsi
    rolq $17, %rdx
    xorq %rcx, %rdx
    movq %rdx, %rax
    retq
.size tier1_two_div, .-tier1_two_div

.p2align 4
.globl tier1_one_div
.type tier1_one_div,@function
tier1_one_div:
    movq %rdi, %rax
    xorl %edx, %edx
    divq %rsi
    rolq $17, %rdx
    xorq %rdx, %rax
    retq
.size tier1_one_div, .-tier1_one_div
.section .note.GNU-stack,"",@progbits
