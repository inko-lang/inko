use crate::llvm::constants::{HEADER_CLASS_INDEX, HEADER_REFS_INDEX};
use crate::llvm::context::Context;
use crate::llvm::module::Module;
use crate::llvm::runtime_function::RuntimeFunction;
use inkwell::basic_block::BasicBlock;
use inkwell::builder;
use inkwell::debug_info::{
    debug_metadata_version, AsDIScope, DICompileUnit, DIFlags,
    DIFlagsConstants, DILocation, DIScope, DISubprogram, DWARFEmissionKind,
    DWARFSourceLanguage, DebugInfoBuilder,
};
use inkwell::module::{FlagBehavior, Module as InkwellModule};
use inkwell::types::{ArrayType, BasicType, FunctionType, StructType};
use inkwell::values::{
    AggregateValue, ArrayValue, BasicMetadataValueEnum, BasicValue,
    BasicValueEnum, CallSiteValue, FloatValue, FunctionValue,
    InstructionOpcode, IntValue, PointerValue,
};
use inkwell::{
    AddressSpace, AtomicOrdering, AtomicRMWBinOp, FloatPredicate, IntPredicate,
};
use std::path::Path;
use types::{ClassId, Database};

/// A wrapper around an LLVM Builder that provides some additional methods.
pub(crate) struct Builder<'ctx> {
    inner: builder::Builder<'ctx>,
    pub(crate) function: FunctionValue<'ctx>,
    pub(crate) context: &'ctx Context,
}

impl<'ctx> Builder<'ctx> {
    pub(crate) fn new(
        context: &'ctx Context,
        function: FunctionValue<'ctx>,
    ) -> Self {
        Self { inner: context.create_builder(), context, function }
    }

