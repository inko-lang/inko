use crate::llvm::module::Module;
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;

#[derive(Copy, Clone)]
pub(crate) enum RuntimeFunction {
    ArrayNew,
    ArrayNewPermanent,
    ArrayPush,
    CheckRefs,
    ClassObject,
    ClassProcess,
    FloatBoxed,
    FloatBoxedPermanent,
    FloatClone,
    FloatEq,
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
    SocketAccept,
    SocketAddressPairAddress,
    SocketAddressPairDrop,
    SocketAddressPairPort,
    SocketBind,
    SocketConnect,
    SocketDrop,
    SocketListen,
    SocketLocalAddress,
    SocketNew,
    SocketPeerAddress,
    SocketRead,
    SocketReceiveFrom,
    SocketSendBytesTo,
    SocketSendStringTo,
    SocketSetBroadcast,
    SocketSetKeepalive,
    SocketSetLinger,
    SocketSetNodelay,
    SocketSetOnlyV6,
    SocketSetRecvSize,
    SocketSetReuseAddress,
    SocketSetReusePort,
    SocketSetSendSize,
    SocketSetTtl,
    SocketShutdownRead,
    SocketShutdownReadWrite,
    SocketShutdownWrite,
    SocketTryClone,
    SocketWriteBytes,
    SocketWriteString,
    StringConcat,
    StringNewPermanent,
}

