use crate::llvm::layouts::Layouts;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::{
    ArrayType, BasicType, BasicTypeEnum, FloatType, IntType, PointerType,
    StructType, VoidType,
};
use inkwell::values::FunctionValue;
use inkwell::{context, AddressSpace};
use std::mem::size_of;
use types::{Block, Database, ForeignType, MethodId, TypeId, TypeRef};

/// A wrapper around an LLVM Context that provides some additional methods.
pub(crate) struct Context {
    pub(crate) inner: context::Context,
}

impl Context {
    pub(crate) fn new() -> Self {
        Self { inner: context::Context::create() }
    }

    pub(crate) fn pointer_type(&self) -> PointerType<'_> {
        self.inner.i8_type().ptr_type(AddressSpace::default())
    }

    pub(crate) fn bool_type(&self) -> IntType {
        self.inner.bool_type()
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
        methods: usize,
        name: &str,
        method_type: StructType<'a>,
    ) -> StructType<'a> {
        let name_type = self.rust_string_type();
        let class_type = self.inner.opaque_struct_type(name);

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
                // (using `getelementptr`) that are out of bounds. This is
                // necessary for dynamic dispatch as we don't statically know
                // the number of methods a receiver's class has.
                method_type.array_type(methods as _).into(),
            ],
            false,
        );
        class_type
    }

    /// Returns the layout for a built-in type such as Int or String (i.e a type
    /// with only a single value field).
    pub(crate) fn builtin_type<'a>(
        &'a self,
        name: &str,
        header: StructType<'a>,
        value: BasicTypeEnum,
    ) -> StructType<'a> {
        let typ = self.opaque_struct(name);

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

    pub(crate) fn foreign_type<'a>(
        &'a self,
        db: &Database,
        layouts: &Layouts<'a>,
        type_ref: TypeRef,
    ) -> Option<BasicTypeEnum<'a>> {
        let space = AddressSpace::default();

        match type_ref {
            TypeRef::Owned(id) => self
                .foreign_base_type(db, layouts, id)
                .map(|v| v.as_basic_type_enum()),
            TypeRef::Pointer(id) => self
                .foreign_base_type(db, layouts, id)
                .map(|v| v.ptr_type(space).as_basic_type_enum()),
            _ => None,
        }
    }

    pub(crate) fn return_type<'a>(
        &'a self,
        db: &Database,
        layouts: &Layouts<'a>,
        method: MethodId,
    ) -> Option<BasicTypeEnum<'a>> {
        let ret = method.return_type(db);

        self.foreign_type(db, layouts, ret).map(Some).unwrap_or_else(|| {
            if method.returns_value(db) {
                Some(self.pointer_type().as_basic_type_enum())
            } else {
                None
            }
        })
    }

    fn foreign_base_type<'a>(
        &'a self,
        db: &Database,
        layouts: &Layouts<'a>,
        id: TypeId,
    ) -> Option<BasicTypeEnum<'a>> {
        match id {
            TypeId::Foreign(ForeignType::Int(size)) => {
                let typ = match size {
                    8 => self.i8_type(),
                    16 => self.i16_type(),
                    32 => self.i32_type(),
                    _ => self.i64_type(),
                };

                Some(typ.as_basic_type_enum())
            }
            TypeId::Foreign(ForeignType::Float(size)) => {
                let typ = match size {
                    32 => self.f32_type(),
                    _ => self.f64_type(),
                };

                Some(typ.as_basic_type_enum())
            }
            TypeId::ClassInstance(ins)
                if ins.instance_of().kind(db).is_extern() =>
            {
                Some(layouts.instances[&ins.instance_of()].as_basic_type_enum())
            }
            _ => None,
        }
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
