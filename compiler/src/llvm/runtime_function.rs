use crate::llvm::module::Module;
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;

#[derive(Copy, Clone)]
pub(crate) enum RuntimeFunction {
    CheckRefs,
    ClassObject,
    ClassProcess,
    Free,
    MessageNew,
    Allocate,
    AllocateAtomic,
    ProcessFinishMessage,
    ProcessNew,
    ProcessPanic,
    ProcessSendMessage,
    ProcessYield,
    RuntimeDrop,
    RuntimeNew,
    RuntimeStart,
    RuntimeState,
    StringConcat,
    StringNew,
    RuntimeStackMask,
}

impl RuntimeFunction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            RuntimeFunction::CheckRefs => "inko_check_refs",
            RuntimeFunction::ClassObject => "inko_class_object",
            RuntimeFunction::ClassProcess => "inko_class_process",
            RuntimeFunction::Free => "inko_free",
            RuntimeFunction::MessageNew => "inko_message_new",
            RuntimeFunction::Allocate => "inko_alloc",
            RuntimeFunction::AllocateAtomic => "inko_alloc_atomic",
            RuntimeFunction::ProcessFinishMessage => {
                "inko_process_finish_message"
            }
            RuntimeFunction::ProcessNew => "inko_process_new",
            RuntimeFunction::ProcessPanic => "inko_process_panic",
            RuntimeFunction::ProcessSendMessage => "inko_process_send_message",
            RuntimeFunction::ProcessYield => "inko_process_yield",
            RuntimeFunction::RuntimeDrop => "inko_runtime_drop",
            RuntimeFunction::RuntimeNew => "inko_runtime_new",
            RuntimeFunction::RuntimeStart => "inko_runtime_start",
            RuntimeFunction::RuntimeState => "inko_runtime_state",
            RuntimeFunction::StringConcat => "inko_string_concat",
            RuntimeFunction::StringNew => "inko_string_new",
            RuntimeFunction::RuntimeStackMask => "inko_runtime_stack_mask",
        }
    }

    pub(crate) fn build<'ctx>(
        self,
        module: &Module<'_, 'ctx>,
    ) -> FunctionValue<'ctx> {
        let context = module.context;
        let space = AddressSpace::default();
        let fn_type = match self {
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
            RuntimeFunction::ProcessYield => {
                let proc = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc], false)
            }
            RuntimeFunction::Allocate | RuntimeFunction::AllocateAtomic => {
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
            RuntimeFunction::StringNew => {
                let state = module.layouts.state.ptr_type(space).into();
                let bytes = context.pointer_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, bytes, length], false)
            }
            RuntimeFunction::RuntimeStackMask => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = context.i64_type();

                ret.fn_type(&[state], false)
            }
        };

        module.add_function(self.name(), fn_type, None)
    }
}
