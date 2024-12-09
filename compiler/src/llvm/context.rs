use crate::llvm::layouts::{ArgumentType, Layouts, ReturnType};
use crate::state::State;
use crate::target::Architecture;
use inkwell::attributes::Attribute;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::{
    AnyTypeEnum, ArrayType, BasicType, BasicTypeEnum, FloatType, IntType,
    PointerType, StructType, VoidType,
};
use inkwell::values::FunctionValue;
use inkwell::{context, AddressSpace};
use std::cmp::max;
use std::mem::size_of;
use types::{
    Block, Database, ForeignType, MethodId, TypeId, TypeRef, BOOL_ID, FLOAT_ID,
    INT_ID, NIL_ID,
};

fn size_in_bits(bytes: u32) -> u32 {
    // LLVM crashes when passing/returning zero sized types (e.g. structs). In
    // addition, such types aren't useful in Inko, so we enforce a minimum size
    // of 1 bit.
    max(1, bytes * 8)
}

/// A wrapper around an LLVM Context that provides some additional methods.
pub(crate) struct Context {
    pub(crate) inner: context::Context,
}

impl Context {
    pub(crate) fn new() -> Self {
        Self { inner: context::Context::create() }
    }

    pub(crate) fn type_attribute(
        &self,
        name: &str,
        typ: AnyTypeEnum,
    ) -> Attribute {
        let id = Attribute::get_named_enum_kind_id(name);

        self.inner.create_type_attribute(id, typ)
    }

    pub(crate) fn flag(&self, name: &str) -> Attribute {
        let id = Attribute::get_named_enum_kind_id(name);

        self.inner.create_enum_attribute(id, 0)
    }

