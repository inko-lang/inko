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
    pub(crate) fn to_llvm_type(self, context: &Context) -> BasicTypeEnum {
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
        BasicTypeEnum::VectorType(_) => {
            panic!("vector types are not yet supported")
        }
    }
}

pub(crate) fn combine_classes(
    classes: Vec<Class>,
    align: u64,
) -> (Class, Class) {
    let mut a = 0;
    let mut a_float = true;
    let mut b = 0;
    let mut b_float = true;

    for cls in classes {
        match cls {
            Class::Int(v) if a + v <= 8 => {
                a += v;
                a_float = false;
            }
            Class::Float(v) if a + v <= 8 => a += v,
            Class::Int(v) => {
                b += v;
                b_float = false;
            }
            Class::Float(v) => b += v,
        }
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
    fn test_context_type_sizes() {
        let ctx = Context::new();

        // These tests exists just to make sure the layouts match that which the
        // runtime expects. This would only ever fail if Rust suddenly changes
        // the layout of String/Vec.
        assert_eq!(ctx.rust_string_type().len(), 24);
        assert_eq!(ctx.rust_vec_type().len(), 24);
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
    fn test_combine_classes() {
        assert_eq!(
            combine_classes(vec![Class::Float(4), Class::Float(4)], 4),
            (Class::Float(8), Class::Float(4))
        );
        assert_eq!(
            combine_classes(
                vec![Class::Float(4), Class::Float(4), Class::Int(4)],
                4
            ),
            (Class::Float(8), Class::Int(4))
        );
        assert_eq!(
            combine_classes(
                vec![
                    Class::Float(4),
                    Class::Float(4),
                    Class::Int(4),
                    Class::Int(4)
                ],
                4
            ),
            (Class::Float(8), Class::Int(8))
        );
        assert_eq!(
            combine_classes(
                vec![
                    Class::Float(4),
                    Class::Int(4),
                    Class::Int(4),
                    Class::Int(4)
                ],
                4
            ),
            (Class::Int(8), Class::Int(8))
        );
        assert_eq!(
            combine_classes(
                vec![
                    Class::Int(4),
                    Class::Float(4),
                    Class::Int(4),
                    Class::Int(4)
                ],
                4
            ),
            (Class::Int(8), Class::Int(8))
        );
        assert_eq!(
            combine_classes(
                vec![Class::Int(1), Class::Int(4), Class::Int(8)],
                8
            ),
            (Class::Int(8), Class::Int(8))
        );
    }
}
