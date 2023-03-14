//! Lowering of Inko MIR into LLVM IR.
use crate::mir::{
    CloneKind, Constant, Instruction, Method, Mir, RegisterId, RegisterKind,
};
use fnv::{FnvHashMap, FnvHashSet};
use inkwell::basic_block::BasicBlock;
use inkwell::intrinsics::Intrinsic;
use inkwell::passes::{PassManager, PassManagerBuilder};
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetTriple,
};
use inkwell::types::{
    ArrayType, BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType,
    PointerType, StructType,
};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, CallableValue,
    FloatValue, FunctionValue, GlobalValue, InstructionOpcode, IntValue,
    PointerValue, StructValue,
};
use inkwell::OptimizationLevel;
use inkwell::{
    builder, context, module, AddressSpace, AtomicOrdering, AtomicRMWBinOp,
    FloatPredicate, IntPredicate,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::size_of;
use std::ops::Deref;
use std::path::Path;
use types::{
    Block, BuiltinFunction, ClassId, ConstantId, Database, MethodId,
    MethodSource, ModuleId, ARRAY_ID, BOOLEAN_ID, BYTE_ARRAY_ID, CALL_METHOD,
    CHANNEL_ID, DROPPER_METHOD, FLOAT_ID, INT_ID, NIL_ID, STRING_ID,
};

const NAME_MANGLING_VERSION: usize = 1;

/// The size of an object header.
const HEADER_SIZE: u32 = 16;

/// The size of a process, minus its fields.
const PROCESS_SIZE: u32 = 136;

/// The size of the `State` type.
const STATE_SIZE: u32 = 384;

/// The mask to use for tagged integers.
const INT_MASK: i64 = 0b001;

/// The number of bits to shift for tagged integers.
const INT_SHIFT: usize = 1;

/// The minimum integer value that can be stored as a tagged signed integer.
const MIN_INT: i64 = i64::MIN >> INT_SHIFT;

/// The maximum integer value that can be stored as a tagged signed integer.
const MAX_INT: i64 = i64::MAX >> INT_SHIFT;

/// The offset to apply to access a regular field.
///
/// The object header occupies the first field (as an inline struct), so all
/// user-defined fields start at the next field.
const FIELD_OFFSET: usize = 1;

/// The mask to use to check if a value is a tagged integer or reference.
const TAG_MASK: i64 = 0b11;

/// The mask to apply to get rid of the tagging bits.
const UNTAG_MASK: u64 = (!TAG_MASK) as u64;

/// The mask to use for checking if a value is a reference.
const REF_MASK: i64 = 0b10;

/// The field index of the `State` field that contains the `true` singleton.
const TRUE_INDEX: u32 = 0;

/// The field index of the `State` field that contains the `false` singleton.
const FALSE_INDEX: u32 = 1;

/// The field index of the `State` field that contains the `nil` singleton.
const NIL_INDEX: u32 = 2;

const HEADER_CLASS_INDEX: u32 = 0;
const HEADER_KIND_INDEX: u32 = 1;
const HEADER_REFS_INDEX: u32 = 2;

const BOXED_INT_VALUE_INDEX: u32 = 1;
const BOXED_FLOAT_VALUE_INDEX: u32 = 1;

const CLASS_METHODS_COUNT_INDEX: u32 = 2;
const CLASS_METHODS_INDEX: u32 = 3;

const METHOD_HASH_INDEX: u32 = 0;
const METHOD_FUNCTION_INDEX: u32 = 1;

// The values used to represent the kind of a value/reference. These values
// must match the values used by `Kind` in the runtime library.
const OWNED_KIND: u8 = 0;
const REF_KIND: u8 = 1;
const ATOMIC_KIND: u8 = 2;
const PERMANENT_KIND: u8 = 3;
const INT_KIND: u8 = 4;
const FLOAT_KIND: u8 = 5;

const RESULT_TAG_INDEX: u32 = 0;
const RESULT_VALUE_INDEX: u32 = 1;
const RESULT_OK_VALUE: u8 = 0;
const RESULT_ERROR_VALUE: u8 = 1;

const LLVM_RESULT_VALUE_INDEX: u32 = 0;
const LLVM_RESULT_STATUS_INDEX: u32 = 1;

const CONTEXT_STATE_INDEX: u32 = 0;
const CONTEXT_PROCESS_INDEX: u32 = 1;
const CONTEXT_ARGS_INDEX: u32 = 2;

const MESSAGE_ARGUMENTS_INDEX: u32 = 2;

const CLOSURE_CALL_INDEX: u32 = 0;
const CLOSURE_DROPPER_INDEX: u32 = 1;

/// Method table sizes are multiplied by this value in an attempt to reduce the
/// amount of collisions when performing dynamic dispatch.
///
/// While this increases the amount of memory needed per method table, it's not
/// really significant: each slot only takes up one word of memory. On a 64-bits
/// system this means you can fit a total of 131 072 slots in 1 MiB. In
/// addition, this cost is a one-time and constant cost, whereas collisions
/// introduce a cost that you may have to pay every time you perform dynamic
/// dispatch.
const METHOD_TABLE_FACTOR: usize = 4;

/// Rounds the given value to the nearest power of two.
fn round_methods(mut value: usize) -> usize {
    if value == 0 {
        return 0;
    }

    value -= 1;
    value |= value >> 1;
    value |= value >> 2;
    value |= value >> 4;
    value |= value >> 8;
    value |= value >> 16;
    value |= value >> 32;
    value += 1;

    value
}

/// A type for generating method hash codes.
///
/// These hash codes are used as part of dynamic dispatch. Each method name is
/// given a globally unique hash code. We don't need to consider the entire
/// method's signature as Inko doesn't allow overloading of methods.
///
/// The algorithm used by this hasher is FNV-1a, as it's one of the fastest
/// not-so-terrible hash function for small inputs.
struct MethodHasher<'a> {
    hashes: FnvHashMap<&'a str, u64>,
    used: FnvHashSet<u64>,
}

impl<'a> MethodHasher<'a> {
    fn new() -> MethodHasher<'a> {
        // We can't predict how many unique method names there are without
        // counting them, which would involve hashing, which in turn likely
        // wouldn't make this hasher any faster.
        //
        // Instead we conservatively assume every program needs at least this
        // many slots, reducing the amount of rehashing necessary without
        // reserving way too much memory.
        let size = 512;

        MethodHasher {
            hashes: FnvHashMap::with_capacity_and_hasher(
                size,
                Default::default(),
            ),
            used: FnvHashSet::with_capacity_and_hasher(
                size,
                Default::default(),
            ),
        }
    }

    fn hash(&mut self, name: &'a str) -> u64 {
        if let Some(&hash) = self.hashes.get(name) {
            return hash;
        }

        let mut base = 0xcbf29ce484222325;

        for &byte in name.as_bytes() {
            base = self.round(base, byte as u64);
        }

        // Bytes are in the range from 0..255. By starting the extra value at
        // 256 we're (hopefully) less likely to produce collisions with method
        // names that are one byte longer than our current method name.
        let mut extra = 256_u64;
        let mut hash = base;

        // FNV isn't a perfect hash function, so collisions are possible. In
        // this case we just add a number to the base hash until we produce a
        // unique hash.
        while self.used.contains(&hash) {
            hash = self.round(base, extra);
            extra = extra.wrapping_add(1);
        }

        self.hashes.insert(name, hash);
        self.used.insert(hash);
        hash
    }

    fn round(&self, hash: u64, value: u64) -> u64 {
        (hash ^ value).wrapping_mul(0x100_0000_01b3)
    }
}

/// A cache of mangled symbol names.
struct SymbolNames {
    classes: HashMap<ClassId, String>,
    methods: HashMap<MethodId, String>,
    constants: HashMap<ConstantId, String>,
    setup_functions: HashMap<ModuleId, String>,
}

impl SymbolNames {
    fn new(db: &Database, mir: &Mir) -> Self {
        let mut classes = HashMap::new();
        let mut methods = HashMap::new();
        let mut constants = HashMap::new();
        let mut setup_functions = HashMap::new();

        for module_index in 0..mir.modules.len() {
            let module = &mir.modules[module_index];
            let mod_name = module.id.name(db).as_str();

            for &class in &module.classes {
                let class_name = format!(
                    "_I{}T_{}::{}",
                    NAME_MANGLING_VERSION,
                    mod_name,
                    class.name(db)
                );

                classes.insert(class, class_name);

                for &method in &mir.classes[&class].methods {
                    let name = format!(
                        "_I{}M_{}::{}.{}",
                        NAME_MANGLING_VERSION,
                        mod_name,
                        class.name(db),
                        method.name(db)
                    );

                    methods.insert(method, name);
                }
            }
        }

        for id in mir.constants.keys() {
            let mod_name = id.module(db).name(db).as_str();
            let name = id.name(db);

            constants.insert(
                *id,
                format!("_I{}C_{}::{}", NAME_MANGLING_VERSION, mod_name, name),
            );
        }

        for &id in mir.modules.keys() {
            let name = format!(
                "_I{}M_{}::$setup",
                NAME_MANGLING_VERSION,
                id.name(db).as_str()
            );

            setup_functions.insert(id, name);
        }

        Self { classes, methods, constants, setup_functions }
    }
}

#[derive(Copy, Clone)]
enum RuntimeFunction {
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
    ClassDrop,
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
    EnvPlatform,
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
    HasherDrop,
    HasherNew,
    HasherToHash,
    HasherWriteInt,
    IntBoxed,
    IntBoxedPermanent,
    IntClone,
    IntOverflow,
    IntPow,
    IntToString,
    MessageNew,
    MethodNew,
    ObjectNew,
    PathAccessedAt,
    PathCreatedAt,
    PathExists,
    PathIsDirectory,
    PathIsFile,
    PathModifiedAt,
    ProcessFinishMessage,
    ProcessNew,
    ProcessPanic,
    ProcessPopStackFrame,
    ProcessPushStackFrame,
    ProcessSendMessage,
    ProcessStackFrameLine,
    ProcessStackFrameName,
    ProcessStackFramePath,
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
    StringNew,
    StringNewPermanent,
    StringSize,
    StringSliceBytes,
    StringToByteArray,
    StringToCString,
    StringToFloat,
    StringToInt,
    StringToLower,
    StringToUpper,
    TimeMonotonic,
    TimeSystem,
    TimeSystemOffset,
}