    pub(crate) fn argument(&self, index: u32) -> BasicValueEnum<'ctx> {
        self.function.get_nth_param(index).unwrap()
    }

    pub(crate) fn arguments(
        &self,
    ) -> impl Iterator<Item = BasicValueEnum<'ctx>> {
        self.function.get_param_iter()
    }

    pub(crate) fn extract_field<R: AggregateValue<'ctx>>(
        &self,
        receiver: R,
        index: u32,
    ) -> BasicValueEnum<'ctx> {
        self.inner.build_extract_value(receiver, index, "").unwrap()
    }

    pub(crate) fn load_field(
        &self,
        receiver_type: StructType<'ctx>,
        receiver: PointerValue<'ctx>,
        index: u32,
    ) -> BasicValueEnum<'ctx> {
        let vtype = receiver_type.get_field_type_at_index(index).unwrap();
        let field_ptr = self.field_address(receiver_type, receiver, index);

        self.inner.build_load(vtype, field_ptr, "").unwrap()
    }

    pub(crate) fn field_address(
        &self,
        receiver_type: StructType<'ctx>,
        receiver: PointerValue<'ctx>,
        index: u32,
    ) -> PointerValue<'ctx> {
        self.inner.build_struct_gep(receiver_type, receiver, index, "").unwrap()
    }

    pub(crate) fn array_field_index_address(
        &self,
        receiver_type: StructType<'ctx>,
        receiver: PointerValue<'ctx>,
        field: u32,
        index: IntValue<'ctx>,
    ) -> PointerValue<'ctx> {
        if !receiver_type
            .get_field_type_at_index(field)
            .map_or(false, |v| v.is_array_type())
        {
            // In practise we'll never reach this point, but the check exists
            // anyway to ensure this method doesn't segfault the compiler due to
            // an invalid `getelementptr` instruction.
            panic!("The field doesn't point to an array");
        }

        unsafe {
            self.inner
                .build_gep(
                    receiver_type,
                    receiver,
                    &[self.u32_literal(0), self.u32_literal(field), index],
                    "",
                )
                .unwrap()
        }
    }

    pub(crate) fn load_array_index(
        &self,
        array_type: ArrayType<'ctx>,
        array: PointerValue<'ctx>,
        index: usize,
    ) -> BasicValueEnum<'ctx> {
        let ptr = unsafe {
            self.inner
                .build_gep(
                    array_type,
                    array,
                    &[
                        self.context.i32_type().const_int(0, false),
                        self.context.i32_type().const_int(index as _, false),
                    ],
                    "",
                )
                .unwrap()
        };

        self.inner.build_load(array_type.get_element_type(), ptr, "").unwrap()
    }

    pub(crate) fn store_array_field<V: BasicValue<'ctx>>(
        &self,
        array_type: ArrayType<'ctx>,
        array: PointerValue<'ctx>,
        index: u32,
        value: V,
    ) {
        let ptr = unsafe {
            self.inner
                .build_gep(
                    array_type,
                    array,
                    &[
                        self.context.i32_type().const_int(0, false),
                        self.context.i32_type().const_int(index as _, false),
                    ],
                    "",
                )
                .unwrap()
        };

        self.store(ptr, value);
    }

    pub(crate) fn store_field<V: BasicValue<'ctx>>(
        &self,
        receiver_type: StructType<'ctx>,
        receiver: PointerValue<'ctx>,
        index: u32,
        value: V,
    ) {
        let field_ptr = self.field_address(receiver_type, receiver, index);

        self.store(field_ptr, value);
    }

    pub(crate) fn store<V: BasicValue<'ctx>>(
        &self,
        variable: PointerValue<'ctx>,
        value: V,
    ) {
        self.inner.build_store(variable, value).unwrap();
    }

    pub(crate) fn load<T: BasicType<'ctx>>(
        &self,
        typ: T,
        variable: PointerValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        self.inner.build_load(typ, variable, "").unwrap()
    }

    pub(crate) fn load_int(
        &self,
        variable: PointerValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner
            .build_load(self.context.i64_type(), variable, "")
            .unwrap()
            .into_int_value()
    }

    pub(crate) fn load_float(
        &self,
        variable: PointerValue<'ctx>,
    ) -> FloatValue<'ctx> {
        self.inner
            .build_load(self.context.f64_type(), variable, "")
            .unwrap()
            .into_float_value()
    }

    pub(crate) fn load_untyped_pointer(
        &self,
        variable: PointerValue<'ctx>,
    ) -> PointerValue<'ctx> {
        self.load(
            self.context.i8_type().ptr_type(AddressSpace::default()),
            variable,
        )
        .into_pointer_value()
    }

    pub(crate) fn load_pointer<T: BasicType<'ctx>>(
        &self,
        typ: T,
        variable: PointerValue<'ctx>,
    ) -> PointerValue<'ctx> {
        self.load(
            typ.as_basic_type_enum().ptr_type(AddressSpace::default()),
            variable,
        )
        .into_pointer_value()
    }

    pub(crate) fn load_function_pointer(
        &self,
        typ: FunctionType<'ctx>,
        variable: PointerValue<'ctx>,
    ) -> PointerValue<'ctx> {
        self.load(typ.ptr_type(AddressSpace::default()), variable)
            .into_pointer_value()
    }

    pub(crate) fn call(
        &self,
        function: FunctionValue<'ctx>,
        arguments: &[BasicMetadataValueEnum<'ctx>],
    ) -> BasicValueEnum<'ctx> {
        self.inner
            .build_call(function, arguments, "")
            .unwrap()
            .try_as_basic_value()
            .left()
            .unwrap()
    }

    pub(crate) fn indirect_call(
        &self,
        typ: FunctionType<'ctx>,
        func: PointerValue<'ctx>,
        args: &[BasicMetadataValueEnum<'ctx>],
    ) -> CallSiteValue<'ctx> {
        self.inner.build_indirect_call(typ, func, args, "").unwrap()
    }

    pub(crate) fn call_void(
        &self,
        function: FunctionValue<'ctx>,
        arguments: &[BasicMetadataValueEnum<'ctx>],
    ) {
        self.inner.build_call(function, arguments, "").unwrap();
    }

    pub(crate) fn pointer_to_int(
        &self,
        value: PointerValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_ptr_to_int(value, self.context.i64_type(), "").unwrap()
    }

    pub(crate) fn bool_literal(&self, value: bool) -> IntValue<'ctx> {
        self.context.bool_type().const_int(value as u64, false)
    }

    pub(crate) fn u8_literal(&self, value: u8) -> IntValue<'ctx> {
        self.context.i8_type().const_int(value as u64, false)
    }

    pub(crate) fn i64_literal(&self, value: i64) -> IntValue<'ctx> {
        self.u64_literal(value as u64)
    }

    pub(crate) fn u16_literal(&self, value: u16) -> IntValue<'ctx> {
        self.context.i16_type().const_int(value as u64, false)
    }

    pub(crate) fn u32_literal(&self, value: u32) -> IntValue<'ctx> {
        self.context.i32_type().const_int(value as u64, false)
    }

    pub(crate) fn u64_literal(&self, value: u64) -> IntValue<'ctx> {
        self.context.i64_type().const_int(value, false)
    }

    pub(crate) fn f64_literal(&self, value: f64) -> FloatValue<'ctx> {
        self.context.f64_type().const_float(value)
    }

    pub(crate) fn string_literal(
        &self,
        value: &str,
    ) -> (PointerValue<'ctx>, IntValue<'ctx>) {
        let string = self
            .inner
            .build_global_string_ptr(value, "")
            .unwrap()
            .as_pointer_value();

        let len = self.u64_literal(value.len() as _);

        (string, len)
    }

    pub(crate) fn atomic_add(
        &self,
        pointer: PointerValue<'ctx>,
        value: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner
            .build_atomicrmw(
                AtomicRMWBinOp::Add,
                pointer,
                value,
                AtomicOrdering::AcquireRelease,
            )
            .unwrap()
    }

    pub(crate) fn atomic_sub(
        &self,
        pointer: PointerValue<'ctx>,
        value: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner
            .build_atomicrmw(
                AtomicRMWBinOp::Sub,
                pointer,
                value,
                AtomicOrdering::AcquireRelease,
            )
            .unwrap()
    }

    pub(crate) fn load_atomic_counter(
        &self,
        variable: PointerValue<'ctx>,
    ) -> IntValue<'ctx> {
        let res = self
            .inner
            .build_load(self.context.i32_type(), variable, "")
            .unwrap();
        let ins = res.as_instruction_value().unwrap();

        // If the alignment doesn't match the value size, LLVM compiles this to
        // an __atomic_load() function call. For the sake of
        // clarity/future-proofing, we set the alignment explicitly, even though
        // this is technically redundant.
        ins.set_alignment(4).unwrap();
        ins.set_atomic_ordering(AtomicOrdering::Monotonic).unwrap();
        res.into_int_value()
    }

    pub(crate) fn int_eq(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_compare(IntPredicate::EQ, lhs, rhs, "").unwrap()
    }

    pub(crate) fn int_gt(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_compare(IntPredicate::SGT, lhs, rhs, "").unwrap()
    }

    pub(crate) fn int_ge(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_compare(IntPredicate::SGE, lhs, rhs, "").unwrap()
    }

    pub(crate) fn int_lt(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_compare(IntPredicate::SLT, lhs, rhs, "").unwrap()
    }

    pub(crate) fn int_le(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_compare(IntPredicate::SLE, lhs, rhs, "").unwrap()
    }

    pub(crate) fn int_sub(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_sub(lhs, rhs, "").unwrap()
    }

    pub(crate) fn int_add(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_add(lhs, rhs, "").unwrap()
    }

    pub(crate) fn int_mul(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_mul(lhs, rhs, "").unwrap()
    }

    pub(crate) fn int_div(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_signed_div(lhs, rhs, "").unwrap()
    }

    pub(crate) fn int_rem(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_int_signed_rem(lhs, rhs, "").unwrap()
    }

    pub(crate) fn bit_and(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_and(lhs, rhs, "").unwrap()
    }

    pub(crate) fn bit_or(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_or(lhs, rhs, "").unwrap()
    }

    pub(crate) fn bit_xor(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_xor(lhs, rhs, "").unwrap()
    }

    pub(crate) fn bit_not(&self, value: IntValue<'ctx>) -> IntValue<'ctx> {
        self.inner.build_not(value, "").unwrap()
    }

    pub(crate) fn left_shift(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_left_shift(lhs, rhs, "").unwrap()
    }

    pub(crate) fn right_shift(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_right_shift(lhs, rhs, false, "").unwrap()
    }

    pub(crate) fn signed_right_shift(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_right_shift(lhs, rhs, true, "").unwrap()
    }

    pub(crate) fn int_to_float(
        &self,
        value: IntValue<'ctx>,
        size: u32,
    ) -> FloatValue<'ctx> {
        let typ = if size == 32 {
            self.context.f32_type()
        } else {
            self.context.f64_type()
        };

        self.inner
            .build_cast(InstructionOpcode::SIToFP, value, typ, "")
            .unwrap()
            .into_float_value()
    }

    pub(crate) fn int_to_int(
        &self,
        value: IntValue<'ctx>,
        size: u32,
        signed: bool,
    ) -> IntValue<'ctx> {
        let target = match size {
            1 => self.context.bool_type(),
            8 => self.context.i8_type(),
            16 => self.context.i16_type(),
            32 => self.context.i32_type(),
            _ => self.context.i64_type(),
        };

        self.inner.build_int_cast_sign_flag(value, target, signed, "").unwrap()
    }

    pub(crate) fn bool_to_int(&self, value: IntValue<'ctx>) -> IntValue<'ctx> {
        let typ = self.context.i64_type();

        self.inner.build_int_cast_sign_flag(value, typ, false, "").unwrap()
    }

    pub(crate) fn int_to_bool(&self, value: IntValue<'ctx>) -> IntValue<'ctx> {
        let typ = self.context.bool_type();

        self.inner.build_int_cast_sign_flag(value, typ, true, "").unwrap()
    }

    pub(crate) fn float_to_float(
        &self,
        value: FloatValue<'ctx>,
        size: u32,
    ) -> FloatValue<'ctx> {
        let target = match size {
            32 => self.context.f32_type(),
            _ => self.context.f64_type(),
        };

        self.inner.build_float_cast(value, target, "").unwrap()
    }

    pub(crate) fn int_to_pointer(
        &self,
        value: IntValue<'ctx>,
    ) -> PointerValue<'ctx> {
        self.inner
            .build_int_to_ptr(value, self.context.pointer_type(), "")
            .unwrap()
    }

    pub(crate) fn float_add(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> FloatValue<'ctx> {
        self.inner.build_float_add(lhs, rhs, "").unwrap()
    }

    pub(crate) fn float_sub(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> FloatValue<'ctx> {
        self.inner.build_float_sub(lhs, rhs, "").unwrap()
    }

    pub(crate) fn float_div(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> FloatValue<'ctx> {
        self.inner.build_float_div(lhs, rhs, "").unwrap()
    }

    pub(crate) fn float_mul(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> FloatValue<'ctx> {
        self.inner.build_float_mul(lhs, rhs, "").unwrap()
    }

    pub(crate) fn float_rem(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> FloatValue<'ctx> {
        self.inner.build_float_rem(lhs, rhs, "").unwrap()
    }

    pub(crate) fn float_eq(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner
            .build_float_compare(FloatPredicate::OEQ, lhs, rhs, "")
            .unwrap()
    }

    pub(crate) fn float_lt(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner
            .build_float_compare(FloatPredicate::OLT, lhs, rhs, "")
            .unwrap()
    }

    pub(crate) fn float_le(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner
            .build_float_compare(FloatPredicate::OLE, lhs, rhs, "")
            .unwrap()
    }

    pub(crate) fn float_gt(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner
            .build_float_compare(FloatPredicate::OGT, lhs, rhs, "")
            .unwrap()
    }

    pub(crate) fn float_ge(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner
            .build_float_compare(FloatPredicate::OGE, lhs, rhs, "")
            .unwrap()
    }

    pub(crate) fn float_is_nan(
        &self,
        value: FloatValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner
            .build_float_compare(FloatPredicate::UNO, value, value, "")
            .unwrap()
    }

    pub(crate) fn pointer_is_null(
        &self,
        value: PointerValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.inner.build_is_null(value, "").unwrap()
    }

    pub(crate) fn bitcast<V: BasicValue<'ctx>, T: BasicType<'ctx>>(
        &self,
        value: V,
        typ: T,
    ) -> BasicValueEnum<'ctx> {
        self.inner.build_bitcast(value, typ, "").unwrap()
    }

    pub(crate) fn first_block(&self) -> BasicBlock<'ctx> {
        self.function.get_first_basic_block().unwrap()
    }

    pub(crate) fn add_block(&self) -> BasicBlock<'ctx> {
        self.context.append_basic_block(self.function)
    }

    pub(crate) fn switch_to_block(&self, block: BasicBlock<'ctx>) {
        self.inner.position_at_end(block);
    }

    pub(crate) fn new_temporary<T: BasicType<'ctx>>(
        &self,
        typ: T,
    ) -> PointerValue<'ctx> {
        self.inner.build_alloca(typ, "").unwrap()
    }

    pub(crate) fn jump(&self, block: BasicBlock<'ctx>) {
        self.inner.build_unconditional_branch(block).unwrap();
    }

    pub(crate) fn return_value(&self, val: Option<&dyn BasicValue<'ctx>>) {
        self.inner.build_return(val).unwrap();
    }

    pub(crate) fn branch(
        &self,
        condition: IntValue<'ctx>,
        true_block: BasicBlock<'ctx>,
        false_block: BasicBlock<'ctx>,
    ) {
        self.inner
            .build_conditional_branch(condition, true_block, false_block)
            .unwrap();
    }

    pub(crate) fn switch(
        &self,
        value: IntValue<'ctx>,
        cases: &[(IntValue<'ctx>, BasicBlock<'ctx>)],
        fallback: BasicBlock<'ctx>,
    ) {
        self.inner.build_switch(value, fallback, cases).unwrap();
    }

    pub(crate) fn exhaustive_switch(
        &self,
        value: IntValue<'ctx>,
        cases: &[(IntValue<'ctx>, BasicBlock<'ctx>)],
    ) {
        self.switch(value, cases, cases[0].1);
    }

    pub(crate) fn unreachable(&self) {
        self.inner.build_unreachable().unwrap();
    }

    pub(crate) fn string_bytes(&self, value: &str) -> ArrayValue<'ctx> {
        let bytes = value
            .bytes()
            .map(|v| self.context.i8_type().const_int(v as _, false))
            .collect::<Vec<_>>();

        self.context.i8_type().const_array(&bytes)
    }

    pub(crate) fn new_stack_slot<T: BasicType<'ctx>>(
        &self,
        value_type: T,
    ) -> PointerValue<'ctx> {
        let builder = Builder::new(self.context, self.function);
        let block = self.first_block();

        if let Some(ins) = block.get_first_instruction() {
            builder.inner.position_before(&ins);
        } else {
            builder.switch_to_block(block);
        }

        builder.new_temporary(value_type)
    }

    pub(crate) fn debug_scope(&self) -> DIScope<'ctx> {
        self.function.get_subprogram().unwrap().as_debug_info_scope()
    }

    pub(crate) fn set_debug_location(&self, location: DILocation<'ctx>) {
        self.inner.set_current_debug_location(location);
    }

    pub(crate) fn set_debug_function(&self, function: DISubprogram) {
        self.function.set_subprogram(function);
    }

    pub(crate) fn allocate<'a, 'b>(
        &self,
        module: &'a mut Module<'b, 'ctx>,
        db: &Database,
        names: &crate::symbol_names::SymbolNames,
        class: ClassId,
    ) -> PointerValue<'ctx> {
        let atomic = class.is_atomic(db);
        let name = &names.classes[&class];
        let global = module.add_class(class, name).as_pointer_value();
        let class_ptr = self.load_untyped_pointer(global);
        let size = module.layouts.size_of_class(class);
        let err_func =
            module.runtime_function(RuntimeFunction::AllocationError);
        let alloc_func = module.runtime_function(RuntimeFunction::Allocate);
        let size = self.u64_literal(size).into();
        let res = self.call(alloc_func, &[size]).into_pointer_value();

        let err_block = self.add_block();
        let ok_block = self.add_block();
        let is_null = self.pointer_is_null(res);
        let header = module.layouts.header;

        self.branch(is_null, err_block, ok_block);

        // The block to jump to when the allocation failed.
        self.switch_to_block(err_block);
        self.call_void(err_func, &[class_ptr.into()]);
        self.unreachable();

        // The block to jump to when the allocation succeeds.
        self.switch_to_block(ok_block);

        // Atomic values start with a reference count of 1, so atomic decrements
        // returns the correct result for a value for which no extra references
        // have been created (instead of underflowing).
        let refs = self.u32_literal(if atomic { 1 } else { 0 });

        self.store_field(header, res, HEADER_CLASS_INDEX, class_ptr);
        self.store_field(header, res, HEADER_REFS_INDEX, refs);
        res
    }
}

