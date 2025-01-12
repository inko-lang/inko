use crate::llvm::context::{size_in_bits, Context};
use crate::llvm::layouts::{ArgumentType, ReturnType};
use inkwell::targets::TargetData;
use inkwell::types::{BasicType, StructType};

pub(crate) fn struct_argument<'ctx>(
    ctx: &'ctx Context,
    tdata: &TargetData,
    typ: StructType<'ctx>,
) -> ArgumentType<'ctx> {
    let bytes = tdata.get_abi_size(&typ) as u32;

    if bytes <= 8 {
        let bits = ctx.custom_int(size_in_bits(bytes));

        ArgumentType::Regular(bits.as_basic_type_enum())
    } else if bytes <= 16 {
        ArgumentType::Regular(
            ctx.binary_struct(tdata, typ).as_basic_type_enum(),
        )
    } else {
        ArgumentType::StructValue(typ)
    }
}

pub(crate) fn struct_return<'ctx>(
    ctx: &'ctx Context,
    tdata: &TargetData,
    typ: StructType<'ctx>,
) -> ReturnType<'ctx> {
    let bytes = tdata.get_abi_size(&typ) as u32;

    if bytes <= 8 {
        let bits = ctx.custom_int(size_in_bits(bytes));

        ReturnType::Regular(bits.as_basic_type_enum())
    } else if bytes <= 16 {
        ReturnType::Regular(ctx.binary_struct(tdata, typ).as_basic_type_enum())
    } else {
        ReturnType::Struct(typ)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::targets::{
        CodeModel, InitializationConfig, RelocMode, Target, TargetMachine,
        TargetTriple,
    };
    use inkwell::types::BasicTypeEnum;
    use inkwell::OptimizationLevel;

    fn setup() -> TargetMachine {
        Target::initialize_x86(&InitializationConfig::default());

        let triple = TargetTriple::create("x86_64-unknown-linux-gnu");

        Target::from_triple(&triple)
            .unwrap()
            .create_target_machine(
                &triple,
                "",
                "",
                OptimizationLevel::None,
                RelocMode::PIC,
                CodeModel::Default,
            )
            .unwrap()
    }

    #[test]
    fn test_struct_argument_with_scalar() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let int8 = ctx.i8_type().as_basic_type_enum();
        let int16 = ctx.i16_type().as_basic_type_enum();
        let int32 = ctx.i32_type().as_basic_type_enum();
        let int64 = ctx.i64_type().as_basic_type_enum();
        let tests = [
            (vec![ctx.i8_type().into()], ArgumentType::Regular(int8)),
            (vec![ctx.i16_type().into()], ArgumentType::Regular(int16)),
            (vec![ctx.i32_type().into()], ArgumentType::Regular(int32)),
            (vec![ctx.i64_type().into()], ArgumentType::Regular(int64)),
            (
                vec![ctx.i32_type().into(), ctx.i32_type().into()],
                ArgumentType::Regular(int64),
            ),
        ];

        for (in_fields, exp) in tests {
            let inp = ctx.struct_type(in_fields.as_slice());

            assert_eq!(struct_argument(&ctx, &tdata, inp), exp);
        }
    }

    #[test]
    fn test_struct_argument_sixteen_bytes() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let int8 = ctx.i8_type().as_basic_type_enum();
        let int16 = ctx.i16_type().as_basic_type_enum();
        let int32 = ctx.i32_type().as_basic_type_enum();
        let int64 = ctx.i64_type().as_basic_type_enum();
        let f32 = ctx.f32_type().as_basic_type_enum();
        let f64 = ctx.f64_type().as_basic_type_enum();
        let tests = [
            (vec![int8; 16], vec![int64; 2]),
            (vec![int32; 4], vec![int64; 2]),
            (vec![int32, int16, int16, int64], vec![int64; 2]),
            (vec![int64, int8], vec![int64, int64]),
            (vec![int64, int32], vec![int64, int64]),
            (vec![int32, int32, int32], vec![int64, int32]),
            (vec![f32, f32, f32, f32], vec![f64, f64]),
            (vec![f64, f32, f32], vec![f64, f64]),
        ];

        for (in_fields, exp) in tests {
            let inp = ctx.struct_type(in_fields.as_slice());
            let ArgumentType::Regular(BasicTypeEnum::StructType(out)) =
                struct_argument(&ctx, &tdata, inp)
            else {
                panic!("expected a struct")
            };

            assert_eq!(out.get_field_types(), exp);
        }
    }

    #[test]
    fn test_struct_argument_large() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let inp = ctx.struct_type(&[
            ctx.i64_type().into(),
            ctx.i64_type().into(),
            ctx.i64_type().into(),
        ]);
        let out = struct_argument(&ctx, &tdata, inp);

        assert_eq!(out, ArgumentType::StructValue(inp));
    }

    #[test]
    fn test_struct_return_with_scalar() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let int8 = ctx.i8_type().as_basic_type_enum();
        let int16 = ctx.i16_type().as_basic_type_enum();
        let int32 = ctx.i32_type().as_basic_type_enum();
        let int64 = ctx.i64_type().as_basic_type_enum();
        let tests = [
            (vec![ctx.i8_type().into()], ReturnType::Regular(int8)),
            (vec![ctx.i16_type().into()], ReturnType::Regular(int16)),
            (vec![ctx.i32_type().into()], ReturnType::Regular(int32)),
            (vec![ctx.i64_type().into()], ReturnType::Regular(int64)),
            (
                vec![ctx.i32_type().into(), ctx.i32_type().into()],
                ReturnType::Regular(int64),
            ),
        ];

        for (in_fields, exp) in tests {
            let inp = ctx.struct_type(in_fields.as_slice());

            assert_eq!(struct_return(&ctx, &tdata, inp), exp);
        }
    }

    #[test]
    fn test_struct_return_sixteen_bytes() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let int8 = ctx.i8_type().as_basic_type_enum();
        let int16 = ctx.i16_type().as_basic_type_enum();
        let int32 = ctx.i32_type().as_basic_type_enum();
        let int64 = ctx.i64_type().as_basic_type_enum();
        let f32 = ctx.f32_type().as_basic_type_enum();
        let f64 = ctx.f64_type().as_basic_type_enum();
        let tests = [
            (vec![int8; 16], vec![int64; 2]),
            (vec![int32; 4], vec![int64; 2]),
            (vec![int32, int16, int16, int64], vec![int64; 2]),
            (vec![int64, int8], vec![int64, int64]),
            (vec![int64, int32], vec![int64, int64]),
            (vec![int32, int32, int32], vec![int64, int32]),
            (vec![f32, f32, f32, f32], vec![f64, f64]),
            (vec![f64, f32, f32], vec![f64, f64]),
        ];

        for (in_fields, exp) in tests {
            let inp = ctx.struct_type(in_fields.as_slice());
            let ReturnType::Regular(BasicTypeEnum::StructType(out)) =
                struct_return(&ctx, &tdata, inp)
            else {
                panic!("expected a struct")
            };

            assert_eq!(out.get_field_types(), exp);
        }
    }

    #[test]
    fn test_struct_return_large() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let inp = ctx.struct_type(&[
            ctx.i64_type().into(),
            ctx.i64_type().into(),
            ctx.i64_type().into(),
        ]);
        let out = struct_return(&ctx, &tdata, inp);

        assert_eq!(out, ReturnType::Struct(inp));
    }
}