impl RuntimeFunction {
    fn name(self) -> &'static str {
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
            RuntimeFunction::ClassDrop => "inko_class_drop",
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
            RuntimeFunction::EnvPlatform => "inko_env_platform",
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
            RuntimeFunction::HasherDrop => "inko_hasher_drop",
            RuntimeFunction::HasherNew => "inko_hasher_new",
            RuntimeFunction::HasherToHash => "inko_hasher_to_hash",
            RuntimeFunction::HasherWriteInt => "inko_hasher_write_int",
            RuntimeFunction::IntBoxed => "inko_int_boxed",
            RuntimeFunction::IntBoxedPermanent => "inko_int_boxed_permanent",
            RuntimeFunction::IntClone => "inko_int_clone",
            RuntimeFunction::IntOverflow => "inko_int_overflow",
            RuntimeFunction::IntPow => "inko_int_pow",
            RuntimeFunction::IntToString => "inko_int_to_string",
            RuntimeFunction::MessageNew => "inko_message_new",
            RuntimeFunction::MethodNew => "inko_method_new",
            RuntimeFunction::ObjectNew => "inko_object_new",
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
            RuntimeFunction::ProcessPopStackFrame => {
                "inko_process_pop_stack_frame"
            }
            RuntimeFunction::ProcessPushStackFrame => {
                "inko_process_push_stack_frame"
            }
            RuntimeFunction::ProcessSendMessage => "inko_process_send_message",
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
            RuntimeFunction::StringNew => "inko_string_new",
            RuntimeFunction::StringNewPermanent => "inko_string_new_permanent",
            RuntimeFunction::StringSize => "inko_string_size",
            RuntimeFunction::StringSliceBytes => "inko_string_slice_bytes",
            RuntimeFunction::StringToByteArray => "inko_string_to_byte_array",
            RuntimeFunction::StringToCString => "inko_string_to_cstring",
            RuntimeFunction::StringToFloat => "inko_string_to_float",
            RuntimeFunction::StringToInt => "inko_string_to_int",
            RuntimeFunction::StringToLower => "inko_string_to_lower",
            RuntimeFunction::StringToUpper => "inko_string_to_upper",
            RuntimeFunction::TimeMonotonic => "inko_time_monotonic",
            RuntimeFunction::TimeSystem => "inko_time_system",
            RuntimeFunction::TimeSystemOffset => "inko_time_system_offset",
        }
    }

    fn build<'a, 'ctx>(self, module: &Module<'a, 'ctx>) -> FunctionValue<'ctx> {
        let context = module.context;
        let space = AddressSpace::Generic;
        let fn_type = match self {
            RuntimeFunction::IntBoxedPermanent => {
                let state = module.types.state.ptr_type(space).into();
                let val = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::IntBoxed => {
                let state = module.types.state.ptr_type(space).into();
                let val = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::IntClone => {
                let state = module.types.state.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let val = context.f64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::FloatClone => {
                let state = module.types.state.ptr_type(space).into();
                let val = context.pointer_type();

                val.fn_type(&[state, val.into()], false)
            }
            RuntimeFunction::ArrayNewPermanent => {
                let state = module.types.state.ptr_type(space).into();
                let len = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, len], false)
            }
            RuntimeFunction::ArrayNew => {
                let state = module.types.state.ptr_type(space).into();
                let len = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, len], false)
            }
            RuntimeFunction::ArrayPush => {
                let state = module.types.state.ptr_type(space).into();
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
            RuntimeFunction::ObjectNew => {
                let class = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[class], false)
            }
            RuntimeFunction::StringEquals => {
                let state = module.types.state.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let val = context.f64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::FloatEq => {
                let state = module.types.state.ptr_type(space).into();
                let lhs = context.f64_type().into();
                let rhs = context.f64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, lhs, rhs], false)
            }
            RuntimeFunction::FloatRound => {
                let state = module.types.state.ptr_type(space).into();
                let lhs = context.f64_type().into();
                let rhs = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, lhs, rhs], false)
            }
            RuntimeFunction::FloatToString => {
                let state = module.types.state.ptr_type(space).into();
                let val = context.f64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::ArrayCapacity => {
                let state = module.types.state.ptr_type(space).into();
                let val = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::ArrayClear => {
                let state = module.types.state.ptr_type(space).into();
                let val = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, val], false)
            }
            RuntimeFunction::ArrayDrop => {
                let state = module.types.state.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ArrayPop => {
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[array], false)
            }
            RuntimeFunction::ArrayRemove => {
                let array = context.pointer_type().into();
                let index = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[array, index], false)
            }
            RuntimeFunction::ArrayReserve => {
                let state = module.types.state.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::ByteArrayPush => {
                let state = module.types.state.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayEq => {
                let state = module.types.state.ptr_type(space).into();
                let lhs = context.pointer_type().into();
                let rhs = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, lhs, rhs], false)
            }
            RuntimeFunction::ByteArrayClear => {
                let state = module.types.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayClone => {
                let state = module.types.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayDrop => {
                let state = module.types.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayToString => {
                let state = module.types.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArrayDrainToString => {
                let state = module.types.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array], false)
            }
            RuntimeFunction::ByteArraySlice => {
                let state = module.types.state.ptr_type(space).into();
                let array = context.pointer_type().into();
                let start = context.i64_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, array, start, length], false)
            }
            RuntimeFunction::ByteArrayAppend => {
                let state = module.types.state.ptr_type(space).into();
                let target = context.pointer_type().into();
                let source = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, target, source], false)
            }
            RuntimeFunction::ByteArrayCopyFrom => {
                let state = module.types.state.ptr_type(space).into();
                let target = context.pointer_type().into();
                let source = context.pointer_type().into();
                let start = context.i64_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, target, source, start, length], false)
            }
            RuntimeFunction::ByteArrayResize => {
                let state = module.types.state.ptr_type(space).into();
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
                let ret = module.types.result;

                ret.fn_type(
                    &[proc, program, args, env, stdin, stdout, stderr, dir],
                    false,
                )
            }
            RuntimeFunction::ChildProcessWait => {
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[proc, child], false)
            }
            RuntimeFunction::ChildProcessTryWait => {
                let child = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[child], false)
            }
            RuntimeFunction::ChildProcessDrop => {
                let state = module.types.state.ptr_type(space).into();
                let child = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, child], false)
            }
            RuntimeFunction::ChildProcessStdoutRead => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, child, buffer, size], false)
            }
            RuntimeFunction::ChildProcessStderrRead => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, child, buffer, size], false)
            }
            RuntimeFunction::ChildProcessStderrClose => {
                let state = module.types.state.ptr_type(space).into();
                let child = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, child], false)
            }
            RuntimeFunction::ChildProcessStdoutClose => {
                let state = module.types.state.ptr_type(space).into();
                let child = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, child], false)
            }
            RuntimeFunction::ChildProcessStdinClose => {
                let state = module.types.state.ptr_type(space).into();
                let child = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, child], false)
            }
            RuntimeFunction::ChildProcessStdinFlush => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, child], false)
            }
            RuntimeFunction::ChildProcessStdinWriteBytes => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let input = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, child, input], false)
            }
            RuntimeFunction::ChildProcessStdinWriteString => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let child = context.pointer_type().into();
                let input = context.pointer_type().into();
                let ret = module.types.result;

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
                let counts = module.types.method_counts.ptr_type(space).into();
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
            RuntimeFunction::MethodNew => {
                let hash = context.i64_type().into();
                let code = context.pointer_type().into();
                let ret = context.method_type();

                ret.fn_type(&[hash, code], false)
            }
            RuntimeFunction::MessageNew => {
                let method = context.pointer_type().into();
                let length = context.i8_type().into();
                let ret = module.types.message.ptr_type(space);

                ret.fn_type(&[method, length], false)
            }
            RuntimeFunction::ProcessSendMessage => {
                let state = module.types.state.ptr_type(space).into();
                let sender = context.pointer_type().into();
                let receiver = context.pointer_type().into();
                let message = module.types.message.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::IntToString => {
                let state = module.types.state.ptr_type(space).into();
                let value = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, value], false)
            }
            RuntimeFunction::ChannelDrop => {
                let state = module.types.state.ptr_type(space).into();
                let chan = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, chan], false)
            }
            RuntimeFunction::ChannelNew => {
                let state = module.types.state.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let channel = context.pointer_type().into();
                let time = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, channel, time], false)
            }
            RuntimeFunction::ChannelSend => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let channel = context.pointer_type().into();
                let message = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, channel, message], false)
            }
            RuntimeFunction::ChannelTryReceive => {
                let proc = context.pointer_type().into();
                let channel = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[proc, channel], false)
            }
            RuntimeFunction::ChannelWait => {
                let proc = context.pointer_type().into();
                let channels = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[proc, channels], false)
            }
            RuntimeFunction::ClassDrop => {
                let class = context.pointer_type().into();
                let ret = context.void_type();

                ret.fn_type(&[class], false)
            }
            RuntimeFunction::DirectoryCreate
            | RuntimeFunction::DirectoryCreateRecursive
            | RuntimeFunction::DirectoryList
            | RuntimeFunction::DirectoryRemove
            | RuntimeFunction::DirectoryRemoveAll => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, path], false)
            }
            RuntimeFunction::EnvArguments => {
                let state = module.types.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvExecutable => {
                let state = module.types.state.ptr_type(space).into();
                let ret = module.types.result;

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvGet => {
                let state = module.types.state.ptr_type(space).into();
                let name = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, name], false)
            }
            RuntimeFunction::EnvGetWorkingDirectory => {
                let state = module.types.state.ptr_type(space).into();
                let ret = module.types.result;

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvHomeDirectory => {
                let state = module.types.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvPlatform => {
                context.pointer_type().fn_type(&[], false)
            }
            RuntimeFunction::EnvSetWorkingDirectory => {
                let state = module.types.state.ptr_type(space).into();
                let path = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, path], false)
            }
            RuntimeFunction::EnvTempDirectory => {
                let state = module.types.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::EnvVariables => {
                let state = module.types.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::Exit => {
                let status = context.i64_type().into();
                let ret = context.void_type();

                ret.fn_type(&[status], false)
            }
            RuntimeFunction::FileCopy => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let from = context.pointer_type().into();
                let to = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, from, to], false)
            }
            RuntimeFunction::FileDrop => {
                let state = module.types.state.ptr_type(space).into();
                let file = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, file], false)
            }
            RuntimeFunction::FileFlush => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let file = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, file], false)
            }
            RuntimeFunction::FileOpen => {
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let mode = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[proc, path, mode], false)
            }
            RuntimeFunction::FileRead => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let file = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, file, buffer, size], false)
            }
            RuntimeFunction::FileRemove => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, path], false)
            }
            RuntimeFunction::FileSeek => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let file = context.pointer_type().into();
                let offset = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, file, offset], false)
            }
            RuntimeFunction::FileSize => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, path], false)
            }
            RuntimeFunction::FileWriteBytes
            | RuntimeFunction::FileWriteString => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let file = context.pointer_type().into();
                let input = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, file, input], false)
            }
            RuntimeFunction::HasherDrop => {
                let state = module.types.state.ptr_type(space).into();
                let hasher = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, hasher], false)
            }
            RuntimeFunction::HasherNew => {
                let state = module.types.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::HasherToHash => {
                let state = module.types.state.ptr_type(space).into();
                let hasher = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, hasher], false)
            }
            RuntimeFunction::HasherWriteInt => {
                let state = module.types.state.ptr_type(space).into();
                let hasher = context.pointer_type().into();
                let value = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, hasher, value], false)
            }
            RuntimeFunction::PathAccessedAt
            | RuntimeFunction::PathCreatedAt
            | RuntimeFunction::PathModifiedAt => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, path], false)
            }
            RuntimeFunction::PathExists
            | RuntimeFunction::PathIsDirectory
            | RuntimeFunction::PathIsFile => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let path = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, path], false)
            }
            // TODO: what to do with these?
            RuntimeFunction::ProcessPopStackFrame => todo!(),
            RuntimeFunction::ProcessPushStackFrame => todo!(),
            RuntimeFunction::ProcessStackFrameLine => todo!(),
            RuntimeFunction::ProcessStackFrameName => todo!(),
            RuntimeFunction::ProcessStackFramePath => todo!(),
            RuntimeFunction::ProcessStacktraceLength => todo!(),
            RuntimeFunction::ProcessSuspend => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let time = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, time], false)
            }
            RuntimeFunction::RandomBytes => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let rng = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, rng, size], false)
            }
            RuntimeFunction::RandomDrop => {
                let state = module.types.state.ptr_type(space).into();
                let rng = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, rng], false)
            }
            RuntimeFunction::RandomFloat => {
                let state = module.types.state.ptr_type(space).into();
                let rng = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, rng], false)
            }
            RuntimeFunction::RandomFloatRange => {
                let state = module.types.state.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let rng = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, rng], false)
            }
            RuntimeFunction::RandomIntRange => {
                let state = module.types.state.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let deadline = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, socket, deadline], false)
            }
            RuntimeFunction::SocketAddressPairAddress => {
                let pair = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[pair], false)
            }
            RuntimeFunction::SocketAddressPairDrop => {
                let state = module.types.state.ptr_type(space).into();
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
                let state = module.types.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let address = context.pointer_type().into();
                let port = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, socket, address, port], false)
            }
            RuntimeFunction::SocketConnect => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let address = context.pointer_type().into();
                let port = context.i64_type().into();
                let deadline = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(
                    &[state, proc, socket, address, port, deadline],
                    false,
                )
            }
            RuntimeFunction::SocketDrop => {
                let state = module.types.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, socket], false)
            }
            RuntimeFunction::SocketListen => {
                let state = module.types.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let value = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, socket, value], false)
            }
            RuntimeFunction::SocketLocalAddress
            | RuntimeFunction::SocketPeerAddress => {
                let state = module.types.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, socket], false)
            }
            RuntimeFunction::SocketRead
            | RuntimeFunction::SocketReceiveFrom => {
                let state = module.types.state.ptr_type(space).into();
                let process = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let amount = context.i64_type().into();
                let deadline = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(
                    &[state, process, socket, buffer, amount, deadline],
                    false,
                )
            }
            RuntimeFunction::SocketSendBytesTo
            | RuntimeFunction::SocketSendStringTo => {
                let state = module.types.state.ptr_type(space).into();
                let process = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let address = context.pointer_type().into();
                let port = context.i64_type().into();
                let deadline = context.i64_type().into();
                let ret = module.types.result;

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
                let state = module.types.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let value = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, socket, value], false)
            }
            RuntimeFunction::SocketSetLinger
            | RuntimeFunction::SocketSetRecvSize
            | RuntimeFunction::SocketSetSendSize
            | RuntimeFunction::SocketSetTtl => {
                let state = module.types.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let value = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, socket, value], false)
            }
            RuntimeFunction::SocketShutdownRead
            | RuntimeFunction::SocketShutdownReadWrite
            | RuntimeFunction::SocketShutdownWrite => {
                let state = module.types.state.ptr_type(space).into();
                let socket = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, socket], false)
            }
            RuntimeFunction::SocketTryClone => {
                let socket = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[socket], false)
            }
            RuntimeFunction::StderrFlush | RuntimeFunction::StdoutFlush => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc], false)
            }
            RuntimeFunction::StdoutWriteString
            | RuntimeFunction::StdoutWriteBytes
            | RuntimeFunction::StderrWriteString
            | RuntimeFunction::StderrWriteBytes => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let input = context.pointer_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, proc, input], false)
            }
            RuntimeFunction::StdinRead => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let size = context.i64_type().into();
                let ret = module.types.result;

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
            RuntimeFunction::StringCharactersDrop
            | RuntimeFunction::StringCharactersNext => {
                let state = module.types.state.ptr_type(space).into();
                let input = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, input], false)
            }
            RuntimeFunction::StringConcat => {
                let state = module.types.state.ptr_type(space).into();
                let strings = context.pointer_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, strings, length], false)
            }
            RuntimeFunction::StringConcatArray => {
                let state = module.types.state.ptr_type(space).into();
                let strings = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, strings], false)
            }
            RuntimeFunction::StringDrop => {
                let state = module.types.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, string], false)
            }
            RuntimeFunction::StringNew
            | RuntimeFunction::StringNewPermanent => {
                let state = module.types.state.ptr_type(space).into();
                let bytes = context.pointer_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, bytes, length], false)
            }
            RuntimeFunction::StringSize => {
                let state = module.types.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, string], false)
            }
            RuntimeFunction::StringSliceBytes => {
                let state = module.types.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let start = context.i64_type().into();
                let length = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, string, start, length], false)
            }
            RuntimeFunction::StringToByteArray
            | RuntimeFunction::StringToLower
            | RuntimeFunction::StringToUpper => {
                let state = module.types.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, string], false)
            }
            RuntimeFunction::StringToCString => {
                let string = context.pointer_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[string], false)
            }
            RuntimeFunction::StringToFloat => {
                let state = module.types.state.ptr_type(space).into();
                let string = context.pointer_type().into();
                let start = context.i64_type().into();
                let end = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, string, start, end], false)
            }
            RuntimeFunction::StringToInt => {
                let state = module.types.state.ptr_type(space).into();
                let proc = context.pointer_type().into();
                let string = context.pointer_type().into();
                let radix = context.i64_type().into();
                let start = context.i64_type().into();
                let end = context.i64_type().into();
                let ret = context.pointer_type();

                ret.fn_type(&[state, proc, string, radix, start, end], false)
            }
            RuntimeFunction::TimeSystem | RuntimeFunction::TimeSystemOffset => {
                let state = module.types.state.ptr_type(space).into();
                let ret = context.pointer_type();

                ret.fn_type(&[state], false)
            }
            RuntimeFunction::SocketNew => {
                let proto = context.i64_type().into();
                let kind = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[proto, kind], false)
            }
            RuntimeFunction::SocketWriteBytes
            | RuntimeFunction::SocketWriteString => {
                let state = module.types.state.ptr_type(space).into();
                let process = context.pointer_type().into();
                let socket = context.pointer_type().into();
                let buffer = context.pointer_type().into();
                let deadline = context.i64_type().into();
                let ret = module.types.result;

                ret.fn_type(&[state, process, socket, buffer, deadline], false)
            }
        };

        module.add_function(self.name(), fn_type, None)
    }
}

/// A wrapper around an LLVM Context that provides some additional methods.
struct Context {
    inner: context::Context,
}

impl Context {
    fn new() -> Self {
        Self { inner: context::Context::create() }
    }

    fn pointer_type<'a>(&'a self) -> PointerType<'a> {
        self.inner.i8_type().ptr_type(AddressSpace::Generic)
    }

    fn rust_string_type<'a>(&'a self) -> ArrayType<'a> {
        self.inner.i8_type().array_type(size_of::<String>() as u32)
    }

    fn rust_vec_type<'a>(&'a self) -> ArrayType<'a> {
        self.inner.i8_type().array_type(size_of::<Vec<()>>() as u32)
    }

    fn method_type<'a>(&'a self) -> StructType<'a> {
        self.inner.struct_type(
            &[
                self.inner.i64_type().into(), // Hash
                self.pointer_type().into(),   // Function pointer
            ],
            false,
        )
    }

    fn class_type<'a>(&'a self, methods: usize, name: &str) -> StructType<'a> {
        let name_type = self.rust_string_type();
        let class_type = self.inner.opaque_struct_type(name);

        class_type.set_body(
            &[
                name_type.into(),             // Name
                self.inner.i32_type().into(), // Instance size
                self.inner.i16_type().into(), // Number of methods
                self.method_type().array_type(methods as u32).into(),
            ],
            false,
        );

        class_type
    }

    /// Returns the layout for a built-in type such as Int or String (i.e a type
    /// with only a single value field).
    fn builtin_type<'a>(
        &'a self,
        name: &str,
        header: StructType<'a>,
        value: BasicTypeEnum,
    ) -> StructType<'a> {
        let typ = self.inner.opaque_struct_type(name);

        typ.set_body(&[header.into(), value], false);
        typ
    }
}

impl Deref for Context {
    type Target = context::Context;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// A wrapper around an LLVM Builder that provides some additional methods.
struct Builder<'a, 'ctx> {
    inner: builder::Builder<'ctx>,
    context: &'ctx Context,
    types: &'a Types<'ctx>,
}

impl<'a, 'ctx> Builder<'a, 'ctx> {
    fn new(context: &'ctx Context, types: &'a Types<'ctx>) -> Self {
        Self { inner: context.create_builder(), context, types }
    }

    fn extract_field(
        &self,
        receiver: StructValue<'ctx>,
        index: u32,
    ) -> BasicValueEnum<'ctx> {
        self.inner.build_extract_value(receiver, index, "").unwrap()
    }

    fn load_field(
        &self,
        receiver: PointerValue<'ctx>,
        index: u32,
    ) -> BasicValueEnum<'ctx> {
        let field_ptr =
            self.inner.build_struct_gep(receiver, index, "").unwrap();

        self.inner.build_load(field_ptr, "")
    }

    fn load_array_index(
        &self,
        receiver: PointerValue<'ctx>,
        index: usize,
    ) -> Result<BasicValueEnum<'ctx>, ()> {
        let rec_typ = receiver.get_type().get_element_type();

        if !rec_typ.is_array_type() {
            return Err(());
        }

        let ptr = unsafe {
            self.inner.build_gep(
                receiver,
                &[
                    self.context.i32_type().const_int(0, false),
                    self.context.i32_type().const_int(index as _, false),
                ],
                "",
            )
        };

        Ok(self.inner.build_load(ptr, ""))
    }

    fn store_array_field<V: BasicValue<'ctx>>(
        &self,
        receiver: PointerValue<'ctx>,
        index: u32,
        value: V,
    ) {
        let rec_typ = receiver.get_type().get_element_type();

        if !rec_typ.is_array_type() {
            panic!(
                "The receiver of the array store isn't a pointer to an array"
            );
        }

        let ptr = unsafe {
            self.inner.build_gep(
                receiver,
                &[
                    self.context.i32_type().const_int(0, false),
                    self.context.i32_type().const_int(index as _, false),
                ],
                "",
            )
        };

        self.store(ptr, value);
    }

    fn store_field<V: BasicValue<'ctx>>(
        &self,
        receiver: PointerValue<'ctx>,
        index: u32,
        value: V,
    ) {
        let field_ptr =
            self.inner.build_struct_gep(receiver, index, "").unwrap();

        self.store(field_ptr, value);
    }

    fn load_global_to_stack(
        &self,
        variable: PointerValue<'ctx>,
        global: GlobalValue<'ctx>,
    ) {
        self.store(variable, self.load(global.as_pointer_value()));
    }

    fn store<V: BasicValue<'ctx>>(
        &self,
        variable: PointerValue<'ctx>,
        value: V,
    ) {
        self.inner.build_store(variable, value);
    }

    fn load(&self, variable: PointerValue<'ctx>) -> BasicValueEnum<'ctx> {
        self.inner.build_load(variable, "")
    }

    fn load_pointer(&self, variable: PointerValue<'ctx>) -> PointerValue<'ctx> {
        self.load(variable).into_pointer_value()
    }

    fn call<F: Into<CallableValue<'ctx>>>(
        &self,
        function: F,
        arguments: &[BasicMetadataValueEnum<'ctx>],
    ) -> BasicValueEnum<'ctx> {
        self.inner
            .build_call(function, arguments, "")
            .try_as_basic_value()
            .left()
            .unwrap()
    }

    fn call_void(
        &self,
        function: FunctionValue<'ctx>,
        arguments: &[BasicMetadataValueEnum<'ctx>],
    ) {
        self.inner.build_call(function, arguments, "");
    }

    fn cast_to_pointer(&self, value: PointerValue<'ctx>) -> PointerValue<'ctx> {
        self.inner
            .build_bitcast(value, self.context.pointer_type(), "")
            .into_pointer_value()
    }

    fn cast_to_typed_pointer(
        &self,
        value: PointerValue<'ctx>,
        layout: StructType<'ctx>,
    ) -> PointerValue<'ctx> {
        self.inner
            .build_bitcast(value, layout.ptr_type(AddressSpace::Generic), "")
            .into_pointer_value()
    }

    fn cast_to_header(&self, value: PointerValue<'ctx>) -> PointerValue<'ctx> {
        let typ = self.types.header.ptr_type(AddressSpace::Generic);

        self.inner.build_bitcast(value, typ, "").into_pointer_value()
    }

    fn cast_pointer_to_int(&self, value: PointerValue<'ctx>) -> IntValue<'ctx> {
        self.inner.build_ptr_to_int(value, self.context.i64_type(), "")
    }

    fn cast_int_to_pointer(&self, value: IntValue<'ctx>) -> PointerValue<'ctx> {
        self.inner.build_int_to_ptr(
            value,
            self.context.i8_type().ptr_type(AddressSpace::Generic),
            "",
        )
    }

    fn cast_int_to_typed_pointer(
        &self,
        value: IntValue<'ctx>,
        layout: StructType<'ctx>,
    ) -> PointerValue<'ctx> {
        self.inner.build_int_to_ptr(
            value,
            layout.ptr_type(AddressSpace::Generic),
            "",
        )
    }

    fn u8_literal(&self, value: u8) -> IntValue<'ctx> {
        self.context.i8_type().const_int(value as u64, false)
    }

    fn i64_literal(&self, value: i64) -> IntValue<'ctx> {
        self.u64_literal(value as u64)
    }

    fn u16_literal(&self, value: u16) -> IntValue<'ctx> {
        self.context.i16_type().const_int(value as u64, false)
    }

    fn u32_literal(&self, value: u32) -> IntValue<'ctx> {
        self.context.i32_type().const_int(value as u64, false)
    }

    fn u64_literal(&self, value: u64) -> IntValue<'ctx> {
        self.context.i64_type().const_int(value, false)
    }

    fn f64_literal(&self, value: f64) -> FloatValue<'ctx> {
        self.context.f64_type().const_float(value)
    }

    fn string_literal(
        &self,
        value: &str,
    ) -> (PointerValue<'ctx>, IntValue<'ctx>) {
        let string = self.build_global_string_ptr(value, "").as_pointer_value();
        let len = self.u64_literal(value.len() as _);

        (string, len)
    }

    fn int_eq(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_int_compare(IntPredicate::EQ, lhs, rhs, "")
    }

    fn int_gt(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_int_compare(IntPredicate::SGT, lhs, rhs, "")
    }

    fn int_ge(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_int_compare(IntPredicate::SGE, lhs, rhs, "")
    }

    fn int_lt(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_int_compare(IntPredicate::SLT, lhs, rhs, "")
    }

    fn int_le(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_int_compare(IntPredicate::SLE, lhs, rhs, "")
    }

    fn int_sub(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_int_sub(lhs, rhs, "")
    }

    fn int_add(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_int_add(lhs, rhs, "")
    }

    fn int_mul(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_int_mul(lhs, rhs, "")
    }

    fn bit_and(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_and(lhs, rhs, "")
    }

    fn bit_or(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_or(lhs, rhs, "")
    }

    fn left_shift(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_left_shift(lhs, rhs, "")
    }

    fn right_shift(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_right_shift(lhs, rhs, false, "")
    }

    fn signed_right_shift(
        &self,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_right_shift(lhs, rhs, true, "")
    }

    fn float_eq(
        &self,
        lhs: FloatValue<'ctx>,
        rhs: FloatValue<'ctx>,
    ) -> IntValue<'ctx> {
        self.build_float_compare(FloatPredicate::OEQ, lhs, rhs, "")
    }

    fn tagged_int(&self, value: i64) -> Option<PointerValue<'ctx>> {
        if value >= MIN_INT && value <= MAX_INT {
            let addr = (value << INT_SHIFT) | INT_MASK;
            let int = self.i64_literal(addr);

            Some(int.const_to_pointer(self.context.pointer_type()))
        } else {
            None
        }
    }

    fn cast_to_untagged_pointer(
        &self,
        pointer: PointerValue<'ctx>,
        layout: StructType<'ctx>,
    ) -> PointerValue<'ctx> {
        let tagged_addr = self.cast_pointer_to_int(pointer);
        let mask = self.u64_literal(UNTAG_MASK);
        let addr = self.bit_and(tagged_addr, mask);

        self.cast_int_to_typed_pointer(addr, layout)
    }
}

