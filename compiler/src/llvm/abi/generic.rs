use crate::llvm::context::Context;
use inkwell::targets::TargetData;
use inkwell::types::{BasicType, BasicTypeEnum};
use std::cmp::max;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum Class {
    Int(u64),
    Float(u64),
}

impl Class {
    pub(crate) fn to_llvm_type(self, context: &Context) -> BasicTypeEnum<'_> {
        match self {
            Class::Int(bytes) => {
                context.custom_int(bytes as u32 * 8).as_basic_type_enum()
            }
            Class::Float(4) => context.f32_type().as_basic_type_enum(),
            Class::Float(_) => context.f64_type().as_basic_type_enum(),
        }
    }

    pub(crate) fn is_float(self) -> bool {
        matches!(self, Class::Float(_))
    }
}

pub(crate) fn classify(
    target_data: &TargetData,
    typ: BasicTypeEnum,
    classes: &mut Vec<Class>,
) {
    match typ {
        BasicTypeEnum::StructType(t) => {
            for field in t.get_field_types_iter() {
                classify(target_data, field, classes);
            }
        }
        BasicTypeEnum::ArrayType(t) => {
            let field = t.get_element_type();

            for _ in 0..t.len() {
                classify(target_data, field, classes);
            }
        }
        BasicTypeEnum::FloatType(t) => {
            classes.push(Class::Float(target_data.get_abi_size(&t)))
        }
        BasicTypeEnum::IntType(t) => {
            classes.push(Class::Int(target_data.get_abi_size(&t)))
        }
        BasicTypeEnum::PointerType(t) => {
            classes.push(Class::Int(target_data.get_abi_size(&t)))
        }
        BasicTypeEnum::VectorType(_) | BasicTypeEnum::ScalableVectorType(_) => {
            panic!("vector types are not yet supported")
        }
    }
}

pub(crate) fn single_word_class(classes: Vec<Class>) -> Class {
    let mut len = 0;
    let mut float = true;

    for cls in classes {
        match cls {
            Class::Int(v) => {
                len += v;
                float = false;
            }
            Class::Float(v) => len += v,
        }
    }

    if float {
        Class::Float(len)
    } else if len <= 2 {
        Class::Int(len)
    } else if len <= 4 {
        Class::Int(4)
    } else {
        Class::Int(8)
    }
}