/// A wrapper around the LLVM types used for building debugging information.
pub(crate) struct DebugBuilder<'ctx> {
    inner: DebugInfoBuilder<'ctx>,
    unit: DICompileUnit<'ctx>,
    context: &'ctx Context,
}

impl<'ctx> DebugBuilder<'ctx> {
    pub(crate) fn new(
        module: &InkwellModule<'ctx>,
        context: &'ctx Context,
        path: &Path,
    ) -> DebugBuilder<'ctx> {
        let version =
            context.i32_type().const_int(debug_metadata_version() as _, false);

        module.add_basic_value_flag(
            "Debug Info Version",
            FlagBehavior::Warning,
            version,
        );

        let file_name =
            path.file_name().and_then(|p| p.to_str()).unwrap_or("unknown");
        let dir_name = path.parent().and_then(|p| p.to_str()).unwrap_or(".");
        let (inner, unit) = module.create_debug_info_builder(
            true,
            DWARFSourceLanguage::C,
            file_name,
            dir_name,
            "Inko",
            false,
            "",
            0,
            "",
            DWARFEmissionKind::Full,
            0,
            false,
            false,
            "",
            "",
        );

        DebugBuilder { inner, context, unit }
    }

    pub(crate) fn new_location(
        &self,
        line: usize,
        column: usize,
        scope: DIScope<'ctx>,
    ) -> DILocation<'ctx> {
        self.inner.create_debug_location(
            &self.context.inner,
            line as u32,
            column as u32,
            scope,
            None,
        )
    }

    pub(crate) fn new_function(
        &self,
        name: &str,
        mangled_name: &str,
        path: &Path,
        line: usize,
        private: bool,
        optimised: bool,
    ) -> DISubprogram<'ctx> {
        // LLVM caches the file data so we don't have to worry about creating
        // too many redundant files here. Of course instead of doing the obvious
        // thing and taking _just_ a path to the file, LLVM wants us to provide
        // a path to the directory and the file name separately. Brilliant.
        let dir = path.parent().and_then(|p| p.to_str()).unwrap_or(".");
        let file_name =
            path.file_name().and_then(|p| p.to_str()).unwrap_or("unknown");

        let file = self.inner.create_file(file_name, dir);
        let typ =
            self.inner.create_subroutine_type(file, None, &[], DIFlags::PUBLIC);
        let scope = self.unit.as_debug_info_scope();

        self.inner.create_function(
            scope,
            name,
            Some(mangled_name),
            file,
            line as u32,
            typ,
            private,
            true,
            line as u32,
            DIFlags::PUBLIC,
            optimised,
        )
    }

    pub(crate) fn finalize(&self) {
        self.inner.finalize();
    }
}