impl<'a, 'ctx> Deref for Builder<'a, 'ctx> {
    type Target = builder::Builder<'ctx>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// A wrapper around an LLVM Module that provides some additional methods.
struct Module<'a, 'ctx> {
    inner: module::Module<'ctx>,
    context: &'ctx Context,

    /// The name of the module.
    name: String,

    /// The global types available to this module (i.e. all Inko class layouts).
    types: &'a Types<'ctx>,

    /// The literals defined in this module, and their corresponding global
    /// variables.
    ///
    /// This mapping only includes Int, Float and String literals.
    literals: HashMap<Constant, GlobalValue<'ctx>>,
}

impl<'a, 'ctx> Module<'a, 'ctx> {
    fn new(
        context: &'ctx Context,
        types: &'a Types<'ctx>,
        name: String,
    ) -> Self {
        Self {
            inner: context.create_module(&name),
            context,
            name,
            types,
            literals: HashMap::new(),
        }
    }

    fn add_global(&self, name: &str) -> GlobalValue<'ctx> {
        let typ = self.context.pointer_type();
        let space = AddressSpace::Generic;

        self.inner.add_global(typ, Some(space), name)
    }

    fn add_literal(&mut self, value: &Constant) -> GlobalValue<'ctx> {
        if let Some(&global) = self.literals.get(value) {
            global
        } else {
            let name = format!(
                "_I{}L_{}_{}",
                NAME_MANGLING_VERSION,
                self.name,
                self.literals.len()
            );

            let global = self.add_global(&name);
            global.set_initializer(
                &self.context.pointer_type().const_null().as_basic_value_enum(),
            );

            self.literals.insert(value.clone(), global);
            global
        }
    }

    fn add_constant(&mut self, name: &str) -> GlobalValue<'ctx> {
        self.inner.get_global(name).unwrap_or_else(|| self.add_global(name))
    }

    fn add_class(&mut self, id: ClassId, name: &str) -> GlobalValue<'ctx> {
        self.inner.get_global(name).unwrap_or_else(|| {
            let space = AddressSpace::Generic;
            let typ = self.types.classes[&id].ptr_type(space);

            self.inner.add_global(typ, Some(space), name)
        })
    }

    fn add_method(&self, name: &str, method: MethodId) -> FunctionValue<'ctx> {
        self.inner.get_function(name).unwrap_or_else(|| {
            self.inner.add_function(
                name,
                self.types.methods[&method].signature,
                None,
            )
        })
    }

    fn add_setup_function(&self, name: &str) -> FunctionValue<'ctx> {
        if let Some(func) = self.inner.get_function(name) {
            func
        } else {
            let space = AddressSpace::Generic;
            let args = [self.types.state.ptr_type(space).into()];
            let typ = self.context.void_type().fn_type(&args, false);

            self.inner.add_function(name, typ, None)
        }
    }

    fn runtime_function(
        &self,
        function: RuntimeFunction,
    ) -> FunctionValue<'ctx> {
        self.inner
            .get_function(function.name())
            .unwrap_or_else(|| function.build(&self))
    }
}

impl<'a, 'ctx> Deref for Module<'a, 'ctx> {
    type Target = module::Module<'ctx>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

struct MethodInfo<'ctx> {
    index: u16,
    hash: u64,
    collision: bool,
    signature: FunctionType<'ctx>,
}

/// Types and layout information to expose to all modules.
struct Types<'ctx> {
    /// The layout of an empty class.
    ///
    /// This is used for generating dynamic dispatch code, as we don't know the
    /// exact class in such cases.
    empty_class: StructType<'ctx>,

    /// All MIR classes and their corresponding structure layouts.
    classes: HashMap<ClassId, StructType<'ctx>>,

    /// The structure layouts for all class instances.
    instances: HashMap<ClassId, StructType<'ctx>>,

    /// The structure layout of the runtime's `State` type.
    state: StructType<'ctx>,

    /// The layout of object headers.
    header: StructType<'ctx>,

    /// The layout of the runtime's result type.
    result: StructType<'ctx>,

    /// The layout of the context type passed to async methods.
    context: StructType<'ctx>,

    /// The layout to use for the type that stores the built-in type method
    /// counts.
    method_counts: StructType<'ctx>,

    /// Information about methods defined on classes, such as their signatures
    /// and hash codes.
    methods: HashMap<MethodId, MethodInfo<'ctx>>,

    /// The layout of messages sent to processes.
    message: StructType<'ctx>,
}

impl<'ctx> Types<'ctx> {
    fn new(context: &'ctx Context, db: &Database, mir: &Mir) -> Self {
        let space = AddressSpace::Generic;
        let mut class_layouts = HashMap::new();
        let mut instance_layouts = HashMap::new();
        let mut methods = HashMap::new();
        let header = context.struct_type(
            &[
                context.pointer_type().into(),   // Class
                context.inner.i8_type().into(),  // Kind
                context.inner.i32_type().into(), // References
            ],
            false,
        );

        let state_layout = context.struct_type(
            &[
                context.i8_type().ptr_type(AddressSpace::Generic).into(),
                context.i8_type().ptr_type(AddressSpace::Generic).into(),
                context.i8_type().ptr_type(AddressSpace::Generic).into(),
                // We don't care about the rest of the State type, so we just
                // pad it with the remaining bytes.
                context.i8_type().array_type(STATE_SIZE - 24).into(),
            ],
            false,
        );

        let context_layout = context.struct_type(
            &[
                state_layout.ptr_type(space).into(), // State
                context.pointer_type().into(),       // Process
                context.pointer_type().into(),       // Arguments pointer
            ],
            false,
        );

        let result_layout = context.struct_type(
            &[
                context.i8_type().into(),                 // Tag
                context.i8_type().ptr_type(space).into(), // Value
            ],
            false,
        );

        let method_counts_layout = context.struct_type(
            &[
                context.i16_type().into(), // Int
                context.i16_type().into(), // Float
                context.i16_type().into(), // String
                context.i16_type().into(), // Array
                context.i16_type().into(), // Bool
                context.i16_type().into(), // Nil
                context.i16_type().into(), // ByteArray
                context.i16_type().into(), // Channel
            ],
            false,
        );

        let message_layout = context.struct_type(
            &[
                context.pointer_type().into(), // Function
                context.i8_type().into(),      // Length
                context.pointer_type().array_type(0).into(), // Arguments
            ],
            false,
        );

        let mut method_hasher = MethodHasher::new();

        // We need to define the method information for trait methods, as
        // this information is necessary when generating dynamic dispatch code.
        //
        // This information is defined first so we can update the `collision`
        // flag when generating this information for method implementations.
        for mir_trait in mir.traits.values() {
            for method in mir_trait
                .id
                .required_methods(db)
                .into_iter()
                .chain(mir_trait.id.default_methods(db))
            {
                let name = method.name(db);
                let hash = method_hasher.hash(name);
                let mut args: Vec<BasicMetadataTypeEnum> = vec![
                    state_layout.ptr_type(space).into(), // State
                    context.pointer_type().into(),       // Process
                    context.pointer_type().into(),       // Receiver
                ];

                for _ in 0..method.number_of_arguments(db) {
                    args.push(context.pointer_type().into());
                }

                let signature = if method.throw_type(db).is_never(db) {
                    context.pointer_type().fn_type(&args, false)
                } else {
                    result_layout.fn_type(&args, false)
                };

                methods.insert(
                    method,
                    MethodInfo { index: 0, hash, signature, collision: false },
                );
            }
        }

        for (id, mir_class) in &mir.classes {
            // We size classes larger than actually needed in an attempt to
            // reduce collisions when performing dynamic dispatch.
            let methods_len =
                round_methods(mir_class.methods.len()) * METHOD_TABLE_FACTOR;
            let name =
                format!("{}::{}", id.module(db).name(db).as_str(), id.name(db));
            let class =
                context.class_type(methods_len, &format!("{}::class", name));
            let instance = match id.0 {
                INT_ID => context.builtin_type(
                    &name,
                    header,
                    context.i64_type().into(),
                ),
                FLOAT_ID => context.builtin_type(
                    &name,
                    header,
                    context.f64_type().into(),
                ),
                STRING_ID => context.builtin_type(
                    &name,
                    header,
                    context.pointer_type().into(),
                ),
                ARRAY_ID => context.builtin_type(
                    &name,
                    header,
                    context.rust_vec_type().into(),
                ),
                BOOLEAN_ID | NIL_ID => {
                    let typ = context.opaque_struct_type(&name);

                    typ.set_body(&[header.into()], false);
                    typ
                }
                BYTE_ARRAY_ID => context.builtin_type(
                    &name,
                    header,
                    context.rust_vec_type().into(),
                ),
                CHANNEL_ID => context.builtin_type(
                    &name,
                    header,
                    context.pointer_type().into(),
                ),
                _ => {
                    // First we forward-declare the structures, as fields
                    // may need to refer to other classes regardless of
                    // ordering.
                    context.opaque_struct_type(&name)
                }
            };

            let mut buckets = vec![false; methods_len];
            let max_bucket = methods_len.saturating_sub(1);

            // Define the method signatures once (so we can cheaply retrieve
            // them whenever needed), and assign the methods to their method
            // table slots.
            for &method in &mir_class.methods {
                let name = method.name(db);
                let hash = method_hasher.hash(name);
                let mut collision = false;
                let index = if mir_class.id.kind(db).is_closure() {
                    // For closures we use a fixed layout so we can call its
                    // methods using virtual dispatch instead of dynamic
                    // dispatch.
                    match method.name(db).as_str() {
                        CALL_METHOD => CLOSURE_CALL_INDEX as _,
                        DROPPER_METHOD => CLOSURE_DROPPER_INDEX as _,
                        _ => unreachable!(),
                    }
                } else {
                    let mut index = hash as usize & (methods_len - 1);

                    while buckets[index] {
                        collision = true;
                        index = (index + 1) & max_bucket;
                    }

                    index
                };

                buckets[index] = true;

                // We track collisions so we can generate more optimal dynamic
                // dispatch code if we statically know one method never collides
                // with another method in the same class.
                if collision {
                    if let MethodSource::Implementation(_, orig) =
                        method.source(db)
                    {
                        // We have to track the original method as defined in
                        // the trait, not the implementation defined for the
                        // class. This is because when we generate the dynamic
                        // dispatch code, we only know about the trait method.
                        methods.get_mut(&orig).unwrap().collision = true;
                    }
                }

                let typ = if method.is_async(db) {
                    context.void_type().fn_type(
                        &[context_layout.ptr_type(space).into()],
                        false,
                    )
                } else {
                    let mut args: Vec<BasicMetadataTypeEnum> = vec![
                        state_layout.ptr_type(space).into(), // State
                        context.pointer_type().into(),       // Process
                    ];

                    if method.is_instance_method(db) {
                        args.push(context.pointer_type().into());
                    }

                    for _ in 0..method.number_of_arguments(db) {
                        args.push(context.pointer_type().into());
                    }

                    if method.throw_type(db).is_never(db) {
                        context.pointer_type().fn_type(&args, false)
                    } else {
                        result_layout.fn_type(&args, false)
                    }
                };

                methods.insert(
                    method,
                    MethodInfo {
                        index: index as u16,
                        hash,
                        signature: typ,
                        collision,
                    },
                );
            }

            class_layouts.insert(*id, class);
            instance_layouts.insert(*id, instance);
        }

        for id in mir.classes.keys() {
            if id.is_builtin() {
                continue;
            }

            let layout = instance_layouts[id];
            let mut fields: Vec<BasicTypeEnum> = vec![header.into()];

            // For processes we need to take into account the space between the
            // header and the first field. We don't actually care about that
            // state in the generated code, so we just insert a single member
            // that covers it.
            if id.kind(db).is_async() {
                fields.push(
                    context
                        .i8_type()
                        .array_type(PROCESS_SIZE - HEADER_SIZE)
                        .into(),
                );
            }

            for _ in 0..id.number_of_fields(db) {
                fields.push(context.pointer_type().into());
            }

            layout.set_body(&fields, false);
        }

        Self {
            empty_class: context.class_type(0, ""),
            classes: class_layouts,
            instances: instance_layouts,
            state: state_layout,
            header,
            result: result_layout,
            context: context_layout,
            method_counts: method_counts_layout,
            methods,
            message: message_layout,
        }
    }

    fn methods(&self, class: ClassId) -> u32 {
        self.classes[&class]
            .get_field_type_at_index(3)
            .unwrap()
            .into_array_type()
            .len()
    }
}

/// A pass that lowers the MIR of a module into LLVM IR.
pub(crate) struct Lower<'a, 'b, 'ctx> {
    db: &'a Database,
    mir: &'a Mir,
    module_index: usize,
    types: &'a Types<'ctx>,
    names: &'a SymbolNames,
    context: &'ctx Context,
    module: &'b mut Module<'a, 'ctx>,

    /// All native functions and the class IDs they belong to.
    functions: HashMap<ClassId, Vec<FunctionValue<'ctx>>>,
}

impl<'a, 'b, 'ctx> Lower<'a, 'b, 'ctx> {
    pub(crate) fn run_all(db: &'a Database, mir: &'a Mir) {
        let context = Context::new();
        let types = Types::new(&context, db, mir);
        let names = SymbolNames::new(db, mir);
        let mut modules = Vec::with_capacity(mir.modules.len());

        for module_index in 0..mir.modules.len() {
            let mod_id = mir.modules[module_index].id;
            let name = mod_id.name(db).to_string();
            let mut module = Module::new(&context, &types, name);

            Lower {
                db,
                mir,
                module_index,
                names: &names,
                context: &context,
                module: &mut module,
                types: &types,
                functions: HashMap::new(),
            }
            .run();

            modules.push(module);
        }

        // TODO: move elsewhere
        let main_module = Module::new(&context, &types, "$main".to_string());

        GenerateMain {
            db,
            mir,
            names: &names,
            context: &context,
            module: &main_module,
            types: &types,
        }
        .run();

        modules.push(main_module);

        Target::initialize_x86(&InitializationConfig::default());

        let opt = OptimizationLevel::Default;
        let reloc = RelocMode::PIC;
        let model = CodeModel::Default;
        let target = Target::from_name("x86-64").unwrap();
        let triple = TargetTriple::create("x86_64-pc-linux-gnu");
        let target_machine = target
            .create_target_machine(&triple, "x86-64", "", opt, reloc, model)
            .unwrap();
        let layout = target_machine.get_target_data().get_data_layout();

        // TODO: remove
        let cache_dir = Path::new("/tmp/inko");

        if cache_dir.exists() {
            std::fs::remove_dir_all(cache_dir).unwrap();
        }

        std::fs::create_dir(cache_dir).unwrap();

        let pm_builder = PassManagerBuilder::create();
        let pm = PassManager::create(());

        pm_builder.set_optimization_level(opt);
        pm_builder.populate_module_pass_manager(&pm);
        pm.add_promote_memory_to_register_pass();

        for module in modules {
            module.set_data_layout(&layout);
            module.set_triple(&triple);

            let name = module.get_name().to_string_lossy();

            pm.run_on(&module.inner);

            module
                .print_to_file(&format!("/tmp/inko/{}.ll", name))
                .expect("Failed to print the LLVM IR");

            target_machine
                .write_to_file(
                    &module,
                    FileType::Object,
                    Path::new(&format!("/tmp/inko/{}.o", name)),
                )
                .expect("Failed to write the object file");
        }
    }

    pub(crate) fn run(mut self) {
        for &class_id in &self.mir.modules[self.module_index].classes {
            for method_id in &self.mir.classes[&class_id].methods {
                let func = LowerMethod::new(
                    self.db,
                    self.mir,
                    self.types,
                    self.context,
                    &mut self.module,
                    self.names,
                    self.module_index,
                    class_id,
                    &self.mir.methods[method_id],
                )
                .run();

                self.functions
                    .entry(class_id)
                    .or_insert_with(Vec::new)
                    .push(func);
            }
        }

        self.generate_setup_function();

        if let Err(err) = self.module.verify() {
            println!(
                "WARNING: the LLVM module {} is invalid:\n\n{}\n",
                self.mir.modules[self.module_index].id.name(self.db),
                err.to_string()
            );
        }
    }

