use crate::llvm::module::Module;
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;

#[derive(Copy, Clone)]
pub(crate) enum RuntimeFunction {
    ArrayCapacity,
    ArrayClear,
    ArrayDrop,
    ArrayGet,
    ArrayLength,
    ArrayNew,
    ArrayNewPermanent,
    ArrayPop,
    ArrayPush,
    ArrayRemove,
    ArrayReserve,
    ArraySet,
    ByteArrayAppend,
    ByteArrayClear,
    ByteArrayClone,
    ByteArrayCopyFrom,
    ByteArrayDrainToString,
    ByteArrayDrop,
    ByteArrayEq,
    ByteArrayGet,
    ByteArrayLength,
    ByteArrayNew,
    ByteArrayPop,
    ByteArrayPush,
    ByteArrayRemove,
    ByteArrayResize,
    ByteArraySet,
    ByteArraySlice,
    ByteArrayToString,
    ChannelDrop,
    ChannelNew,
    ChannelReceive,
    ChannelReceiveUntil,
    ChannelSend,
    ChannelTryReceive,
    ChannelWait,
    CheckRefs,
    ChildProcessDrop,
    ChildProcessSpawn,
    ChildProcessStderrClose,
    ChildProcessStderrRead,
    ChildProcessStdinClose,
    ChildProcessStdinFlush,
    ChildProcessStdinWriteBytes,
    ChildProcessStdinWriteString,
    ChildProcessStdoutClose,
    ChildProcessStdoutRead,
    ChildProcessTryWait,
    ChildProcessWait,
    ClassObject,
    ClassProcess,
    CpuCores,
    DirectoryCreate,
    DirectoryCreateRecursive,
    DirectoryList,
    DirectoryRemove,
    DirectoryRemoveAll,
    EnvArguments,
    EnvExecutable,
    EnvGet,
    EnvGetWorkingDirectory,
    EnvHomeDirectory,
    EnvSetWorkingDirectory,
    EnvTempDirectory,
    EnvVariables,
    Exit,
    FileCopy,
    FileDrop,
    FileFlush,
    FileOpen,
    FileRead,
    FileRemove,
    FileSeek,
    FileSize,
    FileWriteBytes,
    FileWriteString,
    FloatBoxed,
    FloatBoxedPermanent,
    FloatClone,
    FloatEq,
    FloatRound,
    FloatToString,
    Free,
    IntBoxed,
    IntBoxedPermanent,
    IntClone,
    IntOverflow,
    IntPow,
    IntToString,
    MessageNew,
    Allocate,
    PathAccessedAt,
    PathCreatedAt,
    PathExists,
    PathExpand,
    PathIsDirectory,
    PathIsFile,
    PathModifiedAt,
    ProcessFinishMessage,
    ProcessNew,
    ProcessPanic,
    ProcessSendMessage,
    ProcessStackFrameLine,
    ProcessStackFrameName,
    ProcessStackFramePath,
    ProcessStacktrace,
    ProcessStacktraceDrop,
    ProcessStacktraceLength,
    ProcessSuspend,
    RandomBytes,
    RandomDrop,
    RandomFloat,
    RandomFloatRange,
    RandomFromInt,
    RandomInt,
    RandomIntRange,
    RandomNew,
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
    StderrFlush,
    StderrWriteBytes,
    StderrWriteString,
    StdinRead,
    StdoutFlush,
    StdoutWriteBytes,
    StdoutWriteString,
    StringByte,
    StringCharacters,
    StringCharactersDrop,
    StringCharactersNext,
    StringConcat,
    StringConcatArray,
    StringDrop,
    StringEquals,
    StringNewPermanent,
    StringSize,
    StringSliceBytes,
    StringToByteArray,
    StringToFloat,
    StringToInt,
    StringToLower,
    StringToUpper,
    TimeMonotonic,
    TimeSystem,
    TimeSystemOffset,
}