pub(crate) fn combine_classes(
    classes: Vec<Class>,
    size: u64,
    align: u64,
) -> (Class, Class) {
    let mut a = 0;
    let mut a_float = true;
    let mut b = 0;
    let mut b_float = true;

    // The maximum size (in bytes) of a single slot.
    let limit = 8;

    for cls in classes {
        match cls {
            Class::Int(v) if a + v <= limit => {
                a += v;
                a_float = false;
            }
            Class::Int(v) if a < limit => {
                let diff = limit - a;

                a += diff;
                b += v - diff;
                a_float = false;
                b_float = false;
            }
            Class::Float(v) if a + v <= limit => a += v,
            Class::Float(v) if a < limit => {
                let diff = limit - a;

                a += diff;
                b += v - diff;
                a_float = false;
            }
            Class::Int(v) => {
                b += v;
                b_float = false;
            }
            Class::Float(v) => {
                b += v;
            }
        }
    }

    if b == 0 {
        b_float = false;
    }

    if size == 16 {
        b = limit;
    }

    a = max(a, align);
    b = max(b, align);

    (
        if a_float { Class::Float(a) } else { Class::Int(a) },
        if b_float { Class::Float(b) } else { Class::Int(b) },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::OptimizationLevel;
    use inkwell::targets::{
        CodeModel, InitializationConfig, RelocMode, Target, TargetMachine,
        TargetTriple,
    };
    use inkwell::types::BasicTypeEnum;

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
    fn test_class_to_llvm_type() {
        let ctx = Context::new();
        let tests = [
            (Class::Int(1), BasicTypeEnum::IntType(ctx.i8_type())),
            (Class::Int(2), BasicTypeEnum::IntType(ctx.i16_type())),
            (Class::Int(4), BasicTypeEnum::IntType(ctx.i32_type())),
            (Class::Int(8), BasicTypeEnum::IntType(ctx.i64_type())),
            (Class::Float(4), BasicTypeEnum::FloatType(ctx.f32_type())),
            (Class::Float(8), BasicTypeEnum::FloatType(ctx.f64_type())),
        ];

        for (cls, typ) in tests {
            assert_eq!(cls.to_llvm_type(&ctx), typ);
        }
    }

    #[test]
    fn test_class_is_float() {
        assert!(Class::Float(4).is_float());
        assert!(!Class::Int(4).is_float());
    }

    #[test]
    fn test_classify() {
        let machine = setup();
        let ctx = Context::new();
        let inp = ctx
            .struct_type(&[
                ctx.struct_type(&[ctx.i8_type().into()]).into(),
                ctx.i32_type().into(),
                ctx.struct_type(&[ctx.i64_type().into()]).array_type(2).into(),
            ])
            .as_basic_type_enum();
        let mut classes = Vec::new();

        classify(&machine.get_target_data(), inp, &mut classes);

        assert_eq!(
            classes,
            vec![Class::Int(1), Class::Int(4), Class::Int(8), Class::Int(8)]
        );
    }

    #[test]
    fn test_single_word_class() {
        let int8 = Class::Int(1);
        let int16 = Class::Int(2);
        let int32 = Class::Int(4);
        let int64 = Class::Int(8);
        let float32 = Class::Float(4);
        let float64 = Class::Float(8);
        let tests = [
            (vec![int8], int8),
            (vec![int16], int16),
            (vec![int32], int32),
            (vec![int64], int64),
            (vec![int8, int8], int16),
            (vec![int8, int8, int8], int32),
            (vec![int8, int8, int8, int8], int32),
            (vec![int8, int8, int8, int8, int8], int64),
            (vec![float32, int8], int64),
            (vec![float32, int16], int64),
            (vec![float32, int32], int64),
            (vec![float32, float32], float64),
            (vec![float64], float64),
        ];

        for (classes, exp) in tests {
            assert_eq!(single_word_class(classes), exp);
        }
    }

    #[test]
    fn test_combine_classes() {
        let int8 = Class::Int(1);
        let int16 = Class::Int(2);
        let int32 = Class::Int(4);
        let int64 = Class::Int(8);
        let float32 = Class::Float(4);
        let float64 = Class::Float(8);
        let tests = [
            (12, 4, vec![float32, float32, int32], (float64, int32)),
            (16, 4, vec![float32, float32, int32, int32], (float64, int64)),
            (16, 4, vec![float32, int32, int32, int32], (int64, int64)),
            (16, 4, vec![int32, float32, int32, int32], (int64, int64)),
            (16, 8, vec![int8, int32, int64], (int64, int64)),
            (16, 8, vec![int64, int8], (int64, int64)),
            (16, 8, vec![int64, int16], (int64, int64)),
            (16, 8, vec![int64, int32], (int64, int64)),
            (16, 8, vec![int64, int64], (int64, int64)),
            (12, 4, vec![int32, int32, int8], (int64, int32)),
            (12, 4, vec![int32, int32, int8, int8], (int64, int32)),
            (16, 4, vec![int8, int32, int32, int8], (int64, int64)),
            (16, 4, vec![int8, int32, int16, int16, int8], (int64, int64)),
            (12, 4, vec![int8, int32, int8, int8, int8], (int64, int32)),
            (12, 4, vec![int32, int32, int16], (int64, int32)),
            (
                16,
                4,
                vec![int32, int32, int8, int8, int8, int8, int8],
                (int64, int64),
            ),
            (16, 4, vec![int32, int8, int32, int8], (int64, int64)),
            (16, 4, vec![int32, int16, int32, int16], (int64, int64)),
            (16, 4, vec![int32, int32, int32, int32], (int64, int64)),
            (16, 8, vec![int32, float32, float64], (int64, float64)),
            (16, 8, vec![int16, float32, float64], (int64, float64)),
            (16, 8, vec![float64, int16], (float64, int64)),
            (16, 8, vec![float64, int16, float32], (float64, int64)),
            (12, 4, vec![float32, int16, float32], (int64, float32)),
            (
                12,
                1,
                vec![
                    int8, int8, int8, int8, int8, int8, int8, int8, int8, int8,
                    int8, int8,
                ],
                (int64, int32),
            ),
        ];

        for (size, align, classes, exp) in tests {
            assert_eq!(combine_classes(classes, size, align), exp);
        }
    }
}