    fn generate_setup_function(&mut self) {
        let mod_id = self.mir.modules[self.module_index].id;
        let space = AddressSpace::Generic;
        let fn_name = &self.names.setup_functions[&mod_id];
        let fn_val = self.module.add_setup_function(fn_name);
        let builder = Builder::new(self.context, self.types);
        let entry_block = self.context.append_basic_block(fn_val, "");

        builder.position_at_end(entry_block);

        let state_var =
            builder.build_alloca(self.types.state.ptr_type(space), "");

        builder.store(state_var, fn_val.get_nth_param(0).unwrap());

        let body = self.context.append_basic_block(fn_val, "");

        builder.build_unconditional_branch(body);
        builder.position_at_end(body);

        // Allocate all classes defined in this module, and store them in their
        // corresponding globals.
        for &class_id in &self.mir.modules[self.module_index].classes {
            let raw_name = class_id.name(self.db);
            let name_ptr = builder.string_literal(raw_name).0.into();
            let fields_len = self
                .context
                .i8_type()
                .const_int(class_id.number_of_fields(self.db) as _, false)
                .into();
            let methods_len = self
                .context
                .i16_type()
                .const_int((self.types.methods(class_id) as usize) as _, false)
                .into();

            let class_new = if class_id.kind(self.db).is_async() {
                self.module.runtime_function(RuntimeFunction::ClassProcess)
            } else {
                self.module.runtime_function(RuntimeFunction::ClassObject)
            };

            let layout = self.types.classes[&class_id];
            let global_name = &self.names.classes[&class_id];
            let global = self.module.add_class(class_id, global_name);

            // The class globals must have an initializer, otherwise LLVM treats
            // them as external globals.
            global.set_initializer(
                &layout.ptr_type(space).const_null().as_basic_value_enum(),
            );

            let ptr = builder
                .call(class_new, &[name_ptr, fields_len, methods_len])
                .into_pointer_value();
            let ptr_casted = builder.cast_to_typed_pointer(ptr, layout);
            let method_new =
                self.module.runtime_function(RuntimeFunction::MethodNew);

            for method in &self.mir.classes[&class_id].methods {
                let info = &self.types.methods[method];
                let name = &self.names.methods[method];

                // TODO: remove once all modules are processed, as then this is
                // actually an indicator of a compiler bug.
                if self.module.get_function(name).is_none() {
                    continue;
                }

                let func = builder.cast_to_pointer(
                    self.module
                        .get_function(name)
                        .unwrap()
                        .as_global_value()
                        .as_pointer_value(),
                );

                let write_to = unsafe {
                    builder.build_gep(
                        ptr_casted,
                        &[
                            builder.u32_literal(0),
                            builder.u32_literal(CLASS_METHODS_INDEX as _),
                            builder.u16_literal(info.index),
                        ],
                        "",
                    )
                };

                let hash = builder.u64_literal(info.hash);
                let method = builder
                    .call(method_new, &[hash.into(), func.into()])
                    .into_struct_value();

                builder.store(write_to, method);
            }

            builder.store(global.as_pointer_value(), ptr_casted);
        }

        // Populate the globals for the constants defined in this module.
        for &cid in &self.mir.modules[self.module_index].constants {
            let name = &self.names.constants[&cid];
            let global = self.module.add_constant(name);
            let value = &self.mir.constants[&cid];

            global.set_initializer(
                &self.context.pointer_type().const_null().as_basic_value_enum(),
            );
            self.set_constant_global(&builder, state_var, value, global);
        }

        // Populate the globals for the literals defined in this module.
        for (value, global) in &self.module.literals {
            self.set_constant_global(&builder, state_var, value, *global);
        }

        builder.build_return(None);
    }

    fn set_constant_global(
        &self,
        builder: &Builder<'a, 'ctx>,
        state_var: PointerValue<'ctx>,
        constant: &Constant,
        global: GlobalValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let global = global.as_pointer_value();
        let value = self.permanent_value(builder, state_var, constant);

        builder.store(global, value);
        global
    }

    fn permanent_value(
        &self,
        builder: &Builder<'a, 'ctx>,
        state_var: PointerValue<'ctx>,
        constant: &Constant,
    ) -> BasicValueEnum<'ctx> {
        let state = builder.load(state_var).into();

        match constant {
            Constant::Int(val) => {
                if let Some(ptr) = builder.tagged_int(*val) {
                    ptr.into()
                } else {
                    let val = builder.i64_literal(*val).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::IntBoxedPermanent);

                    builder.call(func, &[state, val])
                }
            }
            Constant::Float(val) => {
                let val = builder.context.f64_type().const_float(*val).into();
                let func = self
                    .module
                    .runtime_function(RuntimeFunction::FloatBoxedPermanent);

                builder.call(func, &[state, val])
            }
            Constant::String(val) => {
                let global_ptr =
                    builder.build_global_string_ptr(val, "").as_pointer_value();
                let ptr = builder.cast_to_pointer(global_ptr).into();
                let len = builder.u64_literal(val.len() as u64).into();
                let func = self
                    .module
                    .runtime_function(RuntimeFunction::StringNewPermanent);

                builder.call(func, &[state, ptr, len])
            }
            Constant::Array(values) => {
                let len = builder.u64_literal(values.len() as u64).into();
                let new_func = self
                    .module
                    .runtime_function(RuntimeFunction::ArrayNewPermanent);
                let push_func =
                    self.module.runtime_function(RuntimeFunction::ArrayPush);
                let array = builder.call(new_func, &[state, len]);

                for val in values {
                    let ptr = builder.cast_to_pointer(
                        self.permanent_value(builder, state_var, val)
                            .into_pointer_value(),
                    );

                    builder.call(push_func, &[state, array.into(), ptr.into()]);
                }

                array
            }
        }
    }
}

/// A pass for lowering the MIR of a single method.
pub struct LowerMethod<'a, 'b, 'ctx> {
    db: &'a Database,
    mir: &'a Mir,
    types: &'a Types<'ctx>,

    /// The index of the MIR module our method is defined in.
    /// TODO: do we need this?
    module_index: usize,

    /// The class the method belongs to that we're lowering.
    class_id: ClassId,

    /// The MIR method that we're lowering to LLVM.
    method: &'b Method,

    /// A map of method names to their mangled names.
    ///
    /// We cache these so we don't have to recalculate them on every reference.
    names: &'a SymbolNames,

    /// The LLVM context to use for generating instructions.
    context: &'ctx Context,

    /// The LLVM module the generated code belongs to.
    module: &'b mut Module<'a, 'ctx>,

    /// MIR registers and their corresponding LLVM stack variables.
    variables: HashMap<RegisterId, PointerValue<'ctx>>,

    /// The LLVM function value of the method we're compiling.
    function: FunctionValue<'ctx>,

    /// A flag indicating that this method may throw a value.
    throws: bool,
}

impl<'a, 'b, 'ctx> LowerMethod<'a, 'b, 'ctx> {
    fn new(
        db: &'a Database,
        mir: &'a Mir,
        types: &'a Types<'ctx>,
        context: &'ctx Context,
        module: &'b mut Module<'a, 'ctx>,
        names: &'a SymbolNames,
        module_index: usize,
        class_id: ClassId,
        method: &'b Method,
    ) -> Self {
        let throws = !method.id.throw_type(db).is_never(db);
        let function = module.add_method(&names.methods[&method.id], method.id);

        LowerMethod {
            db,
            mir,
            types,
            module_index,
            class_id,
            method,
            names,
            context,
            module,
            variables: HashMap::new(),
            function,
            throws,
        }
    }

    fn run(&mut self) -> FunctionValue<'ctx> {
        if self.method.id.is_async(self.db) {
            self.async_method();
        } else {
            self.regular_method();
        }

