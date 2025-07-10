use crate::llvm::module::Module;
use inkwell::values::FunctionValue;

#[derive(Copy, Clone)]
pub(crate) enum RuntimeFunction {
    AllocationError,
    Free,
    Malloc,
    NewProcess,
    NewType,
    ProcessFinishMessage,
    ProcessNew,
    ProcessSendMessage,
    ProcessYield,
    ReferenceCountError,
    RuntimeDrop,
    RuntimeNew,
    RuntimeStackMask,
    RuntimeStart,
    RuntimeState,
}

impl RuntimeFunction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            RuntimeFunction::ReferenceCountError => {
                "inko_reference_count_error"
            }
            RuntimeFunction::NewType => "inko_type_object",
            RuntimeFunction::NewProcess => "inko_type_process",
            RuntimeFunction::ProcessFinishMessage => {
                "inko_process_finish_message"
            }
            RuntimeFunction::ProcessNew => "inko_process_new",
            RuntimeFunction::ProcessSendMessage => "inko_process_send_message",
            RuntimeFunction::ProcessYield => "inko_process_yield",
            RuntimeFunction::RuntimeDrop => "inko_runtime_drop",
            RuntimeFunction::RuntimeNew => "inko_runtime_new",
            RuntimeFunction::RuntimeStart => "inko_runtime_start",
            RuntimeFunction::RuntimeState => "inko_runtime_state",
            RuntimeFunction::RuntimeStackMask => "inko_runtime_stack_mask",
            RuntimeFunction::Malloc => "malloc",
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
            RuntimeFunction::ProcessFinishMessage => {
                let proc = context.pointer_type().into();
                let terminate = context.bool_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc, terminate], false)
            }
            RuntimeFunction::RuntimeNew => {
                let argc = context.i32_type().into();
                let argv = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[argc, argv], false)
            }
            RuntimeFunction::RuntimeDrop => {
                let runtime = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[runtime], false)
            }
            RuntimeFunction::RuntimeStart => {
                let runtime = context.pointer_type().into();
                let typ = context.pointer_type().into();
                let method = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[runtime, typ, method], false)
            }
            RuntimeFunction::RuntimeState => {
                let runtime = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[runtime], false)
            }
            RuntimeFunction::NewType | RuntimeFunction::NewProcess => {
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
                let typ = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[typ], false)
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
            RuntimeFunction::Malloc => {
                let ptr = context.pointer_type();
                let len = context.i64_type().into();

                ptr.fn_type(&[len], false)
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
