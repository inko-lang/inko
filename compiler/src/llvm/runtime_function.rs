use crate::llvm::module::Module;
use inkwell::values::FunctionValue;

#[derive(Copy, Clone)]
pub(crate) enum RuntimeFunction {
    ReferenceCountError,
    ClassObject,
    ClassProcess,
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
    Free,
    AllocationError,
}

impl RuntimeFunction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            RuntimeFunction::ReferenceCountError => {
                "inko_reference_count_error"
            }
            RuntimeFunction::ClassObject => "inko_class_object",
            RuntimeFunction::ClassProcess => "inko_class_process",
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
            RuntimeFunction::Free => "free",
            RuntimeFunction::AllocationError => "inko_alloc_error",
        }
    }

    pub(crate) fn build<'ctx>(
        self,
        module: &Module<'_, 'ctx>,
    ) -> FunctionValue<'ctx> {
        let context = module.context;
        let fn_type = match self {
            RuntimeFunction::ReferenceCountError => {
                let proc = context.pointer_type().into();
                let val = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc, val], false)
            }
            RuntimeFunction::ProcessYield => {
                let proc = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc], false)
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
                let counts = context.pointer_type().into();
                let argc = context.i32_type().into();
                let argv = context.pointer_type().into();
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
            RuntimeFunction::ProcessSendMessage => {
                let state = context.pointer_type().into();
                let sender = context.pointer_type().into();
                let receiver = context.pointer_type().into();
                let func = context.pointer_type().into();
                let data = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[state, sender, receiver, func, data], false)
            }
            RuntimeFunction::ProcessNew => {
                let process = context.pointer_type().into();
                let class = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[process, class], false)
            }
            RuntimeFunction::StringConcat => {
                let state = context.pointer_type().into();
                let strings = context.pointer_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, strings, length], false)
            }
            RuntimeFunction::StringNew => {
                let state = context.pointer_type().into();
                let bytes = context.pointer_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, bytes, length], false)
            }
            RuntimeFunction::RuntimeStackMask => {
                let state = context.pointer_type().into();
                let ret = context.i64_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::Free => {
                let ptr = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[ptr], false)
            }
            RuntimeFunction::AllocationError => {
                let size = context.i64_type().into();
                let ret = context.void_type();

                ret.fn_type(&[size], false)
            }
        };

        module.add_function(self.name(), fn_type, None)
    }
}
