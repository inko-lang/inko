//! VM instruction handlers for setting literal values.
use compiled_code::CompiledCodePointer;
use process::RcProcess;
use vm::instruction::Instruction;

/// Sets a literal value in a register.
///
/// This instruction requires two arguments:
///
/// 1. The register to store the literal value in.
/// 2. The index to the value in the literals table of the current compiled code
///    object.
#[inline(always)]
pub fn set_literal(process: &RcProcess,
                   code: &CompiledCodePointer,
                   instruction: &Instruction) {
    let register = instruction.arg(0);
    let index = instruction.arg(1);

    process.set_register(register, code.literal(index));
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm::instructions::test::*;
    use vm::instruction::InstructionType;

    #[test]
    fn test_set_literal() {
        let (_machine, mut block, process) = setup();
        let instruction = new_instruction(InstructionType::SetLiteral,
                                          vec![0, 0]);

        block.code.literals.push(ObjectPointer::integer(10));

        set_literal(&process, &block.code, &instruction);

        let pointer = process.get_register(0);

        assert_eq!(pointer.integer_value().unwrap(), 10);
    }
}