impl RuntimeFunction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            RuntimeFunction::ArrayCapacity => "inko_array_capacity",
            RuntimeFunction::ArrayClear => "inko_array_clear",
            RuntimeFunction::ArrayDrop => "inko_array_drop",
            RuntimeFunction::ArrayGet => "inko_array_get",
            RuntimeFunction::ArrayLength => "inko_array_length",
            RuntimeFunction::ArrayNew => "inko_array_new",
            RuntimeFunction::ArrayNewPermanent => "inko_array_new_permanent",
            RuntimeFunction::ArrayPop => "inko_array_pop",
            RuntimeFunction::ArrayPush => "inko_array_push",
            RuntimeFunction::ArrayRemove => "inko_array_remove",
            RuntimeFunction::ArrayReserve => "inko_array_reserve",
            RuntimeFunction::ArraySet => "inko_array_set",
            RuntimeFunction::ByteArrayAppend => "inko_byte_array_append",
            RuntimeFunction::ByteArrayClear => "inko_byte_array_clear",
            RuntimeFunction::ByteArrayClone => "inko_byte_array_clone",
            RuntimeFunction::ByteArrayCopyFrom => "inko_byte_array_copy_from",
            RuntimeFunction::ByteArrayDrainToString => {
                "inko_byte_array_drain_to_string"
            }
            RuntimeFunction::ByteArrayDrop => "inko_byte_array_drop",
            RuntimeFunction::ByteArrayEq => "inko_byte_array_eq",
            RuntimeFunction::ByteArrayGet => "inko_byte_array_get",
            RuntimeFunction::ByteArrayLength => "inko_byte_array_length",
            RuntimeFunction::ByteArrayNew => "inko_byte_array_new",
            RuntimeFunction::ByteArrayPop => "inko_byte_array_pop",
            RuntimeFunction::ByteArrayPush => "inko_byte_array_push",
            RuntimeFunction::ByteArrayRemove => "inko_byte_array_remove",
            RuntimeFunction::ByteArrayResize => "inko_byte_array_resize",
            RuntimeFunction::ByteArraySet => "inko_byte_array_set",
            RuntimeFunction::ByteArraySlice => "inko_byte_array_slice",
            RuntimeFunction::ByteArrayToString => "inko_byte_array_to_string",
            RuntimeFunction::ChannelDrop => "inko_channel_drop",
            RuntimeFunction::ChannelNew => "inko_channel_new",
            RuntimeFunction::ChannelReceive => "inko_channel_receive",
            RuntimeFunction::ChannelReceiveUntil => {
                "inko_channel_receive_until"
            }
            RuntimeFunction::ChannelSend => "inko_channel_send",
            RuntimeFunction::ChannelTryReceive => "inko_channel_try_receive",
            RuntimeFunction::ChannelWait => "inko_channel_wait",
            RuntimeFunction::CheckRefs => "inko_check_refs",
            RuntimeFunction::ChildProcessDrop => "inko_child_process_drop",
            RuntimeFunction::ChildProcessSpawn => "inko_child_process_spawn",
            RuntimeFunction::ChildProcessStderrClose => {
                "inko_child_process_stderr_close"
            }
            RuntimeFunction::ChildProcessStderrRead => {
                "inko_child_process_stderr_read"
            }
            RuntimeFunction::ChildProcessStdinClose => {
                "inko_child_process_stdin_close"
            }
            RuntimeFunction::ChildProcessStdinFlush => {
                "inko_child_process_stdin_flush"
            }
            RuntimeFunction::ChildProcessStdinWriteBytes => {
                "inko_child_process_stdin_write_bytes"
            }
            RuntimeFunction::ChildProcessStdinWriteString => {
                "inko_child_process_stdin_write_string"
            }
            RuntimeFunction::ChildProcessStdoutClose => {
                "inko_child_process_stdout_close"
            }
            RuntimeFunction::ChildProcessStdoutRead => {
                "inko_child_process_stdout_read"
            }
            RuntimeFunction::ChildProcessTryWait => {
                "inko_child_process_try_wait"
            }
            RuntimeFunction::ChildProcessWait => "inko_child_process_wait",
            RuntimeFunction::ClassObject => "inko_class_object",
            RuntimeFunction::ClassProcess => "inko_class_process",
            RuntimeFunction::CpuCores => "inko_cpu_cores",
            RuntimeFunction::DirectoryCreate => "inko_directory_create",
            RuntimeFunction::DirectoryCreateRecursive => {
                "inko_directory_create_recursive"
            }
            RuntimeFunction::DirectoryList => "inko_directory_list",
            RuntimeFunction::DirectoryRemove => "inko_directory_remove",
            RuntimeFunction::DirectoryRemoveAll => "inko_directory_remove_all",
            RuntimeFunction::EnvArguments => "inko_env_arguments",
            RuntimeFunction::EnvExecutable => "inko_env_executable",
            RuntimeFunction::EnvGet => "inko_env_get",
            RuntimeFunction::EnvGetWorkingDirectory => {
                "inko_env_get_working_directory"
            }
            RuntimeFunction::EnvHomeDirectory => "inko_env_home_directory",
            RuntimeFunction::EnvSetWorkingDirectory => {
                "inko_env_set_working_directory"
            }
            RuntimeFunction::EnvTempDirectory => "inko_env_temp_directory",
            RuntimeFunction::EnvVariables => "inko_env_variables",
            RuntimeFunction::Exit => "inko_exit",
            RuntimeFunction::FileCopy => "inko_file_copy",
            RuntimeFunction::FileDrop => "inko_file_drop",
            RuntimeFunction::FileFlush => "inko_file_flush",
            RuntimeFunction::FileOpen => "inko_file_open",
            RuntimeFunction::FileRead => "inko_file_read",
            RuntimeFunction::FileRemove => "inko_file_remove",
            RuntimeFunction::FileSeek => "inko_file_seek",
            RuntimeFunction::FileSize => "inko_file_size",
            RuntimeFunction::FileWriteBytes => "inko_file_write_bytes",
            RuntimeFunction::FileWriteString => "inko_file_write_string",
            RuntimeFunction::FloatBoxed => "inko_float_boxed",
            RuntimeFunction::FloatBoxedPermanent => {
                "inko_float_boxed_permanent"
            }
            RuntimeFunction::FloatClone => "inko_float_clone",
            RuntimeFunction::FloatEq => "inko_float_eq",
            RuntimeFunction::FloatRound => "inko_float_round",
            RuntimeFunction::FloatToString => "inko_float_to_string",
            RuntimeFunction::Free => "inko_free",
            RuntimeFunction::IntBoxed => "inko_int_boxed",
            RuntimeFunction::IntBoxedPermanent => "inko_int_boxed_permanent",
            RuntimeFunction::IntClone => "inko_int_clone",
            RuntimeFunction::IntOverflow => "inko_int_overflow",
            RuntimeFunction::IntPow => "inko_int_pow",
            RuntimeFunction::IntToString => "inko_int_to_string",
            RuntimeFunction::MessageNew => "inko_message_new",
            RuntimeFunction::Allocate => "inko_alloc",
            RuntimeFunction::PathAccessedAt => "inko_path_accessed_at",
            RuntimeFunction::PathCreatedAt => "inko_path_created_at",
            RuntimeFunction::PathExists => "inko_path_exists",
            RuntimeFunction::PathIsDirectory => "inko_path_is_directory",
            RuntimeFunction::PathIsFile => "inko_path_is_file",
            RuntimeFunction::PathModifiedAt => "inko_path_modified_at",
            RuntimeFunction::ProcessFinishMessage => {
                "inko_process_finish_message"
            }
            RuntimeFunction::ProcessNew => "inko_process_new",
            RuntimeFunction::ProcessPanic => "inko_process_panic",
            RuntimeFunction::ProcessSendMessage => "inko_process_send_message",
            RuntimeFunction::ProcessStacktrace => "inko_process_stacktrace",
            RuntimeFunction::ProcessStacktraceDrop => {
                "inko_process_stacktrace_drop"
            }
            RuntimeFunction::ProcessStackFrameLine => {
                "inko_process_stack_frame_line"
            }
            RuntimeFunction::ProcessStackFrameName => {
                "inko_process_stack_frame_name"
            }
            RuntimeFunction::ProcessStackFramePath => {
                "inko_process_stack_frame_path"
            }
            RuntimeFunction::ProcessStacktraceLength => {
                "inko_process_stacktrace_length"
            }
            RuntimeFunction::ProcessSuspend => "inko_process_suspend",
            RuntimeFunction::RandomBytes => "inko_random_bytes",
            RuntimeFunction::RandomDrop => "inko_random_drop",
            RuntimeFunction::RandomFloat => "inko_random_float",
            RuntimeFunction::RandomFloatRange => "inko_random_float_range",
            RuntimeFunction::RandomFromInt => "inko_random_from_int",
            RuntimeFunction::RandomInt => "inko_random_int",
            RuntimeFunction::RandomIntRange => "inko_random_int_range",
            RuntimeFunction::RandomNew => "inko_random_new",
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
            RuntimeFunction::StderrFlush => "inko_stderr_flush",
            RuntimeFunction::StderrWriteBytes => "inko_stderr_write_bytes",
            RuntimeFunction::StderrWriteString => "inko_stderr_write_string",
            RuntimeFunction::StdinRead => "inko_stdin_read",
            RuntimeFunction::StdoutFlush => "inko_stdout_flush",
            RuntimeFunction::StdoutWriteBytes => "inko_stdout_write_bytes",
            RuntimeFunction::StdoutWriteString => "inko_stdout_write_string",
            RuntimeFunction::StringByte => "inko_string_byte",
            RuntimeFunction::StringCharacters => "inko_string_characters",
            RuntimeFunction::StringCharactersDrop => {
                "inko_string_characters_drop"
            }
            RuntimeFunction::StringCharactersNext => {
                "inko_string_characters_next"
            }
            RuntimeFunction::StringConcat => "inko_string_concat",
            RuntimeFunction::StringConcatArray => "inko_string_concat_array",
            RuntimeFunction::StringDrop => "inko_string_drop",
            RuntimeFunction::StringEquals => "inko_string_equals",
            RuntimeFunction::StringNewPermanent => "inko_string_new_permanent",
            RuntimeFunction::StringSize => "inko_string_size",
            RuntimeFunction::StringSliceBytes => "inko_string_slice_bytes",
            RuntimeFunction::StringToByteArray => "inko_string_to_byte_array",
            RuntimeFunction::StringToFloat => "inko_string_to_float",
            RuntimeFunction::StringToInt => "inko_string_to_int",
            RuntimeFunction::StringToLower => "inko_string_to_lower",
            RuntimeFunction::StringToUpper => "inko_string_to_upper",
            RuntimeFunction::TimeMonotonic => "inko_time_monotonic",
            RuntimeFunction::TimeSystem => "inko_time_system",
            RuntimeFunction::TimeSystemOffset => "inko_time_system_offset",
            RuntimeFunction::PathExpand => "inko_path_expand",
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
            RuntimeFunction::StringEquals => {
                let state = module.layouts.state.ptr_type(space).into();
                let lhs = context.pointer_type().into();
                let rhs = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, lhs, rhs], false)
            }
            RuntimeFunction::ProcessPanic => {
                let proc = context.pointer_type().into();
                let val = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[proc, val], false)
            }
            RuntimeFunction::IntPow => {
                let proc = context.pointer_type().into();
                let lhs = context.i64_type().into();
                let rhs = context.i64_type().into();
                let ret = context.i64_type();

                ret.fn_type(&[proc, lhs, rhs], false)
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
            RuntimeFunction::FloatRound => {
                let state = module.layouts.state.ptr_type(space).into();
                let lhs = context.f64_type().into();
                let rhs = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, lhs, rhs], false)
            }
            RuntimeFunction::FloatToString => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.f64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::ArrayCapacity => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::ArrayClear => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::ArrayDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let val = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::ArrayGet => {
                let array = context.pointer_type().into();
                let index = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[array, index], false)
            }
            RuntimeFunction::ArrayLength => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ArrayPop => {
                let array = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[array], false)
            }
            RuntimeFunction::ArrayRemove => {
                let array = context.pointer_type().into();
                let index = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[array, index], false)
            }
            RuntimeFunction::ArrayReserve => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let amount = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array, amount], false)
            }
            RuntimeFunction::ArraySet => {
                let array = context.pointer_type().into();
                let index = context.i64_type().into();
                let value = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[array, index, value], false)
            }
            RuntimeFunction::ByteArrayNew => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::ByteArrayPush => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let value = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array, value], false)
            }
            RuntimeFunction::ByteArrayPop => {
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[array], false)
            }
            RuntimeFunction::ByteArraySet => {
                let array = context.pointer_type().into();
                let index = context.i64_type().into();
                let value = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[array, index, value], false)
            }
            RuntimeFunction::ByteArrayGet => {
                let array = context.pointer_type().into();
                let index = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[array, index], false)
            }
            RuntimeFunction::ByteArrayRemove => {
                let array = context.pointer_type().into();
                let index = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[array, index], false)
            }
            RuntimeFunction::ByteArrayLength => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayEq => {
                let state = module.layouts.state.ptr_type(space).into();
                let lhs = context.pointer_type().into();
                let rhs = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, lhs, rhs], false)
            }
            RuntimeFunction::ByteArrayClear => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayClone => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayToString => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayDrainToString => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArraySlice => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let start = context.i64_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array, start, length], false)
            }
            RuntimeFunction::ByteArrayAppend => {
                let state = module.layouts.state.ptr_type(space).into();
                let target = context.pointer_type().into();
                let source = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, target, source], false)
            }
            RuntimeFunction::ByteArrayCopyFrom => {
                let state = module.layouts.state.ptr_type(space).into();
                let target = context.pointer_type().into();
                let source = context.pointer_type().into();
                let start = context.i64_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, target, source, start, length], false)
            }
            RuntimeFunction::ByteArrayResize => {
                let state = module.layouts.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let size = context.i64_type().into();
                let filler = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array, size, filler], false)
            }
            RuntimeFunction::ChildProcessSpawn => {
                let proc = context.pointer_type().into();
                let program = context.pointer_type().into();
                let args = context.pointer_type().into();
                let env = context.pointer_type().into();
                let stdin = context.i64_type().into();
                let stdout = context.i64_type().into();
                let stderr = context.i64_type().into();
                let dir = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(
                    &[proc, program, args, env, stdin, stdout, stderr, dir],
                    false,
                )
            }
            RuntimeFunction::ChildProcessWait => {
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[proc, child], false)
            }
            RuntimeFunction::ChildProcessTryWait => {
                let child = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[child], false)
            }
            RuntimeFunction::ChildProcessDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let child = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, child], false)
            }
            RuntimeFunction::ChildProcessStdoutRead => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, child, buffer, size], false)
            }
            RuntimeFunction::ChildProcessStderrRead => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, child, buffer, size], false)
            }
            RuntimeFunction::ChildProcessStderrClose => {
                let state = module.layouts.state.ptr_type(space).into();
                let child = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, child], false)
            }
            RuntimeFunction::ChildProcessStdoutClose => {
                let state = module.layouts.state.ptr_type(space).into();
                let child = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, child], false)
            }
            RuntimeFunction::ChildProcessStdinClose => {
                let state = module.layouts.state.ptr_type(space).into();
                let child = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, child], false)
            }
            RuntimeFunction::ChildProcessStdinFlush => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, child], false)
            }
            RuntimeFunction::ChildProcessStdinWriteBytes => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let input = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, child, input], false)
            }
            RuntimeFunction::ChildProcessStdinWriteString => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let input = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, child, input], false)
            }
            RuntimeFunction::CpuCores => {
                let ret = context.pointer_type();

                ret.fn_type(&[], false)
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
                let fields = context.i8_type().into();
                let methods = context.i16_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[name, fields, methods], false)
            }
            RuntimeFunction::ClassProcess => {
                let name = context.pointer_type().into();
                let fields = context.i8_type().into();
                let methods = context.i16_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[name, fields, methods], false)
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
            RuntimeFunction::TimeMonotonic => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::IntToString => {
                let state = module.layouts.state.ptr_type(space).into();
                let value = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, value], false)
            }
            RuntimeFunction::ChannelDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let chan = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, chan], false)
            }
            RuntimeFunction::ChannelNew => {
                let state = module.layouts.state.ptr_type(space).into();
                let capacity = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, capacity], false)
            }
            RuntimeFunction::ChannelReceive => {
                let proc = context.pointer_type().into();
                let chan = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[proc, chan], false)
            }
            RuntimeFunction::ChannelReceiveUntil => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let channel = context.pointer_type().into();
                let time = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, channel, time], false)
            }
            RuntimeFunction::ChannelSend => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let channel = context.pointer_type().into();
                let message = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, channel, message], false)
            }
            RuntimeFunction::ChannelTryReceive => {
                let proc = context.pointer_type().into();
                let channel = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[proc, channel], false)
            }
            RuntimeFunction::ChannelWait => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let channels = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, channels], false)
            }
            RuntimeFunction::DirectoryCreate
            | RuntimeFunction::DirectoryCreateRecursive
            | RuntimeFunction::DirectoryList
            | RuntimeFunction::DirectoryRemove
            | RuntimeFunction::DirectoryRemoveAll => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, path], false)
            }
            RuntimeFunction::EnvArguments => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvExecutable => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = module.layouts.result;

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvGet => {
                let state = module.layouts.state.ptr_type(space).into();
                let name = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, name], false)
            }
            RuntimeFunction::EnvGetWorkingDirectory => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = module.layouts.result;

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvHomeDirectory => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = module.layouts.result;

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvSetWorkingDirectory => {
                let state = module.layouts.state.ptr_type(space).into();
                let path = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, path], false)
            }
            RuntimeFunction::EnvTempDirectory => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvVariables => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::Exit => {
                let status = context.i64_type().into();
                let ret = context.void_type();

                ret.fn_type(&[status], false)
            }
            RuntimeFunction::FileCopy => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let from = context.pointer_type().into();
                let to = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, from, to], false)
            }
            RuntimeFunction::FileDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let file = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, file], false)
            }
            RuntimeFunction::FileFlush => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let file = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, file], false)
            }
            RuntimeFunction::FileOpen => {
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let mode = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[proc, path, mode], false)
            }
            RuntimeFunction::FileRead => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let file = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, file, buffer, size], false)
            }
            RuntimeFunction::FileRemove => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, path], false)
            }
            RuntimeFunction::FileSeek => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let file = context.pointer_type().into();
                let offset = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, file, offset], false)
            }
            RuntimeFunction::FileSize => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, path], false)
            }
            RuntimeFunction::FileWriteBytes
            | RuntimeFunction::FileWriteString => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let file = context.pointer_type().into();
                let input = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, file, input], false)
            }
            RuntimeFunction::PathAccessedAt
            | RuntimeFunction::PathCreatedAt
            | RuntimeFunction::PathModifiedAt => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, path], false)
            }
            RuntimeFunction::PathExpand => {
                let state = module.layouts.state.ptr_type(space).into();
                let path = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, path], false)
            }
            RuntimeFunction::PathExists
            | RuntimeFunction::PathIsDirectory
            | RuntimeFunction::PathIsFile => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, path], false)
            }
            RuntimeFunction::ProcessStacktrace => {
                let proc = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[proc], false)
            }
            RuntimeFunction::ProcessStackFrameLine => {
                let state = module.layouts.state.ptr_type(space).into();
                let trace = context.pointer_type().into();
                let index = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, trace, index], false)
            }
            RuntimeFunction::ProcessStackFrameName => {
                let state = module.layouts.state.ptr_type(space).into();
                let trace = context.pointer_type().into();
                let index = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, trace, index], false)
            }
            RuntimeFunction::ProcessStackFramePath => {
                let state = module.layouts.state.ptr_type(space).into();
                let trace = context.pointer_type().into();
                let index = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, trace, index], false)
            }
            RuntimeFunction::ProcessStacktraceLength => {
                let state = module.layouts.state.ptr_type(space).into();
                let trace = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, trace], false)
            }
            RuntimeFunction::ProcessStacktraceDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let trace = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, trace], false)
            }
            RuntimeFunction::ProcessSuspend => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let time = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, time], false)
            }
            RuntimeFunction::RandomBytes => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let rng = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, rng, size], false)
            }
            RuntimeFunction::RandomDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let rng = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, rng], false)
            }
            RuntimeFunction::RandomFloat => {
                let state = module.layouts.state.ptr_type(space).into();
                let rng = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, rng], false)
            }
            RuntimeFunction::RandomFloatRange => {
                let state = module.layouts.state.ptr_type(space).into();
                let rng = context.pointer_type().into();
                let min = context.f64_type().into();
                let max = context.f64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, rng, min, max], false)
            }
            RuntimeFunction::RandomFromInt => {
                let seed = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[seed], false)
            }
            RuntimeFunction::RandomInt => {
                let state = module.layouts.state.ptr_type(space).into();
                let rng = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, rng], false)
            }
            RuntimeFunction::RandomIntRange => {
                let state = module.layouts.state.ptr_type(space).into();
                let rng = context.pointer_type().into();
                let min = context.i64_type().into();
                let max = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, rng, min, max], false)
            }
            RuntimeFunction::RandomNew => {
                let proc = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[proc], false)
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
            RuntimeFunction::StderrFlush | RuntimeFunction::StdoutFlush => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc], false)
            }
            RuntimeFunction::StdoutWriteString
            | RuntimeFunction::StdoutWriteBytes
            | RuntimeFunction::StderrWriteString
            | RuntimeFunction::StderrWriteBytes => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let input = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, input], false)
            }
            RuntimeFunction::StdinRead => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, buffer, size], false)
            }
            RuntimeFunction::StringByte => {
                let string = context.pointer_type().into();
                let index = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[string, index], false)
            }
            RuntimeFunction::StringCharacters => {
                let string = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[string], false)
            }
            RuntimeFunction::StringCharactersDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let input = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, input], false)
            }
            RuntimeFunction::StringCharactersNext => {
                let state = module.layouts.state.ptr_type(space).into();
                let input = context.pointer_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, input], false)
            }
            RuntimeFunction::StringConcat => {
                let state = module.layouts.state.ptr_type(space).into();
                let strings = context.pointer_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, strings, length], false)
            }
            RuntimeFunction::StringConcatArray => {
                let state = module.layouts.state.ptr_type(space).into();
                let strings = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, strings], false)
            }
            RuntimeFunction::StringDrop => {
                let state = module.layouts.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, string], false)
            }
            RuntimeFunction::StringNewPermanent => {
                let state = module.layouts.state.ptr_type(space).into();
                let bytes = context.pointer_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, bytes, length], false)
            }
            RuntimeFunction::StringSize => {
                let state = module.layouts.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, string], false)
            }
            RuntimeFunction::StringSliceBytes => {
                let state = module.layouts.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let start = context.i64_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, string, start, length], false)
            }
            RuntimeFunction::StringToByteArray
            | RuntimeFunction::StringToLower
            | RuntimeFunction::StringToUpper => {
                let state = module.layouts.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, string], false)
            }
            RuntimeFunction::StringToFloat => {
                let state = module.layouts.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let start = context.i64_type().into();
                let end = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, string, start, end], false)
            }
            RuntimeFunction::StringToInt => {
                let state = module.layouts.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let string = context.pointer_type().into();
                let radix = context.i64_type().into();
                let start = context.i64_type().into();
                let end = context.i64_type().into();
                let ret = module.layouts.result;

                ret.fn_type(&[state, proc, string, radix, start, end], false)
            }
            RuntimeFunction::TimeSystem | RuntimeFunction::TimeSystemOffset => {
                let state = module.layouts.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
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