    pub(crate) fn pointer_type(&self) -> PointerType<'_> {
        self.inner.ptr_type(AddressSpace::default())
    }

    pub(crate) fn bool_type(&self) -> IntType {
        self.inner.bool_type()
    }

    pub(crate) fn custom_int(&self, bits: u32) -> IntType {
        self.inner.custom_width_int_type(bits)
    }

    pub(crate) fn i8_type(&self) -> IntType {
        self.inner.i8_type()
    }

    pub(crate) fn i16_type(&self) -> IntType {
        self.inner.i16_type()
    }

    pub(crate) fn i32_type(&self) -> IntType {
        self.inner.i32_type()
    }

    pub(crate) fn i64_type(&self) -> IntType {
        self.inner.i64_type()
    }

    pub(crate) fn f32_type(&self) -> FloatType {
        self.inner.f32_type()
    }

    pub(crate) fn f64_type(&self) -> FloatType {
        self.inner.f64_type()
    }

    pub(crate) fn void_type(&self) -> VoidType {
        self.inner.void_type()
    }

    pub(crate) fn rust_string_type(&self) -> ArrayType<'_> {
        self.inner.i8_type().array_type(size_of::<String>() as u32)
    }

    pub(crate) fn rust_vec_type(&self) -> ArrayType<'_> {
        self.inner.i8_type().array_type(size_of::<Vec<()>>() as u32)
    }

    pub(crate) fn opaque_struct<'a>(&'a self, name: &str) -> StructType<'a> {
        self.inner.opaque_struct_type(name)
    }

    pub(crate) fn struct_type<'a>(
        &'a self,
        fields: &[BasicTypeEnum],
    ) -> StructType<'a> {
        self.inner.struct_type(fields, false)
    }

    pub(crate) fn class_type<'a>(
        &'a self,
        method_type: StructType<'a>,
    ) -> StructType<'a> {
        let name_type = self.rust_string_type();
        let class_type = self.inner.opaque_struct_type("");

        class_type.set_body(
            &[
                // Name
                name_type.into(),
                // Instance size
                self.inner.i32_type().into(),
                // Number of methods
                self.inner.i16_type().into(),
                // The method table entries. We use an array instead of one
                // field per method, as this allows us to generate indexes
                // (using `getelementptr`) that are out of bounds.
                method_type.array_type(0).into(),
            ],
            false,
        );
        class_type
    }

    /// Returns the layout for a built-in type such as Int or String (i.e a type
    /// with only a single value field).
    pub(crate) fn builtin_type<'a>(
        &'a self,
        header: StructType<'a>,
        value: BasicTypeEnum,
    ) -> StructType<'a> {
        let typ = self.opaque_struct("");

        typ.set_body(&[header.into(), value], false);
        typ
    }

    pub(crate) fn append_basic_block<'a>(
        &'a self,
        function: FunctionValue<'a>,
    ) -> BasicBlock<'a> {
        self.inner.append_basic_block(function, "")
    }

    pub(crate) fn create_builder(&self) -> Builder {
        self.inner.create_builder()
    }

    pub(crate) fn create_module(&self, name: &str) -> Module {
        self.inner.create_module(name)
    }

    pub(crate) fn llvm_type<'a>(
        &'a self,
        db: &Database,
        layouts: &Layouts<'a>,
        type_ref: TypeRef,
    ) -> BasicTypeEnum<'a> {
        if let TypeRef::Pointer(_) = type_ref {
            return self.pointer_type().as_basic_type_enum();
        }

        let Ok(id) = type_ref.type_id(db) else {
            return self.pointer_type().as_basic_type_enum();
        };

        match id {
            TypeId::Foreign(ForeignType::Int(8, _)) => {
                self.i8_type().as_basic_type_enum()
            }
            TypeId::Foreign(ForeignType::Int(16, _)) => {
                self.i16_type().as_basic_type_enum()
            }
            TypeId::Foreign(ForeignType::Int(32, _)) => {
                self.i32_type().as_basic_type_enum()
            }
            TypeId::Foreign(ForeignType::Int(_, _)) => {
                self.i64_type().as_basic_type_enum()
            }
            TypeId::Foreign(ForeignType::Float(32)) => {
                self.f32_type().as_basic_type_enum()
            }
            TypeId::Foreign(ForeignType::Float(_)) => {
                self.f64_type().as_basic_type_enum()
            }
            TypeId::ClassInstance(ins) => {
                let cls = ins.instance_of();

                match cls.0 {
                    BOOL_ID | NIL_ID => self.bool_type().as_basic_type_enum(),
                    INT_ID => self.i64_type().as_basic_type_enum(),
                    FLOAT_ID => self.f64_type().as_basic_type_enum(),
                    _ if cls.is_stack_allocated(db) => {
                        layouts.instances[cls.0 as usize].as_basic_type_enum()
                    }
                    _ => self.pointer_type().as_basic_type_enum(),
                }
            }
            _ => self.pointer_type().as_basic_type_enum(),
        }
    }

    pub(crate) fn argument_type<'ctx>(
        &'ctx self,
        state: &State,
        layouts: &Layouts<'ctx>,
        typ: BasicTypeEnum<'ctx>,
    ) -> ArgumentType<'ctx> {
        let BasicTypeEnum::StructType(typ) = typ else {
            return ArgumentType::Regular(typ);
        };
        let bytes = layouts.target_data.get_abi_size(&typ) as u32;

        match state.config.target.arch {
            Architecture::Amd64 => {
                if bytes <= 8 {
                    let bits = self.custom_int(size_in_bits(bytes));

                    ArgumentType::Regular(bits.as_basic_type_enum())
                } else if bytes <= 16 {
                    ArgumentType::Regular(
                        self.abi_struct_type(layouts, typ).as_basic_type_enum(),
                    )
                } else {
                    ArgumentType::StructValue(typ)
                }
            }
            Architecture::Arm64 => {
                if bytes <= 8 {
                    ArgumentType::Regular(self.i64_type().as_basic_type_enum())
                } else if bytes <= 16 {
                    let word = self.i64_type().into();
                    let sret = self.struct_type(&[word, word]);

                    ArgumentType::Regular(sret.as_basic_type_enum())
                } else {
                    // clang and Rust don't use "byval" for ARM64 when the
                    // struct is too large, so neither do we.
                    ArgumentType::Pointer
                }
            }
        }
    }

    pub(crate) fn method_return_type<'ctx>(
        &'ctx self,
        state: &State,
        layouts: &Layouts<'ctx>,
        method: MethodId,
    ) -> ReturnType<'ctx> {
        if method.returns_value(&state.db) {
            let typ = self.llvm_type(
                &state.db,
                layouts,
                method.return_type(&state.db),
            );

            self.return_type(state, layouts, typ)
        } else {
            ReturnType::None
        }
    }

    pub(crate) fn return_type<'ctx>(
        &'ctx self,
        state: &State,
        layouts: &Layouts<'ctx>,
        typ: BasicTypeEnum<'ctx>,
    ) -> ReturnType<'ctx> {
        let BasicTypeEnum::StructType(typ) = typ else {
            return ReturnType::Regular(typ);
        };

        let bytes = layouts.target_data.get_abi_size(&typ) as u32;

        match state.config.target.arch {
            Architecture::Amd64 => {
                if bytes <= 8 {
                    let bits = self.custom_int(size_in_bits(bytes));

                    ReturnType::Regular(bits.as_basic_type_enum())
                } else if bytes <= 16 {
                    ReturnType::Regular(
                        self.abi_struct_type(layouts, typ).as_basic_type_enum(),
                    )
                } else {
                    ReturnType::Struct(typ)
                }
            }
            Architecture::Arm64 => {
                if bytes <= 8 {
                    let bits = self.custom_int(size_in_bits(bytes));

                    ReturnType::Regular(bits.as_basic_type_enum())
                } else if bytes <= 16 {
                    let word = self.i64_type().into();
                    let sret = self.struct_type(&[word, word]);

                    ReturnType::Regular(sret.as_basic_type_enum())
                } else {
                    ReturnType::Struct(typ)
                }
            }
        }
    }

    fn abi_struct_type<'ctx>(
        &'ctx self,
        layouts: &Layouts<'ctx>,
        typ: StructType<'ctx>,
    ) -> StructType<'ctx> {
        // We need to ensure each field is treated as a single/whole value,
        // otherwise (e.g. if a field is an array of bytes) we may end up
        // generating code that's not ABI compliant.
        let fields: Vec<_> = typ
            .get_field_types_iter()
            .map(|t| {
                let bits =
                    size_in_bits(layouts.target_data.get_abi_size(&t) as u32);

                self.custom_int(bits).as_basic_type_enum()
            })
            .collect();

        self.struct_type(&fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_type_sizes() {
        let ctx = Context::new();

        // These tests exists just to make sure the layouts match that which the
        // runtime expects. This would only ever fail if Rust suddenly changes
        // the layout of String/Vec.
        assert_eq!(ctx.rust_string_type().len(), 24);
        assert_eq!(ctx.rust_vec_type().len(), 24);
    }
}