impl RuntimeFunction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            RuntimeFunction::ArrayNew => "inko_array_new",
            RuntimeFunction::ArrayNewPermanent => "inko_array_new_permanent",
            RuntimeFunction::ArrayPush => "inko_array_push",
            RuntimeFunction::CheckRefs => "inko_check_refs",
            RuntimeFunction::ClassObject => "inko_class_object",
            RuntimeFunction::ClassProcess => "inko_class_process",
            RuntimeFunction::FloatBoxed => "inko_float_boxed",
            RuntimeFunction::FloatBoxedPermanent => {
                "inko_float_boxed_permanent"
            }
            RuntimeFunction::FloatClone => "inko_float_clone",
            RuntimeFunction::FloatEq => "inko_float_eq",
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
            RuntimeFunction::SocketAccept => "inko_socket_accept",
            RuntimeFunction::SocketAddressPairAddress => {
                "inko_socket_address_pair_address"
            }
            RuntimeFunction::SocketAddressPairDrop => {
                "inko_socket_address_pair_drop"
            }
            RuntimeFunction::SocketAddressPairPort => {
                "inko_socket_address_pair_port"
            }
            RuntimeFunction::SocketBind => "inko_socket_bind",
            RuntimeFunction::SocketConnect => "inko_socket_connect",
            RuntimeFunction::SocketDrop => "inko_socket_drop",
            RuntimeFunction::SocketListen => "inko_socket_listen",
            RuntimeFunction::SocketLocalAddress => "inko_socket_local_address",
            RuntimeFunction::SocketPeerAddress => "inko_socket_peer_address",
            RuntimeFunction::SocketRead => "inko_socket_read",
            RuntimeFunction::SocketReceiveFrom => "inko_socket_receive_from",
            RuntimeFunction::SocketSendBytesTo => "inko_socket_send_bytes_to",
            RuntimeFunction::SocketSendStringTo => "inko_socket_send_string_to",
            RuntimeFunction::SocketSetBroadcast => "inko_socket_set_broadcast",
            RuntimeFunction::SocketSetKeepalive => "inko_socket_set_keepalive",
            RuntimeFunction::SocketSetLinger => "inko_socket_set_linger",
            RuntimeFunction::SocketSetNodelay => "inko_socket_set_nodelay",
            RuntimeFunction::SocketSetOnlyV6 => "inko_socket_set_only_v6",
            RuntimeFunction::SocketSetRecvSize => "inko_socket_set_recv_size",
            RuntimeFunction::SocketSetReuseAddress => {
                "inko_socket_set_reuse_address"
            }
            RuntimeFunction::SocketSetReusePort => "inko_socket_set_reuse_port",
            RuntimeFunction::SocketSetSendSize => "inko_socket_set_send_size",
            RuntimeFunction::SocketSetTtl => "inko_socket_set_ttl",
            RuntimeFunction::SocketShutdownRead => "inko_socket_shutdown_read",
            RuntimeFunction::SocketShutdownReadWrite => {
                "inko_socket_shutdown_read_write"
            }
            RuntimeFunction::SocketShutdownWrite => {
                "inko_socket_shutdown_write"
            }
            RuntimeFunction::SocketTryClone => "inko_socket_try_clone",
            RuntimeFunction::SocketNew => "inko_socket_new",
            RuntimeFunction::SocketWriteBytes => "inko_socket_write_bytes",
            RuntimeFunction::SocketWriteString => "inko_socket_write_string",
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
            RuntimeFunction::ArrayNewPermanent => {
                let state = module.layouts.state.ptr_type(space).into();
                let capa = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, capa], false)
            }
            RuntimeFunction::ArrayNew => {
                let state = module.layouts.state.ptr_type(space).into();
                let capa = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, capa], false)
            }
            RuntimeFunction::ArrayPush => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let val = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array, val], false)
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
            RuntimeFunction::FloatEq => {
                let state = module.layouts.state.ptr_type(space).into();
                let lhs = context.f64_type().into();
                let rhs = context.f64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, lhs, rhs], false)
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
                let ret = context.pointer_type();

                ret.fn_type(&[counts], false)
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
            RuntimeFunction::ClassObject => {
                let name = context.pointer_type().into();
                let size = context.i32_type().into();
                let methods = context.i16_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[name, size, methods], false)
            }
            RuntimeFunction::ClassProcess => {
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
            RuntimeFunction::SocketAccept => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let deadline = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, socket, deadline], false)
            }
            RuntimeFunction::SocketAddressPairAddress => {
                let pair = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[pair], false)
            }
            RuntimeFunction::SocketAddressPairDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let pair = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, pair], false)
            }
            RuntimeFunction::SocketAddressPairPort => {
                let pair = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[pair], false)
            }
            RuntimeFunction::SocketBind => {
                let state = module.layouts.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let address = context.pointer_type().into();
                let port = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, socket, address, port], false)
            }
            RuntimeFunction::SocketConnect => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let address = context.pointer_type().into();
                let port = context.i64_type().into();
                let deadline = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(
                    &[state, proc, socket, address, port, deadline],
                    false,
                )
            }
            RuntimeFunction::SocketDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, socket], false)
            }
            RuntimeFunction::SocketListen => {
                let state = module.layouts.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let value = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, socket, value], false)
            }
            RuntimeFunction::SocketLocalAddress
            | RuntimeFunction::SocketPeerAddress => {
                let state = module.layouts.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, socket], false)
            }
            RuntimeFunction::SocketRead
            | RuntimeFunction::SocketReceiveFrom => {
                let state = module.layouts.state.ptr_type(space).into();
                let process = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let amount = context.i64_type().into();
                let deadline = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(
                    &[state, process, socket, buffer, amount, deadline],
                    false,
                )
            }
            RuntimeFunction::SocketSendBytesTo
            | RuntimeFunction::SocketSendStringTo => {
                let state = module.layouts.state.ptr_type(space).into();
                let process = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let address = context.pointer_type().into();
                let port = context.i64_type().into();
                let deadline = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(
                    &[state, process, socket, buffer, address, port, deadline],
                    false,
                )
            }
            RuntimeFunction::SocketSetBroadcast
            | RuntimeFunction::SocketSetKeepalive
            | RuntimeFunction::SocketSetNodelay
            | RuntimeFunction::SocketSetOnlyV6
            | RuntimeFunction::SocketSetReuseAddress
            | RuntimeFunction::SocketSetReusePort => {
                let state = module.layouts.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let value = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, socket, value], false)
            }
            RuntimeFunction::SocketSetLinger
            | RuntimeFunction::SocketSetRecvSize
            | RuntimeFunction::SocketSetSendSize
            | RuntimeFunction::SocketSetTtl => {
                let state = module.layouts.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let value = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, socket, value], false)
            }
            RuntimeFunction::SocketShutdownRead
            | RuntimeFunction::SocketShutdownReadWrite
            | RuntimeFunction::SocketShutdownWrite => {
                let state = module.layouts.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, socket], false)
            }
            RuntimeFunction::SocketTryClone => {
                let socket = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[socket], false)
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
            RuntimeFunction::SocketNew => {
                let proto = context.i64_type().into();
                let kind = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[proto, kind], false)
            }
            RuntimeFunction::SocketWriteBytes
            | RuntimeFunction::SocketWriteString => {
                let state = module.layouts.state.ptr_type(space).into();
                let process = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let deadline = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, process, socket, buffer, deadline], false)
            }
        };

        module.add_function(self.name(), fn_type, None)
    }
}