        self.function
    }

    fn regular_method(&mut self) {
        let builder = Builder::new(self.context, self.types);
        let entry_block = self.add_basic_block();

        builder.position_at_end(entry_block);

        let space = AddressSpace::Generic;
        let state_var = self.new_stack_slot(self.types.state.ptr_type(space));
        let proc_var = self.new_stack_slot(self.context.pointer_type());

        // Build the stores for all the arguments, including the generated ones.
        builder.store(state_var, self.function.get_nth_param(0).unwrap());
        builder.store(proc_var, self.function.get_nth_param(1).unwrap());

        self.define_register_variables(&builder);

        for (arg, reg) in self
            .function
            .get_param_iter()
            .skip(2)
            .zip(self.method.arguments.iter())
        {
            builder.store(self.variables[reg], arg);
        }

        self.method_body(builder, state_var, proc_var);
    }

    fn async_method(&mut self) {
        let builder = Builder::new(self.context, self.types);
        let entry_block = self.add_basic_block();

        builder.position_at_end(entry_block);

        let space = AddressSpace::Generic;
        let state_var = self.new_stack_slot(self.types.state.ptr_type(space));
        let proc_var = self.new_stack_slot(self.context.pointer_type());
        let num_args = self.method.arguments.len() as u32;
        let args_type =
            self.context.pointer_type().array_type(num_args).ptr_type(space);
        let args_var = self.new_stack_slot(args_type);
        let ctx_var = self.new_stack_slot(self.types.context.ptr_type(space));

        self.define_register_variables(&builder);

        // Destructure the context into its components. This is necessary as the
        // context only lives until the first yield.
        builder.store(ctx_var, self.function.get_nth_param(0).unwrap());

        let ctx = builder.load_pointer(ctx_var);

        builder.store(state_var, builder.load_field(ctx, CONTEXT_STATE_INDEX));
        builder.store(proc_var, builder.load_field(ctx, CONTEXT_PROCESS_INDEX));

        let args = builder
            .build_bitcast(
                builder
                    .load_field(ctx, CONTEXT_ARGS_INDEX)
                    .into_pointer_value(),
                args_type,
                "",
            )
            .into_pointer_value();

        builder.store(args_var, args);

        // For async methods we don't include the receiver in the message, as
        // this is redundant, and keeps message sizes as compact as possible.
        // Instead, we load the receiver from the context.
        let self_var = self.variables[&self.method.arguments[0]];

        builder.store(self_var, builder.load(proc_var));

        // Populate the argument stack variables according to the values stored
        // in the context structure.
        for (index, reg) in self.method.arguments.iter().skip(1).enumerate() {
            let var = self.variables[reg];
            let args = builder.load_pointer(args_var);
            let val = builder
                .load_array_index(args, index)
                .unwrap()
                .into_pointer_value();

            builder.store(var, val);
        }

        self.method_body(builder, state_var, proc_var);
    }

    fn method_body(
        &mut self,
        builder: Builder<'a, 'ctx>,
        state_var: PointerValue<'ctx>,
        proc_var: PointerValue<'ctx>,
    ) {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        let mut llvm_blocks = Vec::with_capacity(self.method.body.blocks.len());

        for _ in 0..self.method.body.blocks.len() {
            llvm_blocks.push(self.add_basic_block());
        }

        builder.build_unconditional_branch(
            llvm_blocks[self.method.body.start_id.0],
        );

        queue.push_back(self.method.body.start_id);
        visited.insert(self.method.body.start_id);

        while let Some(block_id) = queue.pop_front() {
            let mir_block = &self.method.body.blocks[block_id.0];
            let llvm_block = llvm_blocks[block_id.0];

            builder.position_at_end(llvm_block);

            for ins in &mir_block.instructions {
                // TODO: move some of this into `self`?
                self.instruction(
                    ins,
                    &llvm_blocks,
                    &builder,
                    state_var,
                    proc_var,
                );
            }

            for &child in &mir_block.successors {
                if visited.insert(child) {
                    queue.push_back(child);
                }
            }
        }
    }

    fn instruction(
        &mut self,
        ins: &Instruction,
        llvm_blocks: &[BasicBlock],
        builder: &Builder<'a, 'ctx>,
        state_var: PointerValue<'ctx>,
        proc_var: PointerValue<'ctx>,
    ) {
        match ins {
            Instruction::CallBuiltin(ins) => match ins.name {
                BuiltinFunction::IntAdd => {
                    self.checked_int_operation(
                        "llvm.sadd.with.overflow",
                        builder,
                        state_var,
                        proc_var,
                        self.variables[&ins.register],
                        self.variables[&ins.arguments[0]],
                        self.variables[&ins.arguments[1]],
                    );
                }
                BuiltinFunction::IntSub => {
                    self.checked_int_operation(
                        "llvm.ssub.with.overflow",
                        builder,
                        state_var,
                        proc_var,
                        self.variables[&ins.register],
                        self.variables[&ins.arguments[0]],
                        self.variables[&ins.arguments[1]],
                    );
                }
                BuiltinFunction::IntMul => {
                    self.checked_int_operation(
                        "llvm.smul.with.overflow",
                        builder,
                        state_var,
                        proc_var,
                        self.variables[&ins.register],
                        self.variables[&ins.arguments[0]],
                        self.variables[&ins.arguments[1]],
                    );
                }
                BuiltinFunction::IntDiv => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);

                    self.check_division_overflow(builder, proc_var, lhs, rhs);

                    let raw = builder.build_int_signed_div(lhs, rhs, "");
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntRem => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);

                    self.check_division_overflow(builder, proc_var, lhs, rhs);

                    let raw = builder.build_int_signed_rem(lhs, rhs, "");
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntBitAnd => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.bit_and(lhs, rhs);
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntBitOr => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.bit_or(lhs, rhs);
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntBitNot => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = self.read_int(builder, val_var);
                    let raw = builder.build_not(val, "");
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntBitXor => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.build_xor(lhs, rhs, "");
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntEq => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.int_eq(lhs, rhs);
                    let res = self.new_bool(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntGt => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.int_gt(lhs, rhs);
                    let res = self.new_bool(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntGe => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.int_ge(lhs, rhs);
                    let res = self.new_bool(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntLe => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.int_le(lhs, rhs);
                    let res = self.new_bool(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntLt => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.int_lt(lhs, rhs);
                    let res = self.new_bool(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntPow => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let proc = builder.load_pointer(proc_var).into();
                    let lhs = self.read_int(builder, lhs_var).into();
                    let rhs = self.read_int(builder, rhs_var).into();
                    let func =
                        self.module.runtime_function(RuntimeFunction::IntPow);
                    let raw =
                        builder.call(func, &[proc, lhs, rhs]).into_int_value();
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatAdd => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_float(builder, lhs_var);
                    let rhs = self.read_float(builder, rhs_var);
                    let raw = builder.build_float_add(lhs, rhs, "");
                    let res = self.new_float(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatSub => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_float(builder, lhs_var);
                    let rhs = self.read_float(builder, rhs_var);
                    let raw = builder.build_float_sub(lhs, rhs, "");
                    let res = self.new_float(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatDiv => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_float(builder, lhs_var);
                    let rhs = self.read_float(builder, rhs_var);
                    let raw = builder.build_float_div(lhs, rhs, "");
                    let res = self.new_float(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatMul => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_float(builder, lhs_var);
                    let rhs = self.read_float(builder, rhs_var);
                    let raw = builder.build_float_mul(lhs, rhs, "");
                    let res = self.new_float(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatMod => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_float(builder, lhs_var);
                    let rhs = self.read_float(builder, rhs_var);
                    let raw = builder.build_float_rem(
                        builder.build_float_add(
                            builder.build_float_rem(lhs, rhs, ""),
                            rhs,
                            "",
                        ),
                        rhs,
                        "",
                    );
                    let res = self.new_float(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatCeil => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = self.read_float(builder, val_var);
                    let func = Intrinsic::find("llvm.ceil")
                        .and_then(|intr| {
                            intr.get_declaration(
                                &self.module.inner,
                                &[self.context.f64_type().into()],
                            )
                        })
                        .unwrap();
                    let raw =
                        builder.call(func, &[val.into()]).into_float_value();
                    let res = self.new_float(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatFloor => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = self.read_float(builder, val_var);
                    let func = Intrinsic::find("llvm.floor")
                        .and_then(|intr| {
                            intr.get_declaration(
                                &self.module.inner,
                                &[self.context.f64_type().into()],
                            )
                        })
                        .unwrap();
                    let raw =
                        builder.call(func, &[val.into()]).into_float_value();
                    let res = self.new_float(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatRound => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let lhs = self.read_float(builder, lhs_var).into();
                    let rhs = self.read_int(builder, rhs_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::FloatRound);
                    let res = builder.call(func, &[state, lhs, rhs]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatEq => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let lhs = self.read_float(builder, lhs_var).into();
                    let rhs = self.read_float(builder, rhs_var).into();
                    let func =
                        self.module.runtime_function(RuntimeFunction::FloatEq);
                    let res = builder.call(func, &[state, lhs, rhs]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatToBits => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = self.read_float(builder, val_var);
                    let bits = builder
                        .build_bitcast(val, self.context.i64_type(), "")
                        .into_int_value();
                    let res = self.new_int(builder, state_var, bits);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatFromBits => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = self.read_int(builder, val_var);
                    let bits = builder
                        .build_bitcast(val, self.context.f64_type(), "")
                        .into_float_value();
                    let res = self.new_float(builder, state_var, bits);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatGt => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_float(builder, lhs_var);
                    let rhs = self.read_float(builder, rhs_var);
                    let raw = builder.build_float_compare(
                        FloatPredicate::OGT,
                        lhs,
                        rhs,
                        "",
                    );

                    builder
                        .store(reg_var, self.new_bool(builder, state_var, raw));
                }
                BuiltinFunction::FloatGe => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_float(builder, lhs_var);
                    let rhs = self.read_float(builder, rhs_var);
                    let raw = builder.build_float_compare(
                        FloatPredicate::OGE,
                        lhs,
                        rhs,
                        "",
                    );

                    builder
                        .store(reg_var, self.new_bool(builder, state_var, raw));
                }
                BuiltinFunction::FloatLt => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_float(builder, lhs_var);
                    let rhs = self.read_float(builder, rhs_var);
                    let raw = builder.build_float_compare(
                        FloatPredicate::OLT,
                        lhs,
                        rhs,
                        "",
                    );

                    builder
                        .store(reg_var, self.new_bool(builder, state_var, raw));
                }
                BuiltinFunction::FloatLe => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_float(builder, lhs_var);
                    let rhs = self.read_float(builder, rhs_var);
                    let raw = builder.build_float_compare(
                        FloatPredicate::OLE,
                        lhs,
                        rhs,
                        "",
                    );

                    builder
                        .store(reg_var, self.new_bool(builder, state_var, raw));
                }
                BuiltinFunction::FloatIsInf => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = self.read_float(builder, val_var);
                    let pos_inf = builder.f64_literal(f64::INFINITY);
                    let neg_inf = builder.f64_literal(f64::NEG_INFINITY);
                    let cond1 = builder.float_eq(val, pos_inf);
                    let cond2 = builder.float_eq(val, neg_inf);
                    let raw = builder.bit_and(cond1, cond2);

                    builder
                        .store(reg_var, self.new_bool(builder, state_var, raw));
                }
                BuiltinFunction::FloatIsNan => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = self.read_float(builder, val_var);
                    let raw = builder.build_float_compare(
                        FloatPredicate::ONE,
                        val,
                        val,
                        "",
                    );

                    builder
                        .store(reg_var, self.new_bool(builder, state_var, raw));
                }
                BuiltinFunction::FloatToInt => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = self.read_float(builder, val_var);
                    let raw = builder.build_float_to_signed_int(
                        val,
                        self.context.i64_type(),
                        "",
                    );
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FloatToString => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let val = self.read_float(builder, val_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::FloatToString);
                    let res = builder.call(func, &[state, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArrayCapacity => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::ArrayCapacity;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArrayClear => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::ArrayClear;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArrayDrop => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::ArrayDrop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArrayGet => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let index_var = self.variables[&ins.arguments[1]];
                    let array = builder.load_pointer(array_var).into();
                    let index = self.read_int(builder, index_var).into();
                    let func_name = RuntimeFunction::ArrayGet;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[array, index]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArrayLength => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let func_name = RuntimeFunction::ArrayLength;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, array]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArrayPop => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let array = builder.load_pointer(array_var).into();
                    let func_name = RuntimeFunction::ArrayPop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[array]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArrayPush => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let value_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let value = builder.load_pointer(value_var).into();
                    let func_name = RuntimeFunction::ArrayPush;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, array, value]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArrayRemove => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let idx_var = self.variables[&ins.arguments[1]];
                    let array = builder.load_pointer(array_var).into();
                    let idx = self.read_int(builder, idx_var).into();
                    let func_name = RuntimeFunction::ArrayRemove;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[array, idx]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArrayReserve => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let amount_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let amount = self.read_int(builder, amount_var).into();
                    let func_name = RuntimeFunction::ArrayReserve;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, array, amount]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ArraySet => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let index_var = self.variables[&ins.arguments[1]];
                    let value_var = self.variables[&ins.arguments[2]];
                    let array = builder.load_pointer(array_var).into();
                    let index = self.read_int(builder, index_var).into();
                    let value = builder.load_pointer(value_var).into();
                    let func =
                        self.module.runtime_function(RuntimeFunction::ArraySet);
                    let res = builder.call(func, &[array, index, value]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayNew => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayNew);
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayAppend => {
                    let reg_var = self.variables[&ins.register];
                    let target_var = self.variables[&ins.arguments[0]];
                    let source_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let target = builder.load_pointer(target_var).into();
                    let source = builder.load_pointer(source_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayAppend);
                    let res = builder.call(func, &[state, target, source]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayClear => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayClear);
                    let res = builder.call(func, &[state, array]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayClone => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayClone);
                    let res = builder.call(func, &[state, array]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayCopyFrom => {
                    let reg_var = self.variables[&ins.register];
                    let target_var = self.variables[&ins.arguments[0]];
                    let source_var = self.variables[&ins.arguments[1]];
                    let start_var = self.variables[&ins.arguments[2]];
                    let length_var = self.variables[&ins.arguments[3]];
                    let state = builder.load_pointer(state_var).into();
                    let target = builder.load_pointer(target_var).into();
                    let source = builder.load_pointer(source_var).into();
                    let start = self.read_int(builder, start_var).into();
                    let length = self.read_int(builder, length_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayCopyFrom);
                    let res = builder
                        .call(func, &[state, target, source, start, length]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayDrainToString => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let func = self.module.runtime_function(
                        RuntimeFunction::ByteArrayDrainToString,
                    );
                    let res = builder.call(func, &[state, array]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayToString => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayToString);
                    let res = builder.call(func, &[state, array]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayDrop => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayDrop);
                    let res = builder.call(func, &[state, array]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayEq => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let lhs = builder.load_pointer(lhs_var).into();
                    let rhs = builder.load_pointer(rhs_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayEq);
                    let res = builder.call(func, &[state, lhs, rhs]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayGet => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let index_var = self.variables[&ins.arguments[1]];
                    let array = builder.load_pointer(array_var).into();
                    let index = self.read_int(builder, index_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayGet);
                    let res = builder.call(func, &[array, index]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayLength => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayLength);
                    let res = builder.call(func, &[state, array]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayPop => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let array = builder.load_pointer(array_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayPop);
                    let res = builder.call(func, &[array]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayPush => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let value_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let value = builder.load_pointer(value_var).into();
                    let func_name = RuntimeFunction::ByteArrayPush;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, array, value]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayRemove => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let index_var = self.variables[&ins.arguments[1]];
                    let array = builder.load_pointer(array_var).into();
                    let index = self.read_int(builder, index_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayRemove);
                    let res = builder.call(func, &[array, index]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArrayResize => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let size_var = self.variables[&ins.arguments[1]];
                    let fill_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let size = self.read_int(builder, size_var).into();
                    let fill = self.read_int(builder, fill_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArrayResize);
                    let res = builder.call(func, &[state, array, size, fill]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArraySet => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let index_var = self.variables[&ins.arguments[1]];
                    let value_var = self.variables[&ins.arguments[2]];
                    let array = builder.load_pointer(array_var).into();
                    let index = self.read_int(builder, index_var).into();
                    let value = builder.load_pointer(value_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArraySet);
                    let res = builder.call(func, &[array, index, value]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ByteArraySlice => {
                    let reg_var = self.variables[&ins.register];
                    let array_var = self.variables[&ins.arguments[0]];
                    let start_var = self.variables[&ins.arguments[1]];
                    let length_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let array = builder.load_pointer(array_var).into();
                    let start = self.read_int(builder, start_var).into();
                    let length = self.read_int(builder, length_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ByteArraySlice);
                    let res =
                        builder.call(func, &[state, array, start, length]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessSpawn => {
                    let reg_var = self.variables[&ins.register];
                    let program_var = self.variables[&ins.arguments[0]];
                    let args_var = self.variables[&ins.arguments[1]];
                    let env_var = self.variables[&ins.arguments[2]];
                    let stdin_var = self.variables[&ins.arguments[3]];
                    let stdout_var = self.variables[&ins.arguments[4]];
                    let stderr_var = self.variables[&ins.arguments[5]];
                    let dir_var = self.variables[&ins.arguments[6]];
                    let proc = builder.load_pointer(proc_var).into();
                    let program = builder.load_pointer(program_var).into();
                    let args = builder.load_pointer(args_var).into();
                    let env = builder.load_pointer(env_var).into();
                    let stdin = self.read_int(builder, stdin_var).into();
                    let stdout = self.read_int(builder, stdout_var).into();
                    let stderr = self.read_int(builder, stderr_var).into();
                    let dir = builder.load_pointer(dir_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ChildProcessSpawn);
                    let res = builder.call(
                        func,
                        &[proc, program, args, env, stdin, stdout, stderr, dir],
                    );

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessDrop => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ChildProcessDrop);
                    let res = builder.call(func, &[state, child]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessStderrClose => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let func = self.module.runtime_function(
                        RuntimeFunction::ChildProcessStderrClose,
                    );
                    let res = builder.call(func, &[state, child]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessStderrRead => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let buffer_var = self.variables[&ins.arguments[1]];
                    let size_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let buffer = builder.load_pointer(buffer_var).into();
                    let size = self.read_int(builder, size_var).into();
                    let func = self.module.runtime_function(
                        RuntimeFunction::ChildProcessStderrRead,
                    );
                    let res =
                        builder.call(func, &[state, proc, child, buffer, size]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessStdinClose => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let func = self.module.runtime_function(
                        RuntimeFunction::ChildProcessStdinClose,
                    );
                    let res = builder.call(func, &[state, child]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessStdinFlush => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let func = self.module.runtime_function(
                        RuntimeFunction::ChildProcessStdinFlush,
                    );
                    let res = builder.call(func, &[state, proc, child]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessStdinWriteBytes => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let input_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let input = builder.load_pointer(input_var).into();
                    let func = self.module.runtime_function(
                        RuntimeFunction::ChildProcessStdinWriteBytes,
                    );
                    let res = builder.call(func, &[state, proc, child, input]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessStdinWriteString => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let input_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let input = builder.load_pointer(input_var).into();
                    let func = self.module.runtime_function(
                        RuntimeFunction::ChildProcessStdinWriteString,
                    );
                    let res = builder.call(func, &[state, proc, child, input]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessStdoutClose => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let func = self.module.runtime_function(
                        RuntimeFunction::ChildProcessStdoutClose,
                    );
                    let res = builder.call(func, &[state, child]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessStdoutRead => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let buffer_var = self.variables[&ins.arguments[1]];
                    let size_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let buffer = builder.load_pointer(buffer_var).into();
                    let size = self.read_int(builder, size_var).into();
                    let func = self.module.runtime_function(
                        RuntimeFunction::ChildProcessStdoutRead,
                    );
                    let res =
                        builder.call(func, &[state, proc, child, buffer, size]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessTryWait => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let child = builder.load_pointer(child_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ChildProcessTryWait);
                    let res = builder.call(func, &[child]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChildProcessWait => {
                    let reg_var = self.variables[&ins.register];
                    let child_var = self.variables[&ins.arguments[0]];
                    let proc = builder.load_pointer(proc_var).into();
                    let child = builder.load_pointer(child_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::ChildProcessWait);
                    let res = builder.call(func, &[proc, child]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::CpuCores => {
                    let reg_var = self.variables[&ins.register];
                    let func =
                        self.module.runtime_function(RuntimeFunction::CpuCores);
                    let res = builder.call(func, &[]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::DirectoryCreate => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::DirectoryCreate;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::DirectoryCreateRecursive => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::DirectoryCreateRecursive;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::DirectoryList => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::DirectoryList;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::DirectoryRemove => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::DirectoryRemove;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::DirectoryRemoveRecursive => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::DirectoryRemoveAll;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::EnvArguments => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let func_name = RuntimeFunction::EnvArguments;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::EnvExecutable => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let func_name = RuntimeFunction::EnvExecutable;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::EnvGet => {
                    let reg_var = self.variables[&ins.register];
                    let name_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let name = builder.load_pointer(name_var).into();
                    let func_name = RuntimeFunction::EnvGet;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, name]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::EnvGetWorkingDirectory => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let func_name = RuntimeFunction::EnvGetWorkingDirectory;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::EnvHomeDirectory => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let func_name = RuntimeFunction::EnvHomeDirectory;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::EnvPlatform => {
                    let reg_var = self.variables[&ins.register];
                    let func_name = RuntimeFunction::EnvPlatform;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::EnvSetWorkingDirectory => {
                    let reg_var = self.variables[&ins.register];
                    let dir_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let dir = builder.load_pointer(dir_var).into();
                    let func_name = RuntimeFunction::EnvSetWorkingDirectory;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, dir]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::EnvTempDirectory => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let func_name = RuntimeFunction::EnvTempDirectory;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::EnvVariables => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let func_name = RuntimeFunction::EnvVariables;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::Exit => {
                    let status_var = self.variables[&ins.arguments[0]];
                    let status = self.read_int(builder, status_var).into();
                    let func_name = RuntimeFunction::Exit;
                    let func = self.module.runtime_function(func_name);

                    builder.call_void(func, &[status]);
                    builder.build_unreachable();
                }
                BuiltinFunction::FileCopy => {
                    let reg_var = self.variables[&ins.register];
                    let from_var = self.variables[&ins.arguments[0]];
                    let to_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let from = builder.load_pointer(from_var).into();
                    let to = builder.load_pointer(to_var).into();
                    let func_name = RuntimeFunction::FileCopy;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, from, to]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FileDrop => {
                    let reg_var = self.variables[&ins.register];
                    let file_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let file = builder.load_pointer(file_var).into();
                    let func_name = RuntimeFunction::FileDrop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, file]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FileFlush => {
                    let reg_var = self.variables[&ins.register];
                    let file_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let file = builder.load_pointer(file_var).into();
                    let func_name = RuntimeFunction::FileFlush;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, file]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FileOpen => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let mode_var = self.variables[&ins.arguments[1]];
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let mode = self.read_int(builder, mode_var).into();
                    let func_name = RuntimeFunction::FileOpen;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[proc, path, mode]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FileRead => {
                    let reg_var = self.variables[&ins.register];
                    let file_var = self.variables[&ins.arguments[0]];
                    let buf_var = self.variables[&ins.arguments[1]];
                    let size_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let file = builder.load_pointer(file_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let size = self.read_int(builder, size_var).into();
                    let func_name = RuntimeFunction::FileRead;
                    let func = self.module.runtime_function(func_name);
                    let res =
                        builder.call(func, &[state, proc, file, buf, size]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FileRemove => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::FileRemove;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FileSeek => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let off_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let off = self.read_int(builder, off_var).into();
                    let func_name = RuntimeFunction::FileSeek;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path, off]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FileSize => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::FileSize;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FileWriteBytes => {
                    let reg_var = self.variables[&ins.register];
                    let file_var = self.variables[&ins.arguments[0]];
                    let buf_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let file = builder.load_pointer(file_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let func_name = RuntimeFunction::FileWriteBytes;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, file, buf]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::FileWriteString => {
                    let reg_var = self.variables[&ins.register];
                    let file_var = self.variables[&ins.arguments[0]];
                    let buf_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let file = builder.load_pointer(file_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let func_name = RuntimeFunction::FileWriteString;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, file, buf]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChannelReceive => {
                    let reg_var = self.variables[&ins.register];
                    let chan_var = self.variables[&ins.arguments[0]];
                    let proc = builder.load_pointer(proc_var).into();
                    let chan = builder.load_pointer(chan_var).into();
                    let func_name = RuntimeFunction::ChannelReceive;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[proc, chan]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChannelReceiveUntil => {
                    let reg_var = self.variables[&ins.register];
                    let chan_var = self.variables[&ins.arguments[0]];
                    let time_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let chan = builder.load_pointer(chan_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::ChannelReceiveUntil;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, chan, time]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChannelDrop => {
                    let reg_var = self.variables[&ins.register];
                    let chan_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let chan = builder.load_pointer(chan_var).into();
                    let func_name = RuntimeFunction::ChannelDrop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, chan]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChannelWait => {
                    let reg_var = self.variables[&ins.register];
                    let chans_var = self.variables[&ins.arguments[0]];
                    let proc = builder.load_pointer(proc_var).into();
                    let chans = builder.load_pointer(chans_var).into();
                    let func_name = RuntimeFunction::ChannelWait;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[proc, chans]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChannelNew => {
                    let reg_var = self.variables[&ins.register];
                    let cap_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let cap = self.read_int(builder, cap_var).into();
                    let func_name = RuntimeFunction::ChannelNew;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, cap]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChannelSend => {
                    let reg_var = self.variables[&ins.register];
                    let chan_var = self.variables[&ins.arguments[0]];
                    let msg_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let chan = builder.load_pointer(chan_var).into();
                    let msg = self.read_int(builder, msg_var).into();
                    let func_name = RuntimeFunction::ChannelSend;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, chan, msg]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ChannelTryReceive => {
                    let reg_var = self.variables[&ins.register];
                    let chan_var = self.variables[&ins.arguments[0]];
                    let proc = builder.load_pointer(proc_var).into();
                    let chan = builder.load_pointer(chan_var).into();
                    let func_name = RuntimeFunction::ChannelTryReceive;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[proc, chan]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::HasherDrop => {
                    let reg_var = self.variables[&ins.register];
                    let hash_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let hasher = builder.load_pointer(hash_var).into();
                    let func_name = RuntimeFunction::HasherDrop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, hasher]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::HasherNew => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let func_name = RuntimeFunction::HasherNew;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::HasherToHash => {
                    let reg_var = self.variables[&ins.register];
                    let hash_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let hasher = builder.load_pointer(hash_var).into();
                    let func_name = RuntimeFunction::HasherToHash;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, hasher]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::HasherWriteInt => {
                    let reg_var = self.variables[&ins.register];
                    let hash_var = self.variables[&ins.arguments[0]];
                    let int_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let hasher = builder.load_pointer(hash_var).into();
                    let value = self.read_int(builder, int_var).into();
                    let func_name = RuntimeFunction::HasherWriteInt;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, hasher, value]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntRotateLeft => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var).into();
                    let rhs = self.read_int(builder, rhs_var).into();
                    let func = Intrinsic::find("llvm.fshl")
                        .and_then(|intr| {
                            intr.get_declaration(
                                &self.module.inner,
                                &[self.context.i64_type().into()],
                            )
                        })
                        .unwrap();
                    let raw =
                        builder.call(func, &[lhs, lhs, rhs]).into_int_value();
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntRotateRight => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var).into();
                    let rhs = self.read_int(builder, rhs_var).into();
                    let func = Intrinsic::find("llvm.fshr")
                        .and_then(|intr| {
                            intr.get_declaration(
                                &self.module.inner,
                                &[self.context.i64_type().into()],
                            )
                        })
                        .unwrap();
                    let raw =
                        builder.call(func, &[lhs, lhs, rhs]).into_int_value();
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntShl => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);

                    self.check_shift_bits(proc_var, builder, lhs, rhs);

                    let raw = builder.left_shift(lhs, rhs);
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntShr => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);

                    self.check_shift_bits(proc_var, builder, lhs, rhs);

                    let raw = builder.signed_right_shift(lhs, rhs);
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntUnsignedShr => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);

                    self.check_shift_bits(proc_var, builder, lhs, rhs);

                    let raw = builder.right_shift(lhs, rhs);
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntToFloat => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = self.read_int(builder, val_var);
                    let raw = builder
                        .build_cast(
                            InstructionOpcode::SIToFP,
                            val,
                            self.context.f64_type(),
                            "",
                        )
                        .into_float_value();

                    let res = self.new_float(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntToString => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::IntToString);
                    let state = builder.load_pointer(state_var).into();
                    let val = self.read_int(builder, val_var).into();
                    let ret = builder.call(func, &[state, val]);

                    builder.store(reg_var, ret);
                }
                BuiltinFunction::IntWrappingAdd => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.int_add(lhs, rhs);
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntWrappingMul => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.int_mul(lhs, rhs);
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IntWrappingSub => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = self.read_int(builder, lhs_var);
                    let rhs = self.read_int(builder, rhs_var);
                    let raw = builder.int_sub(lhs, rhs);
                    let res = self.new_int(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::IsNull => {
                    let reg_var = self.variables[&ins.register];
                    let val_var = self.variables[&ins.arguments[0]];
                    let val = builder.load_pointer(val_var);
                    let raw = builder.build_is_null(val, "");
                    let res = self.new_bool(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ObjectEq => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let lhs = builder.load_pointer(lhs_var);
                    let rhs = builder.load_pointer(rhs_var);
                    let raw = builder.int_eq(
                        builder.cast_pointer_to_int(lhs),
                        builder.cast_pointer_to_int(rhs),
                    );
                    let res = self.new_bool(builder, state_var, raw);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::Panic => {
                    let val_var = self.variables[&ins.arguments[0]];
                    let proc = builder.load_pointer(proc_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::ProcessPanic;
                    let func = self.module.runtime_function(func_name);

                    builder.call_void(func, &[proc, val]);
                    builder.build_unreachable();
                }
                BuiltinFunction::PathAccessedAt => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::PathAccessedAt;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::PathCreatedAt => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::PathCreatedAt;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::PathModifiedAt => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::PathModifiedAt;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::PathExists => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::PathExists;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::PathIsDirectory => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::PathIsDirectory;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::PathIsFile => {
                    let reg_var = self.variables[&ins.register];
                    let path_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let path = builder.load_pointer(path_var).into();
                    let func_name = RuntimeFunction::PathIsFile;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, path]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::ProcessStackFrameLine => {
                    // TODO
                }
                BuiltinFunction::ProcessStackFrameName => {
                    // TODO
                }
                BuiltinFunction::ProcessStackFramePath => {
                    // TODO
                }
                BuiltinFunction::ProcessStacktrace => {
                    // TODO
                }
                BuiltinFunction::ProcessStacktraceDrop => {
                    // TODO
                }
                BuiltinFunction::ProcessStacktraceLength => {
                    // TODO
                }
                BuiltinFunction::ProcessSuspend => {
                    let reg_var = self.variables[&ins.register];
                    let time_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::ProcessSuspend;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, time]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::RandomBytes => {
                    let reg_var = self.variables[&ins.register];
                    let rng_var = self.variables[&ins.arguments[0]];
                    let size_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let rng = builder.load_pointer(rng_var).into();
                    let size = self.read_int(builder, size_var).into();
                    let func_name = RuntimeFunction::RandomBytes;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, rng, size]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::RandomDrop => {
                    let reg_var = self.variables[&ins.register];
                    let rng_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let rng = builder.load_pointer(rng_var).into();
                    let func_name = RuntimeFunction::RandomDrop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, rng]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::RandomFloat => {
                    let reg_var = self.variables[&ins.register];
                    let rng_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let rng = builder.load_pointer(rng_var).into();
                    let func_name = RuntimeFunction::RandomFloat;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, rng]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::RandomFloatRange => {
                    let reg_var = self.variables[&ins.register];
                    let rng_var = self.variables[&ins.arguments[0]];
                    let min_var = self.variables[&ins.arguments[1]];
                    let max_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let rng = builder.load_pointer(rng_var).into();
                    let min = self.read_int(builder, min_var).into();
                    let max = self.read_int(builder, max_var).into();
                    let func_name = RuntimeFunction::RandomFloatRange;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, rng, min, max]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::RandomFromInt => {
                    let reg_var = self.variables[&ins.register];
                    let seed_var = self.variables[&ins.arguments[0]];
                    let seed = self.read_int(builder, seed_var).into();
                    let func_name = RuntimeFunction::RandomFromInt;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[seed]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::RandomInt => {
                    let reg_var = self.variables[&ins.register];
                    let rng_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let rng = builder.load_pointer(rng_var).into();
                    let func_name = RuntimeFunction::RandomInt;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, rng]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::RandomIntRange => {
                    let reg_var = self.variables[&ins.register];
                    let rng_var = self.variables[&ins.arguments[0]];
                    let min_var = self.variables[&ins.arguments[1]];
                    let max_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let rng = builder.load_pointer(rng_var).into();
                    let min = self.read_int(builder, min_var).into();
                    let max = self.read_int(builder, max_var).into();
                    let func_name = RuntimeFunction::RandomIntRange;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, rng, min, max]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::RandomNew => {
                    let reg_var = self.variables[&ins.register];
                    let proc = builder.load_pointer(proc_var).into();
                    let func_name = RuntimeFunction::RandomNew;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[proc]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketAccept => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let time_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::SocketAccept;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, sock, time]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketAddressPairAddress => {
                    let reg_var = self.variables[&ins.register];
                    let pair_var = self.variables[&ins.arguments[1]];
                    let pair = builder.load_pointer(pair_var).into();
                    let func_name = RuntimeFunction::SocketAddressPairAddress;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[pair]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketAddressPairDrop => {
                    let reg_var = self.variables[&ins.register];
                    let pair_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let pair = builder.load_pointer(pair_var).into();
                    let func_name = RuntimeFunction::SocketAddressPairDrop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, pair]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketAddressPairPort => {
                    let reg_var = self.variables[&ins.register];
                    let pair_var = self.variables[&ins.arguments[1]];
                    let pair = builder.load_pointer(pair_var).into();
                    let func_name = RuntimeFunction::SocketAddressPairPort;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[pair]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketNew => {
                    let reg_var = self.variables[&ins.register];
                    let proto_var = self.variables[&ins.arguments[0]];
                    let kind_var = self.variables[&ins.arguments[1]];
                    let proto = self.read_int(builder, proto_var).into();
                    let kind = self.read_int(builder, kind_var).into();
                    let func_name = RuntimeFunction::SocketNew;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[proto, kind]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketBind => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let addr_var = self.variables[&ins.arguments[1]];
                    let port_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let addr = builder.load_pointer(addr_var).into();
                    let port = self.read_int(builder, port_var).into();
                    let func_name = RuntimeFunction::SocketBind;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, addr, port]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketConnect => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let addr_var = self.variables[&ins.arguments[1]];
                    let port_var = self.variables[&ins.arguments[2]];
                    let time_var = self.variables[&ins.arguments[3]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let addr = builder.load_pointer(addr_var).into();
                    let port = self.read_int(builder, port_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::SocketConnect;
                    let func = self.module.runtime_function(func_name);
                    let res = builder
                        .call(func, &[state, proc, sock, addr, port, time]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketDrop => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let func_name = RuntimeFunction::SocketDrop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketListen => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = self.read_int(builder, val_var).into();
                    let func_name = RuntimeFunction::SocketListen;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketLocalAddress => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let func_name = RuntimeFunction::SocketLocalAddress;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketPeerAddress => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let func_name = RuntimeFunction::SocketPeerAddress;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketRead => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let buf_var = self.variables[&ins.arguments[1]];
                    let size_var = self.variables[&ins.arguments[2]];
                    let time_var = self.variables[&ins.arguments[3]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let size = self.read_int(builder, size_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::SocketRead;
                    let func = self.module.runtime_function(func_name);
                    let res = builder
                        .call(func, &[state, proc, sock, buf, size, time]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketReceiveFrom => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let buf_var = self.variables[&ins.arguments[1]];
                    let size_var = self.variables[&ins.arguments[2]];
                    let time_var = self.variables[&ins.arguments[3]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let size = self.read_int(builder, size_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::SocketReceiveFrom;
                    let func = self.module.runtime_function(func_name);
                    let res = builder
                        .call(func, &[state, proc, sock, buf, size, time]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSendBytesTo => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let buf_var = self.variables[&ins.arguments[1]];
                    let addr_var = self.variables[&ins.arguments[2]];
                    let port_var = self.variables[&ins.arguments[3]];
                    let time_var = self.variables[&ins.arguments[4]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let addr = self.read_int(builder, addr_var).into();
                    let port = self.read_int(builder, port_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::SocketSendBytesTo;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(
                        func,
                        &[state, proc, sock, buf, addr, port, time],
                    );

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSendStringTo => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let buf_var = self.variables[&ins.arguments[1]];
                    let addr_var = self.variables[&ins.arguments[2]];
                    let port_var = self.variables[&ins.arguments[3]];
                    let time_var = self.variables[&ins.arguments[4]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let addr = self.read_int(builder, addr_var).into();
                    let port = self.read_int(builder, port_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::SocketSendStringTo;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(
                        func,
                        &[state, proc, sock, buf, addr, port, time],
                    );

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetBroadcast => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::SocketSetBroadcast;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetKeepalive => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::SocketSetKeepalive;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetLinger => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = self.read_int(builder, val_var).into();
                    let func_name = RuntimeFunction::SocketSetLinger;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetNodelay => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::SocketSetNodelay;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetOnlyV6 => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::SocketSetOnlyV6;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetRecvSize => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = self.read_int(builder, val_var).into();
                    let func_name = RuntimeFunction::SocketSetRecvSize;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetReuseAddress => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::SocketSetReuseAddress;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetReusePort => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = builder.load_pointer(val_var).into();
                    let func_name = RuntimeFunction::SocketSetReusePort;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetSendSize => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = self.read_int(builder, val_var).into();
                    let func_name = RuntimeFunction::SocketSetSendSize;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketSetTtl => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let val_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let val = self.read_int(builder, val_var).into();
                    let func_name = RuntimeFunction::SocketSetTtl;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock, val]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketShutdownRead => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let func_name = RuntimeFunction::SocketShutdownRead;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketShutdownReadWrite => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let func_name = RuntimeFunction::SocketShutdownReadWrite;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketShutdownWrite => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let func_name = RuntimeFunction::SocketShutdownWrite;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, sock]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketTryClone => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let sock = builder.load_pointer(sock_var).into();
                    let func_name = RuntimeFunction::SocketTryClone;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[sock]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketWriteBytes => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let buf_var = self.variables[&ins.arguments[1]];
                    let time_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::SocketWriteBytes;
                    let func = self.module.runtime_function(func_name);
                    let res =
                        builder.call(func, &[state, proc, sock, buf, time]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::SocketWriteString => {
                    let reg_var = self.variables[&ins.register];
                    let sock_var = self.variables[&ins.arguments[0]];
                    let buf_var = self.variables[&ins.arguments[1]];
                    let time_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let sock = builder.load_pointer(sock_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let time = self.read_int(builder, time_var).into();
                    let func_name = RuntimeFunction::SocketWriteString;
                    let func = self.module.runtime_function(func_name);
                    let res =
                        builder.call(func, &[state, proc, sock, buf, time]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StderrFlush => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let func_name = RuntimeFunction::StderrFlush;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StderrWriteBytes => {
                    let reg_var = self.variables[&ins.register];
                    let input_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let input = builder.load_pointer(input_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::StderrWriteBytes);

                    let ret = builder.call(func, &[state, proc, input]);

                    builder.store(reg_var, ret);
                }
                BuiltinFunction::StderrWriteString => {
                    let reg_var = self.variables[&ins.register];
                    let input_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let input = builder.load_pointer(input_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::StderrWriteString);

                    let ret = builder.call(func, &[state, proc, input]);

                    builder.store(reg_var, ret);
                }
                BuiltinFunction::StdinRead => {
                    let reg_var = self.variables[&ins.register];
                    let buf_var = self.variables[&ins.arguments[0]];
                    let size_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let buf = builder.load_pointer(buf_var).into();
                    let size = self.read_int(builder, size_var).into();
                    let func_name = RuntimeFunction::StdinRead;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, buf, size]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StdoutFlush => {
                    let reg_var = self.variables[&ins.register];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let func_name = RuntimeFunction::StdoutFlush;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StdoutWriteBytes => {
                    let reg_var = self.variables[&ins.register];
                    let input_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let input = builder.load_pointer(input_var).into();
                    let func_name = RuntimeFunction::StdoutWriteBytes;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, input]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StdoutWriteString => {
                    let reg_var = self.variables[&ins.register];
                    let input_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let proc = builder.load_pointer(proc_var).into();
                    let input = builder.load_pointer(input_var).into();
                    let func_name = RuntimeFunction::StdoutWriteString;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, proc, input]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringByte => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let index_var = self.variables[&ins.arguments[1]];
                    let string = builder.load_pointer(string_var).into();
                    let index = self.read_int(builder, index_var).into();
                    let func_name = RuntimeFunction::StringByte;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[string, index]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringCharacters => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let string = builder.load_pointer(string_var).into();
                    let func_name = RuntimeFunction::StringCharacters;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[string]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringCharactersDrop => {
                    let reg_var = self.variables[&ins.register];
                    let iter_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let iter = builder.load_pointer(iter_var).into();
                    let func_name = RuntimeFunction::StringCharactersDrop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, iter]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringCharactersNext => {
                    let reg_var = self.variables[&ins.register];
                    let iter_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let iter = builder.load_pointer(iter_var).into();
                    let func_name = RuntimeFunction::StringCharactersNext;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, iter]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringConcat => {
                    let reg_var = self.variables[&ins.register];
                    let len = builder.i64_literal(ins.arguments.len() as _);
                    let temp_var = self.new_stack_slot(
                        self.context
                            .pointer_type()
                            .array_type(ins.arguments.len() as _),
                    );

                    for (idx, reg) in ins.arguments.iter().enumerate() {
                        let val = builder.load_pointer(self.variables[reg]);

                        builder.store_array_field(temp_var, idx as _, val);
                    }

                    let state = builder.load_pointer(state_var).into();
                    let func_name = RuntimeFunction::StringConcat;
                    let func = self.module.runtime_function(func_name);
                    let res = builder
                        .call(func, &[state, temp_var.into(), len.into()]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringConcatArray => {
                    let reg_var = self.variables[&ins.register];
                    let ary_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let ary = builder.load_pointer(ary_var).into();
                    let func_name = RuntimeFunction::StringConcatArray;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, ary]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringDrop => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let string = builder.load_pointer(string_var).into();
                    let func_name = RuntimeFunction::StringDrop;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, string]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringEq => {
                    let reg_var = self.variables[&ins.register];
                    let lhs_var = self.variables[&ins.arguments[0]];
                    let rhs_var = self.variables[&ins.arguments[1]];
                    let state = builder.load_pointer(state_var).into();
                    let lhs = builder.load_pointer(lhs_var).into();
                    let rhs = builder.load_pointer(rhs_var).into();
                    let func = self
                        .module
                        .runtime_function(RuntimeFunction::StringEquals);
                    let ret = builder.call(func, &[state, lhs, rhs]);

                    builder.store(reg_var, ret);
                }
                BuiltinFunction::StringSize => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let string = builder.load_pointer(string_var).into();
                    let func_name = RuntimeFunction::StringSize;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, string]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringSliceBytes => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let start_var = self.variables[&ins.arguments[1]];
                    let len_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let string = builder.load_pointer(string_var).into();
                    let start = self.read_int(builder, start_var).into();
                    let len = self.read_int(builder, len_var).into();
                    let func_name = RuntimeFunction::StringSliceBytes;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, string, start, len]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringToByteArray => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let string = builder.load_pointer(string_var).into();
                    let func_name = RuntimeFunction::StringToByteArray;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, string]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringToFloat => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let start_var = self.variables[&ins.arguments[1]];
                    let end_var = self.variables[&ins.arguments[2]];
                    let state = builder.load_pointer(state_var).into();
                    let string = builder.load_pointer(string_var).into();
                    let start = builder.load_pointer(start_var).into();
                    let end = builder.load_pointer(end_var).into();
                    let func_name = RuntimeFunction::StringToFloat;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, string, start, end]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringToInt => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let radix_var = self.variables[&ins.arguments[1]];
                    let start_var = self.variables[&ins.arguments[2]];
                    let end_var = self.variables[&ins.arguments[3]];
                    let state = builder.load_pointer(state_var).into();
                    let string = builder.load_pointer(string_var).into();
                    let radix = builder.load_pointer(radix_var).into();
                    let start = builder.load_pointer(start_var).into();
                    let end = builder.load_pointer(end_var).into();
                    let func_name = RuntimeFunction::StringToInt;
                    let func = self.module.runtime_function(func_name);
                    let res =
                        builder.call(func, &[state, string, radix, start, end]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringToLower => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let string = builder.load_pointer(string_var).into();
                    let func_name = RuntimeFunction::StringToLower;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, string]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::StringToUpper => {
                    let reg_var = self.variables[&ins.register];
                    let string_var = self.variables[&ins.arguments[0]];
                    let state = builder.load_pointer(state_var).into();
                    let string = builder.load_pointer(string_var).into();
                    let func_name = RuntimeFunction::StringToUpper;
                    let func = self.module.runtime_function(func_name);
                    let res = builder.call(func, &[state, string]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::TimeMonotonic => {
                    let reg_var = self.variables[&ins.register];
                    let func_name = RuntimeFunction::TimeMonotonic;
                    let func = self.module.runtime_function(func_name);
                    let state = builder.load_pointer(state_var).into();
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::TimeSystem => {
                    let reg_var = self.variables[&ins.register];
                    let func_name = RuntimeFunction::TimeSystem;
                    let func = self.module.runtime_function(func_name);
                    let state = builder.load_pointer(state_var).into();
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::TimeSystemOffset => {
                    let reg_var = self.variables[&ins.register];
                    let func_name = RuntimeFunction::TimeSystemOffset;
                    let func = self.module.runtime_function(func_name);
                    let state = builder.load_pointer(state_var).into();
                    let res = builder.call(func, &[state]);

                    builder.store(reg_var, res);
                }
                BuiltinFunction::GetNil => unreachable!(),
                BuiltinFunction::Moved => unreachable!(),
                BuiltinFunction::PanicThrown => unreachable!(),
            },
            Instruction::Goto(ins) => {
                builder.build_unconditional_branch(llvm_blocks[ins.block.0]);
            }
            Instruction::Return(ins) => {
                let var = self.variables[&ins.register];
                let val = builder.load(var);
                let ret = if self.throws {
                    self.new_result_value(builder, val, false)
                } else {
                    val
                };

                builder.build_return(Some(&ret));
            }
            Instruction::Throw(ins) => {
                let var = self.variables[&ins.register];
                let val = builder.load(var);
                let ret = if self.throws {
                    self.new_result_value(builder, val, true)
                } else {
                    val
                };

                builder.build_return(Some(&ret));
            }
            Instruction::AllocateArray(ins) => {
                let state = builder.load_pointer(state_var).into();
                let len = builder.u64_literal(ins.values.len() as u64).into();
                let new_func =
                    self.module.runtime_function(RuntimeFunction::ArrayNew);
                let set_func =
                    self.module.runtime_function(RuntimeFunction::ArraySet);
                let array = builder.call(new_func, &[state, len]).into();

                for (idx, reg) in ins.values.iter().enumerate() {
                    let stack_var =
                        builder.load(self.variables[reg]).into_pointer_value();
                    let idx = builder.i64_literal(idx as _).into();
                    let val = builder.cast_to_pointer(stack_var).into();

                    builder.call(set_func, &[array, idx, val]);
                }
            }
            Instruction::Branch(ins) => {
                let cond_ptr =
                    builder.load_pointer(self.variables[&ins.condition]);

                // Load the `true` singleton from `State`.
                let state = builder.load_pointer(state_var).into();
                let bool_ptr =
                    builder.load_field(state, TRUE_INDEX).into_pointer_value();

                // Since our booleans are heap objects we have to
                // compare pointer addresses, and as such first have to
                // cast our pointers to ints.
                let cond_int = builder.cast_pointer_to_int(cond_ptr);
                let bool_int = builder.cast_pointer_to_int(bool_ptr);
                let cond = builder.int_eq(cond_int, bool_int);

                builder.build_conditional_branch(
                    cond,
                    llvm_blocks[ins.if_true.0],
                    llvm_blocks[ins.if_false.0],
                );
            }
            Instruction::BranchResult(ins) => {
                let res_var = self.variables[&ins.result];
                let ok_block = llvm_blocks[ins.ok.0];
                let err_block = llvm_blocks[ins.error.0];
                let tag_val = builder
                    .load_field(res_var, RESULT_TAG_INDEX)
                    .into_int_value();
                let ok_val = builder.u8_literal(RESULT_OK_VALUE);
                let condition = builder.int_eq(tag_val, ok_val);

                builder
                    .build_conditional_branch(condition, ok_block, err_block);
            }
            Instruction::Switch(ins) => {
                let reg_var = self.variables[&ins.register];
                let val = builder.load_pointer(reg_var);
                let addr = builder.cast_pointer_to_int(val);
                let shift = builder.i64_literal(INT_SHIFT as i64);
                let untagged = builder.right_shift(addr, shift);
                let mut cases = Vec::with_capacity(ins.blocks.len());

                for (index, block) in ins.blocks.iter().enumerate() {
                    cases.push((
                        builder.u64_literal(index as u64),
                        llvm_blocks[block.0],
                    ));
                }

                // Technically it doesn't matter which block we pick here as a
                // switch() is always exhaustive.
                let fallback = cases.last().unwrap().1;

                builder.build_switch(untagged, fallback, &cases);
            }
            Instruction::SwitchKind(ins) => {
                let val_var = self.variables[&ins.register];
                let kind_var = self.kind_of(&builder, val_var);
                let kind = builder.load(kind_var).into_int_value();

                // Now we can generate the switch that jumps to the correct
                // block based on the value kind.
                let owned_block = llvm_blocks[ins.blocks[0].0];
                let ref_block = llvm_blocks[ins.blocks[1].0];
                let atomic_block = llvm_blocks[ins.blocks[2].0];
                let perm_block = llvm_blocks[ins.blocks[3].0];
                let int_block = llvm_blocks[ins.blocks[4].0];
                let float_block = llvm_blocks[ins.blocks[5].0];
                let cases = [
                    (builder.u8_literal(OWNED_KIND), owned_block),
                    (builder.u8_literal(REF_KIND), ref_block),
                    (builder.u8_literal(ATOMIC_KIND), atomic_block),
                    (builder.u8_literal(PERMANENT_KIND), perm_block),
                    (builder.u8_literal(INT_KIND), int_block),
                    (builder.u8_literal(FLOAT_KIND), float_block),
                ];

                builder.build_switch(kind, owned_block, &cases);
            }
            Instruction::Nil(ins) => {
                let result = self.variables[&ins.register];
                let state = builder.load_pointer(state_var);
                let val = builder.load_field(state, NIL_INDEX);

                builder.store(result, val);
            }
            Instruction::True(ins) => {
                let result = self.variables[&ins.register];
                let state = builder.load_pointer(state_var);
                let val = builder.load_field(state, TRUE_INDEX);

                builder.store(result, val);
            }
            Instruction::False(ins) => {
                let result = self.variables[&ins.register];
                let state = builder.load_pointer(state_var);
                let val = builder.load_field(state, FALSE_INDEX);

                builder.store(result, val);
            }
            Instruction::Int(ins) => {
                let result = self.variables[&ins.register];

                if let Some(ptr) = builder.tagged_int(ins.value) {
                    builder.store(result, ptr);
                } else {
                    let global =
                        self.module.add_literal(&Constant::Int(ins.value));

                    builder.load_global_to_stack(result, global);
                }
            }
            Instruction::Float(ins) => {
                let var = self.variables[&ins.register];
                let global =
                    self.module.add_literal(&Constant::Float(ins.value));

                builder.load_global_to_stack(var, global);
            }
            Instruction::String(ins) => {
                let var = self.variables[&ins.register];
                let global = self
                    .module
                    .add_literal(&Constant::String(ins.value.clone()));

                builder.load_global_to_stack(var, global);
            }
            Instruction::MoveRegister(ins) => {
                let source = self.variables[&ins.source];
                let target = self.variables[&ins.target];

                builder.store(target, builder.load(source));
            }
            Instruction::MoveResult(ins) => {
                let reg_var = self.variables[&ins.register];
                let res_var = self.variables[&ins.result];
                let val = builder.load_field(res_var, RESULT_VALUE_INDEX as _);

                builder.store(reg_var, val);
            }
            Instruction::CallStatic(ins) => {
                let reg_var = self.variables[&ins.register];
                let func_name = &self.names.methods[&ins.method];
                let func = self.module.add_method(func_name, ins.method);
                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    builder.load_pointer(state_var).into(),
                    builder.load_pointer(proc_var).into(),
                ];

                for reg in &ins.arguments {
                    args.push(builder.load_pointer(self.variables[reg]).into());
                }

                builder.store(reg_var, builder.call(func, &args));
            }
            Instruction::CallInstance(ins) => {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let func_name = &self.names.methods[&ins.method];
                let func = self.module.add_method(func_name, ins.method);
                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    builder.load_pointer(state_var).into(),
                    builder.load_pointer(proc_var).into(),
                    builder.load_pointer(rec_var).into(),
                ];

                for reg in &ins.arguments {
                    args.push(builder.load_pointer(self.variables[reg]).into());
                }

                builder.store(reg_var, builder.call(func, &args));
            }
            Instruction::CallDynamic(ins) => {
                // For dynamic dispatch we use hashing as described in
                // https://thume.ca/2019/07/29/shenanigans-with-hash-tables/.
                //
                // Probing is only performed if collisions are known to be
                // possible for a certain hash.
                let loop_start = self.add_basic_block();
                let after_loop = self.add_basic_block();

                let index_var = self.new_stack_slot(self.context.i64_type());
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];

                let rec = builder.load_pointer(rec_var);
                let info = &self.types.methods[&ins.method];
                let class = self.class_of(builder, rec);
                let rec_class = builder
                    .cast_to_typed_pointer(class, self.types.empty_class);

                // (class.method_slots - 1) as u64
                let len = builder.build_int_cast(
                    builder.int_sub(
                        builder
                            .load_field(rec_class, CLASS_METHODS_COUNT_INDEX)
                            .into_int_value(),
                        builder.u16_literal(1),
                    ),
                    self.context.i64_type(),
                    "",
                );

                let hash = builder.u64_literal(info.hash);

                builder.store(index_var, hash);

                let space = AddressSpace::Generic;
                let signature =
                    self.types.methods[&ins.method].signature.ptr_type(space);
                let func_var = self.new_stack_slot(signature);

                builder.build_unconditional_branch(loop_start);

                // The start of the probing loop (probing is necessary).
                builder.position_at_end(loop_start);

                // slot = index & len
                let index = builder.load(index_var).into_int_value();
                let slot = builder.bit_and(index, len);
                let method_addr = unsafe {
                    builder.build_gep(
                        rec_class,
                        &[
                            builder.u32_literal(0),
                            builder.u32_literal(CLASS_METHODS_INDEX as _),
                            slot,
                        ],
                        "",
                    )
                };

                let method = builder.load(method_addr).into_struct_value();

                // We only generate the probing code when it's actually
                // necessary. In practise most dynamic dispatch call sites won't
                // need probing.
                if info.collision {
                    let eq_block = self.add_basic_block();
                    let ne_block = self.add_basic_block();

                    // method.hash == hash
                    let hash_eq = builder.int_eq(
                        builder
                            .extract_field(method, METHOD_HASH_INDEX)
                            .into_int_value(),
                        hash,
                    );

                    builder
                        .build_conditional_branch(hash_eq, eq_block, ne_block);

                    // The block to jump to when the hash codes didn't match.
                    builder.position_at_end(ne_block);
                    builder.store(
                        index_var,
                        builder.int_add(index, builder.u64_literal(1)),
                    );
                    builder.build_unconditional_branch(loop_start);

                    // The block to jump to when the hash codes matched
                    builder.position_at_end(eq_block);
                }

                let func_ptr =
                    builder.extract_field(method, METHOD_FUNCTION_INDEX);

                builder.store(
                    func_var,
                    builder.build_bitcast(func_ptr, signature, ""),
                );

                builder.build_unconditional_branch(after_loop);

                // The block to jump to at the end of the loop, used for
                // calling the native function.
                builder.position_at_end(after_loop);

                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    builder.load_pointer(state_var).into(),
                    builder.load_pointer(proc_var).into(),
                    rec.into(),
                ];

                for reg in &ins.arguments {
                    args.push(builder.load_pointer(self.variables[reg]).into());
                }

                let callable =
                    CallableValue::try_from(builder.load_pointer(func_var))
                        .unwrap();

                builder.store(reg_var, builder.call(callable, &args));
            }
            Instruction::CallClosure(ins) => {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let space = AddressSpace::Generic;

                // For closures we generate the signature on the fly, as the
                // method for `call` isn't always clearly defined: for an
                // argument typed as a closure, we don't know what the actual
                // method is, thus we can't retrieve an existing signature.
                let mut sig_args: Vec<BasicMetadataTypeEnum> = vec![
                    self.types.state.ptr_type(space).into(), // State
                    self.context.pointer_type().into(),      // Process
                    self.context.pointer_type().into(),      // Closure
                ];

                for _ in &ins.arguments {
                    sig_args.push(self.context.pointer_type().into());
                }

                let sig = if ins.throws {
                    self.types.result.fn_type(&sig_args, false)
                } else {
                    self.context.pointer_type().fn_type(&sig_args, false)
                };

                let func = sig.ptr_type(space);

                // Load the method from the method table.
                let rec = builder.load_pointer(rec_var);
                let header =
                    builder.cast_to_untagged_pointer(rec, self.types.header);
                let class = builder.cast_to_typed_pointer(
                    builder
                        .load_field(header, HEADER_CLASS_INDEX)
                        .into_pointer_value(),
                    self.types.empty_class,
                );

                let method_addr = unsafe {
                    builder.build_gep(
                        class,
                        &[
                            builder.u32_literal(0),
                            builder.u32_literal(CLASS_METHODS_INDEX as _),
                            builder.u32_literal(CLOSURE_CALL_INDEX as _),
                            builder.u32_literal(METHOD_FUNCTION_INDEX),
                        ],
                        "",
                    )
                };

                let method = builder
                    .build_bitcast(builder.load_pointer(method_addr), func, "")
                    .into_pointer_value();

                // Now we can call the method.
                let mut args: Vec<BasicMetadataValueEnum> = vec![
                    builder.load_pointer(state_var).into(),
                    builder.load_pointer(proc_var).into(),
                    rec.into(),
                ];

                for reg in &ins.arguments {
                    args.push(builder.load_pointer(self.variables[reg]).into());
                }

                let callable = CallableValue::try_from(method).unwrap();

                builder.store(reg_var, builder.call(callable, &args));
            }
            Instruction::CallDropper(_) => {
                // TODO: implement
            }
            Instruction::Send(ins) => {
                let rec_var = self.variables[&ins.receiver];
                let method_name = &self.names.methods[&ins.method];
                let method = builder
                    .cast_to_pointer(
                        self.module
                            .add_method(method_name, ins.method)
                            .as_global_value()
                            .as_pointer_value(),
                    )
                    .into();
                let len = builder.u8_literal(ins.arguments.len() as u8).into();
                let message_new =
                    self.module.runtime_function(RuntimeFunction::MessageNew);
                let send_message = self
                    .module
                    .runtime_function(RuntimeFunction::ProcessSendMessage);
                let message = builder
                    .call(message_new, &[method, len])
                    .into_pointer_value();

                // The receiver doesn't need to be stored in the message, as
                // each async method sets `self` to the process running it.
                for (index, reg) in ins.arguments.iter().enumerate() {
                    let addr = unsafe {
                        builder.build_gep(
                            message,
                            &[
                                builder.u32_literal(0),
                                builder.u32_literal(MESSAGE_ARGUMENTS_INDEX),
                                builder.u32_literal(index as _),
                            ],
                            "",
                        )
                    };

                    builder
                        .store(addr, builder.load_pointer(self.variables[reg]));
                }

                let state = builder.load_pointer(state_var).into();
                let sender = builder.load_pointer(proc_var).into();
                let receiver = builder.load_pointer(rec_var).into();

                builder.call_void(
                    send_message,
                    &[state, sender, receiver, message.into()],
                );
            }
            Instruction::GetField(ins) => {
                let reg_var = self.variables[&ins.register];
                let rec_var = self.variables[&ins.receiver];
                let index = (ins.field.index(self.db) + FIELD_OFFSET) as u32;
                let rec = builder.load_pointer(rec_var);

                // TODO: the problem is this: if the receiver is typed as
                // `Self`, and we are in a closure, then here we'll resolve the
                // type's class to that of the closure, not of the actual
                // receiver.
                let class = self
                    .register_type(ins.receiver)
                    .class_id(self.db, self.class_id)
                    .unwrap();
                let layout = self.types.instances[&class];

                // TODO: remove
                if index as usize >= layout.get_field_types().len() {
                    println!(
                        "{} index {} out of bounds, length: {}, location: {}:{}",
                        class.name(self.db),
                        index,
                        layout.get_field_types().len(),
                        self.mir.location(ins.location).module.file(self.db).display(),
                        self.mir.location(ins.location).range.line_range.start(),
                    );
                }

                let source = builder.cast_to_untagged_pointer(rec, layout);
                let field = builder.load_field(source, index);

                builder.store(reg_var, field);
            }
            Instruction::SetField(ins) => {
                let rec_var = self.variables[&ins.receiver];
                let val_var = self.variables[&ins.value];
                let index = (ins.field.index(self.db) + FIELD_OFFSET) as u32;
                let rec = builder.load_pointer(rec_var);
                let val = builder.load_pointer(val_var);
                let class = self
                    .register_type(ins.receiver)
                    .class_id(self.db, self.class_id)
                    .unwrap();
                let layout = self.types.instances[&class];
                let source = builder.cast_to_untagged_pointer(rec, layout);

                builder.store_field(source, index, val);
            }
            Instruction::CheckRefs(ins) => {
                let var = self.variables[&ins.register];
                let proc = builder.load_pointer(proc_var).into();
                let val = builder.load_pointer(var);
                let check = builder.cast_to_pointer(val).into();
                let func =
                    self.module.runtime_function(RuntimeFunction::CheckRefs);

                builder.call_void(func, &[proc, check]);
            }
            Instruction::Free(ins) => {
                let var = self.variables[&ins.register];
                let val = builder.load_pointer(var);
                let free = builder.cast_to_pointer(val).into();
                let func = self.module.runtime_function(RuntimeFunction::Free);

                builder.call_void(func, &[free]);
            }
            Instruction::Clone(ins) => {
                let reg_var = self.variables[&ins.register];
                let val_var = self.variables[&ins.source];
                let val = builder.load_pointer(val_var);

                match ins.kind {
                    CloneKind::Float => {
                        let state = builder.load_pointer(state_var);
                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::FloatClone);
                        let result = builder
                            .call(func, &[state.into(), val.into()])
                            .into_pointer_value();

                        builder.store(reg_var, result);
                    }
                    CloneKind::Int => {
                        let addr = builder.cast_pointer_to_int(val);
                        let mask = builder.i64_literal(INT_MASK);
                        let bits = builder.bit_and(addr, mask);
                        let cond = builder.int_eq(bits, mask);
                        let after_block = self.add_basic_block();
                        let tagged_block = self.add_basic_block();
                        let heap_block = self.add_basic_block();

                        builder.build_conditional_branch(
                            cond,
                            tagged_block,
                            heap_block,
                        );

                        // The block to jump to when the Int is a tagged Int.
                        builder.position_at_end(tagged_block);
                        builder.store(reg_var, val);
                        builder.build_unconditional_branch(after_block);

                        // The block to jump to when the Int is a boxed Int.
                        builder.position_at_end(heap_block);

                        let func = self
                            .module
                            .runtime_function(RuntimeFunction::IntClone);
                        let state = builder.load_pointer(state_var);
                        let result = builder
                            .call(func, &[state.into(), val.into()])
                            .into_pointer_value();

                        builder.store(reg_var, result);
                        builder.build_unconditional_branch(after_block);

                        builder.position_at_end(after_block);
                    }
                }
            }
            Instruction::Increment(ins) => {
                let reg_var = self.variables[&ins.register];
                let val_var = self.variables[&ins.value];
                let val = builder.load_pointer(val_var);
                let header =
                    builder.cast_to_untagged_pointer(val, self.types.header);
                let one = builder.u32_literal(1);
                let old = builder
                    .load_field(header, HEADER_REFS_INDEX)
                    .into_int_value();
                let new = builder.build_int_add(old, one, "");

                builder.store_field(header, HEADER_REFS_INDEX, new);

                let old_addr = builder.cast_pointer_to_int(val);
                let mask = builder.i64_literal(REF_MASK);
                let new_addr = builder.bit_or(old_addr, mask);
                let ref_ptr = builder.build_int_to_ptr(
                    new_addr,
                    self.context.pointer_type(),
                    "",
                );

                builder.store(reg_var, ref_ptr);
            }
            Instruction::Decrement(ins) => {
                let var = self.variables[&ins.register];
                let header = builder.cast_to_untagged_pointer(
                    builder.load_pointer(var),
                    self.types.header,
                );

                let old_refs = builder
                    .load_field(header, HEADER_REFS_INDEX)
                    .into_int_value();
                let one = builder.u32_literal(1);
                let new_refs = builder.build_int_sub(old_refs, one, "");

                builder.store_field(header, HEADER_REFS_INDEX, new_refs);
            }
            Instruction::IncrementAtomic(ins) => {
                let reg_var = self.variables[&ins.register];
                let val_var = self.variables[&ins.value];
                let val = builder.load_pointer(val_var);
                let header = builder.cast_to_header(val);
                let one = builder.u32_literal(1);
                let field = builder
                    .build_struct_gep(header, HEADER_REFS_INDEX as u32, "")
                    .unwrap();
                let op = AtomicRMWBinOp::Add;
                let order = AtomicOrdering::AcquireRelease;

                builder.build_atomicrmw(op, field, one, order).unwrap();
                builder.store(reg_var, val);
            }
            Instruction::DecrementAtomic(ins) => {
                let var = self.variables[&ins.register];
                let header = builder.cast_to_header(builder.load_pointer(var));
                let one = builder.u32_literal(1);
                let field = builder
                    .build_struct_gep(header, HEADER_REFS_INDEX as u32, "")
                    .unwrap();
                let op = AtomicRMWBinOp::Sub;
                let order = AtomicOrdering::AcquireRelease;
                let old_refs =
                    builder.build_atomicrmw(op, field, one, order).unwrap();
                let is_zero = builder.int_eq(old_refs, one);

                builder.build_conditional_branch(
                    is_zero,
                    llvm_blocks[ins.if_true.0],
                    llvm_blocks[ins.if_false.0],
                );
            }
            Instruction::Allocate(ins) => {
                let reg_var = self.variables[&ins.register];
                let name = &self.names.classes[&ins.class];
                let global = builder.load_pointer(
                    self.module.add_class(ins.class, name).as_pointer_value(),
                );
                let class = builder.cast_to_pointer(global).into();
                let func =
                    self.module.runtime_function(RuntimeFunction::ObjectNew);
                let ptr = builder.call(func, &[class]);

                builder.store(reg_var, ptr);
            }
            Instruction::Spawn(ins) => {
                let reg_var = self.variables[&ins.register];
                let name = &self.names.classes[&ins.class];
                let global = builder.load_pointer(
                    self.module.add_class(ins.class, name).as_pointer_value(),
                );
                let class = builder.cast_to_pointer(global).into();
                let proc = builder.load_pointer(proc_var).into();
                let func =
                    self.module.runtime_function(RuntimeFunction::ProcessNew);
                let ptr = builder.call(func, &[proc, class]);

                builder.store(reg_var, ptr);
            }
            Instruction::GetConstant(ins) => {
                let var = self.variables[&ins.register];
                let name = &self.names.constants[&ins.id];
                let global = self.module.add_constant(name);

                builder.load_global_to_stack(var, global);
            }
            Instruction::Reduce(ins) => {
                let amount = self
                    .context
                    .i16_type()
                    .const_int(ins.amount as u64, false)
                    .into();
                let proc = builder.load_pointer(proc_var).into();
                let func =
                    self.module.runtime_function(RuntimeFunction::Reduce);

                builder.call_void(func, &[proc, amount]);
            }
            Instruction::Finish(ins) => {
                let proc = builder.load_pointer(proc_var).into();
                let terminate = self
                    .context
                    .bool_type()
                    .const_int(ins.terminate as _, false)
                    .into();
                let func = self
                    .module
                    .runtime_function(RuntimeFunction::ProcessFinishMessage);

                builder.call_void(func, &[proc, terminate]);
                builder.build_unreachable();
            }
            Instruction::Reference(_) => unreachable!(),
            Instruction::Drop(_) => unreachable!(),
        }
    }

    fn kind_of(
        &mut self,
        builder: &Builder<'a, 'ctx>,
        pointer_variable: PointerValue<'ctx>,
    ) -> PointerValue<'ctx> {
        // Instead of fiddling with phi nodes we just inject a new stack slot in
        // the entry block and use that. clang takes a similar approach when
        // building switch() statements.
        let result = self.new_stack_slot(self.context.i8_type());
        let perm_block = self.add_basic_block();
        let ref_block = self.add_basic_block();
        let header_block = self.add_basic_block();
        let after_block = self.add_basic_block();
        let pointer = builder.load_pointer(pointer_variable);
        let addr = builder.cast_pointer_to_int(pointer);
        let mask = builder.i64_literal(TAG_MASK);
        let bits = builder.bit_and(addr, mask);

        // This generates the equivalent of the following:
        //
        //     match ptr as usize & MASK {
        //       INT_MASK => ...
        //       REF_MASK => ...
        //       _        => ...
        //     }
        builder.build_switch(
            bits,
            header_block,
            &[
                (builder.i64_literal(INT_MASK), perm_block),
                (builder.i64_literal(REF_MASK), ref_block),
            ],
        );

        // The case for when the value is a tagged integer.
        builder.position_at_end(perm_block);
        builder.store(result, builder.u8_literal(PERMANENT_KIND));
        builder.build_unconditional_branch(after_block);

        // The case for when the value is a reference.
        builder.position_at_end(ref_block);
        builder.store(result, builder.u8_literal(REF_KIND));
        builder.build_unconditional_branch(after_block);

        // The fallback case where we read the kind from the object header. This
        // generates the equivalent of `(*(ptr as *mut Header)).kind`.
        builder.position_at_end(header_block);

        let header = builder.cast_to_header(pointer);
        let header_val =
            builder.load_field(header, HEADER_KIND_INDEX).into_int_value();

        builder.store(result, header_val);
        builder.build_unconditional_branch(after_block);
        builder.position_at_end(after_block);
        result
    }

    fn class_of(
        &mut self,
        builder: &Builder<'a, 'ctx>,
        receiver: PointerValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let tagged_block = self.add_basic_block();
        let heap_block = self.add_basic_block();
        let after_block = self.add_basic_block();
        let class_var = self.new_stack_slot(self.context.pointer_type());
        let int_class = self
            .module
            .add_class(ClassId::int(), &self.names.classes[&ClassId::int()]);

        let addr = builder.cast_pointer_to_int(receiver);
        let mask = builder.i64_literal(INT_MASK);
        let bits = builder.bit_and(addr, mask);
        let is_tagged = builder.int_eq(bits, mask);

        builder.build_conditional_branch(is_tagged, tagged_block, heap_block);

        // The block to jump to when the receiver is a tagged integer.
        builder.position_at_end(tagged_block);
        builder.store(
            class_var,
            builder.cast_to_pointer(
                builder.load_pointer(int_class.as_pointer_value()),
            ),
        );
        builder.build_unconditional_branch(after_block);

        // The block to jump to when the receiver is a heap object. In this case
        // we read the class from the (untagged) header.
        builder.position_at_end(heap_block);

        let header =
            builder.cast_to_untagged_pointer(receiver, self.types.header);
        let class =
            builder.load_field(header, HEADER_CLASS_INDEX).into_pointer_value();

        builder.store(class_var, class);
        builder.build_unconditional_branch(after_block);

        // The block to jump to to load the method pointer.
        builder.position_at_end(after_block);
        builder.load_pointer(class_var)
    }

    fn read_int(
        &mut self,
        builder: &Builder<'a, 'ctx>,
        variable: PointerValue<'ctx>,
    ) -> IntValue<'ctx> {
        let pointer = builder.load_pointer(variable);
        let res_var = self.new_stack_slot(self.context.i64_type());
        let tagged_block = self.add_basic_block();
        let heap_block = self.add_basic_block();
        let after_block = self.add_basic_block();

        let addr = builder.cast_pointer_to_int(pointer);
        let mask = builder.i64_literal(INT_MASK);
        let bits = builder.bit_and(addr, mask);
        let cond = builder.int_eq(bits, mask);

        builder.build_conditional_branch(cond, tagged_block, heap_block);

        // The block to jump to when the Int is a tagged Int.
        builder.position_at_end(tagged_block);

        let shift = builder.i64_literal(INT_SHIFT as i64);
        let untagged = builder.signed_right_shift(addr, shift);

        builder.store(res_var, untagged);
        builder.build_unconditional_branch(after_block);

        // The block to jump to when the Int is a heap Int.
        builder.position_at_end(heap_block);

        let layout = self.types.instances[&ClassId::int()];
        let casted = builder.cast_to_typed_pointer(pointer, layout);

        builder
            .store(res_var, builder.load_field(casted, BOXED_INT_VALUE_INDEX));
        builder.build_unconditional_branch(after_block);

        builder.position_at_end(after_block);
        builder.load(res_var).into_int_value()
    }

    fn read_float(
        &mut self,
        builder: &Builder<'a, 'ctx>,
        variable: PointerValue<'ctx>,
    ) -> FloatValue<'ctx> {
        let raw_ptr = builder.load_pointer(variable);
        let layout = self.types.instances[&ClassId::float()];
        let float_ptr = builder.cast_to_typed_pointer(raw_ptr, layout);

        builder
            .load_field(float_ptr, BOXED_FLOAT_VALUE_INDEX)
            .into_float_value()
    }

    fn new_float(
        &mut self,
        builder: &Builder<'a, 'ctx>,
        state_var: PointerValue<'ctx>,
        value: FloatValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let func = self.module.runtime_function(RuntimeFunction::FloatBoxed);
        let state = builder.load_pointer(state_var);

        builder.call(func, &[state.into(), value.into()]).into_pointer_value()
    }

    fn checked_int_operation(
        &mut self,
        name: &str,
        builder: &Builder<'a, 'ctx>,
        state_var: PointerValue<'ctx>,
        proc_var: PointerValue<'ctx>,
        reg_var: PointerValue<'ctx>,
        lhs_var: PointerValue<'ctx>,
        rhs_var: PointerValue<'ctx>,
    ) {
        let ok_block = self.add_basic_block();
        let err_block = self.add_basic_block();
        let after_block = self.add_basic_block();
        let lhs = self.read_int(builder, lhs_var);
        let rhs = self.read_int(builder, rhs_var);
        let func = self.module.runtime_function(RuntimeFunction::IntOverflow);
        let add = Intrinsic::find(name)
            .and_then(|intr| {
                intr.get_declaration(
                    &self.module.inner,
                    &[self.context.i64_type().into()],
                )
            })
            .unwrap();

        let res =
            builder.call(add, &[lhs.into(), rhs.into()]).into_struct_value();

        // Check if we overflowed the operation.
        let new_val = builder
            .extract_field(res, LLVM_RESULT_VALUE_INDEX)
            .into_int_value();
        let overflow = builder
            .extract_field(res, LLVM_RESULT_STATUS_INDEX)
            .into_int_value();

        builder.build_conditional_branch(overflow, err_block, ok_block);

        // The block to jump to if the operation didn't overflow.
        builder.position_at_end(ok_block);
        builder.store(reg_var, self.new_int(builder, state_var, new_val));
        builder.build_unconditional_branch(after_block);

        // The block to jump to if the operation overflowed.
        builder.position_at_end(err_block);

        let proc = builder.load_pointer(proc_var);

        builder.call_void(func, &[proc.into(), lhs.into(), rhs.into()]);
        builder.build_unreachable();
        builder.position_at_end(after_block);
    }

    fn new_int(
        &mut self,
        builder: &Builder<'a, 'ctx>,
        state_var: PointerValue<'ctx>,
        value: IntValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let res_var = self.new_stack_slot(self.context.pointer_type());
        let tagged_block = self.add_basic_block();
        let heap_block = self.add_basic_block();
        let after_block = self.add_basic_block();
        let and_block = self.add_basic_block();

        let min = builder.i64_literal(MIN_INT);
        let max = builder.i64_literal(MAX_INT);

        builder.build_conditional_branch(
            builder.int_ge(value, min),
            and_block,
            heap_block,
        );

        // The block to jump to when we're larger than or equal to the minimum
        // value for a tagged Int.
        builder.position_at_end(and_block);
        builder.build_conditional_branch(
            builder.int_le(value, max),
            tagged_block,
            heap_block,
        );

        // The block to jump to when the Int fits in a tagged pointer.
        builder.position_at_end(tagged_block);

        let shift = builder.i64_literal(INT_SHIFT as i64);
        let mask = builder.i64_literal(INT_MASK);
        let addr = builder.bit_or(builder.left_shift(value, shift), mask);

        builder.store(res_var, builder.cast_int_to_pointer(addr));
        builder.build_unconditional_branch(after_block);

        // The block to jump to when the Int must be boxed.
        builder.position_at_end(heap_block);

        let func = self.module.runtime_function(RuntimeFunction::IntBoxed);
        let state = builder.load_pointer(state_var);
        let res = builder.call(func, &[state.into(), value.into()]);

        builder.store(res_var, res);
        builder.build_unconditional_branch(after_block);

        builder.position_at_end(after_block);
        builder.load_pointer(res_var)
    }

    fn new_bool(
        &mut self,
        builder: &Builder<'a, 'ctx>,
        state_var: PointerValue<'ctx>,
        value: IntValue<'ctx>,
    ) -> PointerValue<'ctx> {
        let result = self.new_stack_slot(self.context.pointer_type());
        let state = builder.load_pointer(state_var);
        let true_block = self.add_basic_block();
        let false_block = self.add_basic_block();
        let after_block = self.add_basic_block();

        builder.build_conditional_branch(value, true_block, false_block);

        // The block to jump to when the condition is true.
        builder.position_at_end(true_block);
        builder.store(result, builder.load_field(state, TRUE_INDEX));
        builder.build_unconditional_branch(after_block);

        // The block to jump to when the condition is false.
        builder.position_at_end(false_block);
        builder.store(result, builder.load_field(state, FALSE_INDEX));
        builder.build_unconditional_branch(after_block);

        builder.position_at_end(after_block);
        builder.load_pointer(result)
    }

    fn new_stack_slot<T: BasicType<'ctx>>(
        &mut self,
        value_type: T,
    ) -> PointerValue<'ctx> {
        let builder = Builder::new(self.context, self.types);
        let block = self.function.get_first_basic_block().unwrap();

        if let Some(ins) = block.get_first_instruction() {
            builder.position_before(&ins);
        } else {
            builder.position_at_end(block);
        }

        builder.build_alloca(value_type, "")
    }

    fn check_division_overflow(
        &self,
        builder: &Builder<'a, 'ctx>,
        process_var: PointerValue<'ctx>,
        lhs: IntValue<'ctx>,
        rhs: IntValue<'ctx>,
    ) {
        let min = builder.i64_literal(i64::MIN);
        let minus_one = builder.i64_literal(-1);
        let zero = builder.i64_literal(0);
        let and_block = self.add_basic_block();
        let or_block = self.add_basic_block();
        let overflow_block = self.add_basic_block();
        let ok_block = self.add_basic_block();

        // lhs == MIN AND rhs == -1
        builder.build_conditional_branch(
            builder.int_eq(lhs, min),
            and_block,
            or_block,
        );

        builder.position_at_end(and_block);
        builder.build_conditional_branch(
            builder.int_eq(rhs, minus_one),
            overflow_block,
            or_block,
        );

        // OR rhs == 0
        builder.position_at_end(or_block);
        builder.build_conditional_branch(
            builder.int_eq(rhs, zero),
            overflow_block,
            ok_block,
        );

        // The block to jump to if an overflow would occur.
        builder.position_at_end(overflow_block);

        let func = self.module.runtime_function(RuntimeFunction::IntOverflow);
        let proc = builder.load_pointer(process_var);

        builder.call_void(func, &[proc.into(), lhs.into(), rhs.into()]);
        builder.build_unreachable();

        // The block to jump to when it's safe to perform the
        // operation.
        builder.position_at_end(ok_block);
    }

    fn check_shift_bits(
        &self,
        process_var: PointerValue<'ctx>,
        builder: &Builder<'a, 'ctx>,
        value: IntValue<'ctx>,
        bits: IntValue<'ctx>,
    ) {
        let ok_block = self.add_basic_block();
        let err_block = self.add_basic_block();
        let cond =
            builder.int_gt(bits, builder.i64_literal((i64::BITS - 1) as _));

        builder.build_conditional_branch(cond, err_block, ok_block);

        // The block to jump to when the operation would overflow.
        builder.position_at_end(err_block);

        let func = self.module.runtime_function(RuntimeFunction::IntOverflow);
        let proc = builder.load_pointer(process_var);

        builder.call_void(func, &[proc.into(), value.into(), bits.into()]);
        builder.build_unreachable();

        // The block to jump to when all is well.
        builder.position_at_end(ok_block);
    }

    fn define_register_variables(&mut self, builder: &Builder<'a, 'ctx>) {
        for (index, reg) in self.method.registers.iter().enumerate() {
            let id = RegisterId(index);

            let typ = if let RegisterKind::Result = reg.kind {
                self.types.result.as_basic_type_enum()
            } else {
                self.context.pointer_type().as_basic_type_enum()
            };

            self.variables.insert(id, builder.build_alloca(typ, ""));
        }
    }

    fn register_type(&self, register: RegisterId) -> types::TypeRef {
        self.method.registers.value_type(register)
    }

    fn add_basic_block(&self) -> BasicBlock<'ctx> {
        self.context.append_basic_block(self.function, "")
    }

    fn new_result_value(
        &mut self,
        builder: &Builder<'a, 'ctx>,
        value: BasicValueEnum<'ctx>,
        error: bool,
    ) -> BasicValueEnum<'ctx> {
        let slot = self.new_stack_slot(self.types.result);
        let tag = if error { RESULT_ERROR_VALUE } else { RESULT_OK_VALUE };
        let tag_lit = builder.u8_literal(tag);

        builder.store_field(slot, RESULT_TAG_INDEX, tag_lit);
        builder.store_field(slot, RESULT_VALUE_INDEX, value);
        builder.load(slot)
    }
}

/// A pass for generating the entry module and method (i.e. `main()`).
pub(crate) struct GenerateMain<'a, 'ctx> {
    db: &'a Database,
    mir: &'a Mir,
    types: &'a Types<'ctx>,
    names: &'a SymbolNames,
    context: &'ctx Context,
    module: &'a Module<'a, 'ctx>,
}

impl<'a, 'ctx> GenerateMain<'a, 'ctx> {
    fn run(mut self) {
        let space = AddressSpace::Generic;
        let builder = Builder::new(self.context, self.types);
        let fn_type = self.context.i32_type().fn_type(&[], false);
        let fn_val = self.module.add_function("main", fn_type, None);
        let entry_block = self.context.append_basic_block(fn_val, "");

        // TODO: move to dedicated type/function
        builder.position_at_end(entry_block);

        let counts = builder.build_alloca(self.types.method_counts, "");

        builder.store_field(counts, 0, self.methods(ClassId::int()));
        builder.store_field(counts, 1, self.methods(ClassId::float()));
        builder.store_field(counts, 2, self.methods(ClassId::string()));
        builder.store_field(counts, 3, self.methods(ClassId::array()));
        builder.store_field(counts, 4, self.methods(ClassId::boolean()));
        builder.store_field(counts, 5, self.methods(ClassId::nil()));
        builder.store_field(counts, 6, self.methods(ClassId::byte_array()));
        builder.store_field(counts, 7, self.methods(ClassId::channel()));

        let rt_new = self.module.runtime_function(RuntimeFunction::RuntimeNew);
        let rt_start =
            self.module.runtime_function(RuntimeFunction::RuntimeStart);
        let rt_state =
            self.module.runtime_function(RuntimeFunction::RuntimeState);

        let runtime =
            builder.call(rt_new, &[counts.into()]).into_pointer_value();
        let state = builder.cast_to_typed_pointer(
            builder.call(rt_state, &[runtime.into()]).into_pointer_value(),
            self.types.state,
        );

        // Call all the module setup functions. This is used to populate
        // constants, define classes, etc.
        for &id in self.mir.modules.keys() {
            let name = &self.names.setup_functions[&id];
            let func = self.module.add_setup_function(name);

            builder.call_void(func, &[state.into()]);
        }

        let main_class_id = self.db.main_class().unwrap();
        let main_method_id = self.db.main_method().unwrap();

        let main_class_ptr = self
            .module
            .add_global(&self.names.classes[&main_class_id])
            .as_pointer_value();

        let main_method = self
            .module
            .add_function(
                &self.names.methods[&main_method_id],
                self.context.void_type().fn_type(
                    &[self.types.context.ptr_type(space).into()],
                    false,
                ),
                None,
            )
            .as_global_value()
            .as_pointer_value();

        let main_method = builder.cast_to_pointer(main_method);
        let main_class = builder.load(main_class_ptr);

        builder.call_void(
            rt_start,
            &[runtime.into(), main_class.into(), main_method.into()],
        );

        builder
            .build_return(Some(&self.context.i32_type().const_int(0, false)));

        if let Err(err) = self.module.verify() {
            panic!("The LLVM main module is invalid:\n\n{}", err.to_string());
        }

        self.module
            .print_to_file("/tmp/main.ll")
            .expect("Failed to print the main LLVM IR");
    }

    fn methods(&self, id: ClassId) -> IntValue<'ctx> {
        self.context.i16_type().const_int(self.types.methods(id) as _, false)
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
