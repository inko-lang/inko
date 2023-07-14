use crate::llvm::module::Module;
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;

#[derive(Copy, Clone)]
pub(crate) enum RuntimeFunction {
    CheckRefs,
    ClassObject,
    ClassProcess,
    FloatBoxed,
    FloatBoxedPermanent,
    FloatClone,
    Free,
    IntBoxed,
    IntBoxedPermanent,
    IntClone,
    IntOverflow,
    MessageNew,
    Allocate,
    ProcessFinishMessage,
    ProcessNew,
    ProcessPanic,
    ProcessSendMessage,
    Reduce,
    RuntimeDrop,
    RuntimeNew,
    RuntimeStart,
    RuntimeState,
    StringConcat,
    StringNewPermanent,
}

impl RuntimeFunction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            RuntimeFunction::CheckRefs => "inko_check_refs",
            RuntimeFunction::ClassObject => "inko_class_object",
            RuntimeFunction::ClassProcess => "inko_class_process",
            RuntimeFunction::FloatBoxed => "inko_float_boxed",
            RuntimeFunction::FloatBoxedPermanent => {
                "inko_float_boxed_permanent"
            }
            RuntimeFunction::FloatClone => "inko_float_clone",
            RuntimeFunction::Free => "inko_free",
            RuntimeFunction::IntBoxed => "inko_int_boxed",
            RuntimeFunction::IntBoxedPermanent => "inko_int_boxed_permanent",
            RuntimeFunction::IntClone => "inko_int_clone",
            RuntimeFunction::IntOverflow => "inko_int_overflow",
            RuntimeFunction::MessageNew => "inko_message_new",
            RuntimeFunction::Allocate => "inko_alloc",
            RuntimeFunction::ProcessFinishMessage => {
                "inko_process_finish_message"
            }
            RuntimeFunction::ProcessNew => "inko_process_new",
            RuntimeFunction::ProcessPanic => "inko_process_panic",
            RuntimeFunction::ProcessSendMessage => "inko_process_send_message",
            RuntimeFunction::Reduce => "inko_reduce",
            RuntimeFunction::RuntimeDrop => "inko_runtime_drop",
            RuntimeFunction::RuntimeNew => "inko_runtime_new",
            RuntimeFunction::RuntimeStart => "inko_runtime_start",
            RuntimeFunction::RuntimeState => "inko_runtime_state",
            RuntimeFunction::StringConcat => "inko_string_concat",
            RuntimeFunction::StringNewPermanent => "inko_string_new_permanent",
        }
    }

    pub(crate) fn build<'ctx>(
        self,
        module: &Module<'_, 'ctx>,
    ) -> FunctionValue<'ctx> {
        let context = module.context;
        let space = AddressSpace::default();
        let fn_type = match self {
            RuntimeFunction::IntBoxedPermanent => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::IntBoxed => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::IntClone => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.pointer_type();

                val.fn_type(&[state, val.into()], false)
            }
            RuntimeFunction::IntOverflow => {
                let proc = context.pointer_type().into();
                let lhs = context.i64_type().into();
                let rhs = context.i64_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc, lhs, rhs], false)
            }
            RuntimeFunction::CheckRefs => {
                let proc = context.pointer_type().into();
                let val = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc, val], false)
            }
            RuntimeFunction::Free => {
                let val = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[val], false)
            }
            RuntimeFunction::FloatBoxedPermanent => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.f64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::FloatClone => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.pointer_type();

                val.fn_type(&[state, val.into()], false)
            }
            RuntimeFunction::Reduce => {
                let proc = context.pointer_type().into();
                let amount = context.i16_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc, amount], false)
            }
            RuntimeFunction::Allocate => {
                let class = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[class], false)
            }
            RuntimeFunction::ProcessPanic => {
                let proc = context.pointer_type().into();
                let val = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc, val], false)
            }
            RuntimeFunction::FloatBoxed => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.f64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::ProcessFinishMessage => {
                let proc = context.pointer_type().into();
                let terminate = context.bool_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc, terminate], false)
            }
            RuntimeFunction::RuntimeNew => {
                let counts =
                    module.layouts.method_counts.ptr_type(space).into();
                let argc = context.i32_type().into();
                let argv = context.i8_type().ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[counts, argc, argv], false)
            }
            RuntimeFunction::RuntimeDrop => {
                let runtime = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[runtime], false)
            }
            RuntimeFunction::RuntimeStart => {
                let runtime = context.pointer_type().into();
                let class = context.pointer_type().into();
                let method = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[runtime, class, method], false)
            }
            RuntimeFunction::RuntimeState => {
                let runtime = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[runtime], false)
            }
            RuntimeFunction::ClassObject | RuntimeFunction::ClassProcess => {
                let name = context.pointer_type().into();
                let size = context.i32_type().into();
                let methods = context.i16_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[name, size, methods], false)
            }
            RuntimeFunction::MessageNew => {
                let method = context.pointer_type().into();
                let length = context.i8_type().into();
                let ret = module.layouts.message.ptr_type(space);

                ret.fn_type(&[method, length], false)
            }
            RuntimeFunction::ProcessSendMessage => {
                let state = module.layouts.state.ptr_type(space).into();
                let sender = context.pointer_type().into();
                let receiver = context.pointer_type().into();
                let message = module.layouts.message.ptr_type(space).into();
                let ret = context.void_type();

                ret.fn_type(&[state, sender, receiver, message], false)
            }
            RuntimeFunction::ProcessNew => {
                let process = context.pointer_type().into();
                let class = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[process, class], false)
            }
            RuntimeFunction::StringConcat => {
                let state = module.layouts.state.ptr_type(space).into();
                let strings = context.pointer_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, strings, length], false)
            }
            RuntimeFunction::StringNewPermanent => {
                let state = module.layouts.state.ptr_type(space).into();
                let bytes = context.pointer_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, bytes, length], false)
            }
        };

        module.add_function(self.name(), fn_type, None)
    }
}
