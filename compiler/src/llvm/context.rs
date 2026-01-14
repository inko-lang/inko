use crate::llvm::abi::amd64;
use crate::llvm::abi::arm64;
use crate::llvm::abi::generic::{classify, combine_classes};
use crate::llvm::layouts::{ArgumentType, Layouts, ReturnType};
use crate::state::State;
use crate::target::Architecture;
use inkwell::attributes::Attribute;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::support::get_llvm_version;
use inkwell::targets::TargetData;
use inkwell::types::{
    AnyTypeEnum, ArrayType, BasicType, BasicTypeEnum, FloatType, IntType,
    PointerType, StructType, VoidType,
};
use inkwell::values::{
    ArrayValue, FloatValue, FunctionValue, IntValue, PointerValue, StructValue,
};
use inkwell::{context, AddressSpace};
use std::cmp::max;
use types::{
    Block, Database, ForeignType, MethodId, TypeEnum, TypeRef, BOOL_ID,
    FLOAT_ID, INT_ID, NIL_ID,
};

pub(crate) fn size_in_bits(bytes: u32) -> u32 {
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

    pub(crate) fn no_capture_flag(&self) -> Attribute {
        if get_llvm_version().0 < 21 {
            self.flag("nocapture")
        } else {
            // Starting with version 21 the "nocapture" flag is replaced with
            // "captures(none)". The flag value 0 translates to "none" so we can
            // reuse Context::flag() here.
            self.flag("captures")
        }
    }

    pub(crate) fn flag(&self, name: &str) -> Attribute {
        let id = Attribute::get_named_enum_kind_id(name);

        self.inner.create_enum_attribute(id, 0)
    }

    pub(crate) fn bytes_type(&self, size: usize) -> ArrayType<'_> {
        self.i8_type().array_type(size as _)
    }

    pub(crate) fn pointer_type(&self) -> PointerType<'_> {
        self.inner.ptr_type(AddressSpace::default())
    }

    pub(crate) fn bool_type(&self) -> IntType<'_> {
        self.inner.bool_type()
    }

    pub(crate) fn custom_int(&self, bits: u32) -> IntType<'_> {
        self.inner.custom_width_int_type(bits)
    }

    pub(crate) fn i8_type(&self) -> IntType<'_> {
        self.inner.i8_type()
    }

    pub(crate) fn i16_type(&self) -> IntType<'_> {
        self.inner.i16_type()
    }

    pub(crate) fn i32_type(&self) -> IntType<'_> {
        self.inner.i32_type()
    }

    pub(crate) fn i64_type(&self) -> IntType<'_> {
        self.inner.i64_type()
    }

    pub(crate) fn f32_type(&self) -> FloatType<'_> {
        self.inner.f32_type()
    }

    pub(crate) fn f64_type(&self) -> FloatType<'_> {
        self.inner.f64_type()
    }

    pub(crate) fn void_type(&self) -> VoidType<'_> {
        self.inner.void_type()
    }

    pub(crate) fn bool_literal(&self, value: bool) -> IntValue<'_> {
        self.bool_type().const_int(value as u64, false)
    }

    pub(crate) fn i64_literal(&self, value: i64) -> IntValue<'_> {
        self.inner.i64_type().const_int(value as _, false)
    }

    pub(crate) fn f64_literal(&self, value: f64) -> FloatValue<'_> {
        self.inner.f64_type().const_float(value)
    }

    pub(crate) fn null_terminated_string(
        &self,
        value: &[u8],
    ) -> (ArrayType<'_>, ArrayValue<'_>) {
        let val = self.inner.const_string(value, true);
        let typ = self.bytes_type(value.len() + 1);

        (typ, val)
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

    pub(crate) fn two_words(&self) -> StructType<'_> {
        let word = self.i64_type().into();

        self.inner.struct_type(&[word, word], false)
    }

    pub(crate) fn new_type<'a>(
        &'a self,
        method_type: StructType<'a>,
        methods: usize,
    ) -> StructType<'a> {
        let typ = self.inner.opaque_struct_type("");

        typ.set_body(
            &[
                // Name
                self.pointer_type().into(),
                // Instance size
                self.inner.i32_type().into(),
                // Number of methods
                self.inner.i16_type().into(),
                // The method table entries. We use an array instead of one
                // field per method, as this allows us to generate indexes
                // (using `getelementptr`) that are out of bounds.
                method_type.array_type(methods as _).into(),
            ],
            false,
        );
        typ
    }

    pub(crate) fn atomic_header<'a>(
        &'a self,
        layouts: &Layouts<'a>,
        typ: PointerValue<'a>,
    ) -> StructValue<'a> {
        layouts.header.const_named_struct(&[
            typ.into(),
            self.i32_type().const_int(1, false).into(),
        ])
    }

    pub(crate) fn header<'a>(
        &'a self,
        layouts: &Layouts<'a>,
        typ: PointerValue<'a>,
    ) -> StructValue<'a> {
        layouts.header.const_named_struct(&[
            typ.into(),
            self.i32_type().const_int(0, false).into(),
        ])
    }

    pub(crate) fn append_basic_block<'a>(
        &'a self,
        function: FunctionValue<'a>,
    ) -> BasicBlock<'a> {
        self.inner.append_basic_block(function, "")
    }

    pub(crate) fn create_builder(&self) -> Builder<'_> {
        self.inner.create_builder()
    }

    pub(crate) fn create_module(&self, name: &str) -> Module<'_> {
        self.inner.create_module(name)
    }

    pub(crate) fn llvm_type<'a>(
        &'a self,
        db: &Database,
        layouts: &Layouts<'a>,
        type_ref: TypeRef,
    ) -> BasicTypeEnum<'a> {
        if type_ref.is_pointer(db) {
            return self.pointer_type().as_basic_type_enum();
        }

        let Ok(id) = type_ref.as_type_enum(db) else {
            return self.pointer_type().as_basic_type_enum();
        };

        match id {
            TypeEnum::Foreign(ForeignType::Int(size, _)) => {
                self.custom_int(size).as_basic_type_enum()
            }
            TypeEnum::Foreign(ForeignType::Float(32)) => {
                self.f32_type().as_basic_type_enum()
            }
            TypeEnum::Foreign(ForeignType::Float(_)) => {
                self.f64_type().as_basic_type_enum()
            }
            TypeEnum::TypeInstance(ins) => {
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
        type_ref: TypeRef,
    ) -> ArgumentType<'ctx> {
        let raw = self.llvm_type(&state.db, layouts, type_ref);
        let BasicTypeEnum::StructType(typ) = raw else {
            return ArgumentType::Regular(raw);
        };
        let tdata = layouts.target_data;

        match state.config.target.arch {
            Architecture::Amd64 => amd64::struct_argument(self, tdata, typ),
            Architecture::Arm64 => arm64::struct_argument(self, tdata, typ),
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

            self.return_type(state, layouts.target_data, typ)
        } else {
            ReturnType::None
        }
    }

    pub(crate) fn return_type<'ctx>(
        &'ctx self,
        state: &State,
        tdata: &TargetData,
        typ: BasicTypeEnum<'ctx>,
    ) -> ReturnType<'ctx> {
        let BasicTypeEnum::StructType(typ) = typ else {
            return ReturnType::Regular(typ);
        };

        match state.config.target.arch {
            Architecture::Amd64 => amd64::struct_return(self, tdata, typ),
            Architecture::Arm64 => arm64::struct_return(self, tdata, typ),
        }
    }

    pub(crate) fn binary_struct<'ctx>(
        &'ctx self,
        target_data: &TargetData,
        typ: StructType<'ctx>,
    ) -> StructType<'ctx> {
        let mut classes = Vec::new();

        classify(target_data, typ.as_basic_type_enum(), &mut classes);

        let (a, b) = combine_classes(
            classes,
            target_data.get_abi_alignment(&typ) as u64,
        );

        self.struct_type(&[a.to_llvm_type(self), b.to_llvm_type(self)])
    }
}
