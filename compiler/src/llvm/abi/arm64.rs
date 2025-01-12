use crate::llvm::abi::generic::classify;
use crate::llvm::context::{size_in_bits, Context};
use crate::llvm::layouts::{ArgumentType, ReturnType};
use inkwell::targets::TargetData;
use inkwell::types::{BasicType, StructType};

pub(crate) fn struct_argument<'ctx>(
    ctx: &'ctx Context,
    tdata: &TargetData,
    typ: StructType<'ctx>,
) -> ArgumentType<'ctx> {
    if let Some(h) = homogeneous_struct(ctx, tdata, typ) {
        return ArgumentType::Regular(h.as_basic_type_enum());
    }

    let bytes = tdata.get_abi_size(&typ) as u32;

    if bytes <= 8 {
        return ArgumentType::Regular(ctx.i64_type().as_basic_type_enum());
    }

    if bytes <= 16 {
        ArgumentType::Regular(ctx.two_words().as_basic_type_enum())
    } else {
        // clang and Rust don't use "byval" for ARM64 when the struct is too
        // large, so neither do we.
        ArgumentType::Pointer
    }
}

pub(crate) fn struct_return<'ctx>(
    ctx: &'ctx Context,
    tdata: &TargetData,
    typ: StructType<'ctx>,
) -> ReturnType<'ctx> {
    let bytes = tdata.get_abi_size(&typ) as u32;

    if let Some(h) = homogeneous_struct(ctx, tdata, typ) {
        return ReturnType::Regular(h.as_basic_type_enum());
    }

    if bytes <= 8 {
        let bits = ctx.custom_int(size_in_bits(bytes));

        return ReturnType::Regular(bits.as_basic_type_enum());
    }

    if bytes <= 16 {
        ReturnType::Regular(ctx.two_words().as_basic_type_enum())
    } else {
        ReturnType::Struct(typ)
    }
}

pub(crate) fn homogeneous_struct<'ctx>(
    context: &'ctx Context,
    tdata: &TargetData,
    typ: StructType<'ctx>,
) -> Option<StructType<'ctx>> {
    let mut classes = Vec::new();

    classify(tdata, typ.as_basic_type_enum(), &mut classes);

    if classes.is_empty() || classes.len() > 4 {
        return None;
    }

    let first = classes[0];

    if classes.iter().all(|&c| c.is_float() && c == first) {
        let fields: Vec<_> =
            classes.into_iter().map(|c| c.to_llvm_type(context)).collect();

        Some(context.struct_type(&fields))
    } else {
        None
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
        Target::initialize_aarch64(&InitializationConfig::default());

        let triple = TargetTriple::create("aarch64-unknown-linux-gnu");

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
    fn test_struct_argument_with_homogeneous_struct() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let tests = [
            (vec![ctx.f32_type().into()], vec![ctx.f32_type().into()]),
            (vec![ctx.f32_type().into(); 2], vec![ctx.f32_type().into(); 2]),
            (vec![ctx.f32_type().into(); 3], vec![ctx.f32_type().into(); 3]),
            (vec![ctx.f32_type().into(); 4], vec![ctx.f32_type().into(); 4]),
            (vec![ctx.f64_type().into()], vec![ctx.f64_type().into()]),
            (vec![ctx.f64_type().into(); 2], vec![ctx.f64_type().into(); 2]),
            (vec![ctx.f64_type().into(); 3], vec![ctx.f64_type().into(); 3]),
            (vec![ctx.f64_type().into(); 4], vec![ctx.f64_type().into(); 4]),
        ];

        for (in_fields, out_fields) in tests {
            let inp = ctx.struct_type(in_fields.as_slice());
            let ArgumentType::Regular(BasicTypeEnum::StructType(out)) =
                struct_argument(&ctx, &tdata, inp)
            else {
                panic!("expected a struct")
            };

            assert_eq!(out.get_field_types(), out_fields);
        }
    }

    #[test]
    fn test_struct_argument_with_scalar() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let int64 = ctx.i64_type().as_basic_type_enum();
        let tests = [
            (vec![ctx.i8_type().into()], ArgumentType::Regular(int64)),
            (vec![ctx.i16_type().into()], ArgumentType::Regular(int64)),
            (vec![ctx.i32_type().into()], ArgumentType::Regular(int64)),
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
        let inp =
            ctx.struct_type(&[ctx.i64_type().into(), ctx.i32_type().into()]);
        let ArgumentType::Regular(BasicTypeEnum::StructType(out)) =
            struct_argument(&ctx, &tdata, inp)
        else {
            panic!("expected a struct")
        };

        assert_eq!(
            out.get_field_types(),
            vec![ctx.i64_type().into(), ctx.i64_type().into()]
        );
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

        assert_eq!(struct_argument(&ctx, &tdata, inp), ArgumentType::Pointer);
    }

    #[test]
    fn test_struct_argument_mixed_floats() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let inp =
            ctx.struct_type(&[ctx.f64_type().into(), ctx.f32_type().into()]);
        let ArgumentType::Regular(BasicTypeEnum::StructType(out)) =
            struct_argument(&ctx, &tdata, inp)
        else {
            panic!("expected a struct")
        };

        assert_eq!(
            out.get_field_types(),
            vec![ctx.i64_type().into(), ctx.i64_type().into()]
        );
    }

    #[test]
    fn test_struct_return_with_homogeneous_struct() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let tests = [
            (vec![ctx.f32_type().into()], vec![ctx.f32_type().into()]),
            (vec![ctx.f32_type().into(); 2], vec![ctx.f32_type().into(); 2]),
            (vec![ctx.f32_type().into(); 3], vec![ctx.f32_type().into(); 3]),
            (vec![ctx.f32_type().into(); 4], vec![ctx.f32_type().into(); 4]),
            (vec![ctx.f64_type().into()], vec![ctx.f64_type().into()]),
            (vec![ctx.f64_type().into(); 2], vec![ctx.f64_type().into(); 2]),
            (vec![ctx.f64_type().into(); 3], vec![ctx.f64_type().into(); 3]),
            (vec![ctx.f64_type().into(); 4], vec![ctx.f64_type().into(); 4]),
        ];

        for (in_fields, out_fields) in tests {
            let inp = ctx.struct_type(in_fields.as_slice());
            let ReturnType::Regular(BasicTypeEnum::StructType(out)) =
                struct_return(&ctx, &tdata, inp)
            else {
                panic!("expected a struct")
            };

            assert_eq!(out.get_field_types(), out_fields);
        }
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
        let inp =
            ctx.struct_type(&[ctx.i64_type().into(), ctx.i32_type().into()]);
        let ReturnType::Regular(BasicTypeEnum::StructType(out)) =
            struct_return(&ctx, &tdata, inp)
        else {
            panic!("expected a struct")
        };

        assert_eq!(
            out.get_field_types(),
            vec![ctx.i64_type().into(), ctx.i64_type().into()]
        );
    }

    #[test]
    fn test_struct_return_large() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let int64 = ctx.i64_type().into();
        let inp = ctx.struct_type(&[int64, int64, int64]);
        let ReturnType::Struct(out) = struct_return(&ctx, &tdata, inp) else {
            panic!("expected a struct")
        };

        assert_eq!(out.get_field_types(), vec![int64, int64, int64]);
    }

    #[test]
    fn test_struct_return_mixed_floats() {
        let machine = setup();
        let tdata = machine.get_target_data();
        let ctx = Context::new();
        let inp =
            ctx.struct_type(&[ctx.f64_type().into(), ctx.f32_type().into()]);
        let ReturnType::Regular(BasicTypeEnum::StructType(out)) =
            struct_return(&ctx, &tdata, inp)
        else {
            panic!("expected a struct")
        };

        assert_eq!(
            out.get_field_types(),
            vec![ctx.i64_type().into(), ctx.i64_type().into()]
        );
    }
}
