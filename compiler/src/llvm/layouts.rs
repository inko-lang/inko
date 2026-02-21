use crate::llvm::context::Context;
use crate::llvm::methods::Methods;
use crate::mir::Mir;
use crate::state::State;
use crate::target::OperatingSystem;
use inkwell::targets::TargetData;
use inkwell::types::{
    BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType, StructType,
};
use std::collections::VecDeque;
use types::{
    CallConvention, Database, MethodId, TypeId, TypeRef, BOOL_ID,
    ENUM_TAG_INDEX, FLOAT_ID, INT_ID, NIL_ID, STRING_ID,
};

/// The size of an object header.
const HEADER_SIZE: u32 = 16;

struct Sized {
    map: Vec<bool>,
}

impl Sized {
    fn new(amount: usize) -> Sized {
        Sized { map: vec![false; amount] }
    }

    fn has_size(&self, db: &Database, typ: TypeRef) -> bool {
        // TypeRef::as_type_instance() returns a TypeId for a pointer, but
        // pointers have a known size, so we need to skip the logic below.
        if typ.is_pointer(db) {
            return true;
        }

        if let Some(ins) = typ.as_type_instance(db) {
            let cls = ins.instance_of();

            cls.is_heap_allocated(db) || self.map[cls.0 as usize]
        } else {
            // Everything else (e.g. borrows) uses pointers and thus have a
            // known size.
            true
        }
    }

    fn set_has_size(&mut self, id: TypeId) {
        self.map[id.0 as usize] = true;
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum ArgumentType<'ctx> {
    /// The argument should be passed as a normal value.
    Regular(BasicTypeEnum<'ctx>),

    /// The argument should be passed as a pointer.
    Pointer,

    /// The argument should be a pointer to a struct that's passed using the
    /// "byval" attribute.
    StructValue(StructType<'ctx>),

    /// The argument is the struct return argument.
    StructReturn(StructType<'ctx>),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum ReturnType<'ctx> {
    /// The function doesn't return anything.
    None,

    /// The function returns a regular value.
    Regular(BasicTypeEnum<'ctx>),

    /// The function returns a structure using the ABIs struct return
    /// convention.
    Struct(StructType<'ctx>),
}

impl<'ctx> ReturnType<'ctx> {
    pub(crate) fn is_struct(self) -> bool {
        matches!(self, ReturnType::Struct(_))
    }

    pub(crate) fn is_regular(self) -> bool {
        matches!(self, ReturnType::Regular(_))
    }
}

#[derive(Clone)]
pub(crate) struct Method<'ctx> {
    /// The calling convention to use for this method.
    pub(crate) call_convention: CallConvention,

    /// If the method is a variadic method or not.
    pub(crate) variadic: bool,

    /// The return type, if any.
    pub(crate) returns: ReturnType<'ctx>,

    /// The types of the arguments.
    pub(crate) arguments: Vec<ArgumentType<'ctx>>,
}

impl<'ctx> Method<'ctx> {
    pub(crate) fn new() -> Method<'ctx> {
        Method {
            call_convention: CallConvention::Inko,
            variadic: false,
            returns: ReturnType::None,
            arguments: Vec::new(),
        }
    }

    pub(crate) fn regular(
        state: &State,
        context: &'ctx Context,
        layouts: &Layouts<'ctx>,
        method: MethodId,
    ) -> Method<'ctx> {
        let db = &state.db;
        let ret = context.method_return_type(state, layouts, method);
        let mut args = if let ReturnType::Struct(t) = ret {
            vec![ArgumentType::StructReturn(t)]
        } else {
            Vec::new()
        };

        for &typ in method
            .is_instance(db)
            .then(|| method.receiver(db))
            .iter()
            .chain(method.argument_types(db))
        {
            args.push(context.argument_type(state, layouts, typ));
        }

        Method {
            call_convention: CallConvention::new(method.is_extern(db)),
            variadic: method.is_variadic(db),
            arguments: args,
            returns: ret,
        }
    }

    pub(crate) fn signature(
        &self,
        context: &'ctx Context,
    ) -> FunctionType<'ctx> {
        let var = self.variadic;
        let mut args: Vec<BasicMetadataTypeEnum> = Vec::new();

        for &arg in &self.arguments {
            match arg {
                ArgumentType::Regular(t) => args.push(t.into()),
                ArgumentType::StructValue(_)
                | ArgumentType::StructReturn(_)
                | ArgumentType::Pointer => {
                    args.push(context.pointer_type().into())
                }
            }
        }

        match self.returns {
            ReturnType::None | ReturnType::Struct(_) => {
                context.void_type().fn_type(&args, var)
            }
            ReturnType::Regular(t) => t.fn_type(&args, var),
        }
    }
}

/// Types and layout information to expose to all modules.
pub(crate) struct Layouts<'ctx> {
    pub(crate) target_data: &'ctx TargetData,

    /// The layout of an empty type.
    ///
    /// This is used for generating dynamic dispatch code, as we don't know the
    /// exact type in such cases.
    pub(crate) empty_type: StructType<'ctx>,

    /// The type to use for Inko methods (used for dynamic dispatch).
    pub(crate) method: StructType<'ctx>,

    /// The structure layouts for each _type_ (not their instances).
    pub(crate) types: Vec<StructType<'ctx>>,

    /// The structure layouts for all type instances.
    ///
    /// This `Vec` is indexed using `TypeId` values.
    pub(crate) instances: Vec<StructType<'ctx>>,

    /// The structure layout of the runtime's `State` type.
    pub(crate) state: StructType<'ctx>,

    /// The layout of object headers.
    pub(crate) header: StructType<'ctx>,

    /// Type information of all the defined methods.
    ///
    /// This `Vec` is indexed using `MethodId` values.
    pub(crate) methods: Vec<Method<'ctx>>,

    /// The layout of a process' stack data.
    pub(crate) process_stack_data: StructType<'ctx>,
}

impl<'ctx> Layouts<'ctx> {
    pub(crate) fn new(
        state: &State,
        mir: &Mir,
        methods: &Methods,
        context: &'ctx Context,
        target_data: &'ctx TargetData,
    ) -> Self {
        let db = &state.db;
        let empty_struct = context.struct_type(&[]);
        let num_types = db.number_of_types();

        // Instead of using a HashMap, we use a Vec that's indexed using a
        // TypeId. This works since type IDs are sequential numbers starting
        // at zero.
        //
        // This may over-allocate the number of types, depending on how many
        // are removed through optimizations, but at worst we'd waste a few KiB.
        let mut types = vec![empty_struct; num_types];
        let mut instances = vec![empty_struct; num_types];
        let header = context.struct_type(&[
            context.pointer_type().into(), // Type
            context.i32_type().into(),     // References
        ]);

        let method = context.struct_type(&[
            context.i64_type().into(),     // Hash
            context.pointer_type().into(), // Function pointer
        ]);

        // We only include the fields that we need in the compiler. This is
        // fine/safe is we only use the state type through pointers, so the
        // exact size doesn't matter.
        let state_layout = context.struct_type(&[
            context.pointer_type().into(), // hash_key0
            context.pointer_type().into(), // hash_key1
            context.i32_type().into(),     // scheduler_epoch
        ]);

        let stack_data_layout = context.struct_type(&[
            context.pointer_type().into(), // Process
            context.pointer_type().into(), // Thread
            context.i32_type().into(),     // Epoch
        ]);

        // We generate the bare structs first, that way method signatures can
        // refer to them, regardless of the order in which methods/types are
        // defined.
        for &id in mir.types.keys() {
            let instance = match id.0 {
                INT_ID | FLOAT_ID | BOOL_ID | NIL_ID => {
                    let typ = context.opaque_struct("");

                    typ.set_body(&[header.into()], false);
                    typ
                }
                _ => {
                    // First we forward-declare the structures, as fields may
                    // need to refer to other types regardless of ordering.
                    context.opaque_struct("")
                }
            };

            let idx = id.0 as usize;

            instances[idx] = instance;
            types[idx] = context.new_type(method, methods.counts[idx]);
        }

        // This may over-allocate if many methods are removed through
        // optimizations, but that's OK as in the worst case we just waste a few
        // KiB.
        let num_methods = db.number_of_methods();
        let mut layouts = Self {
            target_data,
            empty_type: context.new_type(method, 0),
            method,
            types,
            instances,
            state: state_layout,
            header,
            methods: vec![Method::new(); num_methods],
            process_stack_data: stack_data_layout,
        };

        // The order here is important: types must come first, then the dynamic
        // calls, then the methods (as those depend on the types).
        layouts.define_types(state, mir, context);
        layouts.define_dynamic_calls(state, mir, context);
        layouts.define_methods(state, mir, context);
        layouts
    }

    fn define_types(
        &mut self,
        state: &State,
        mir: &Mir,
        context: &'ctx Context,
    ) {
        let db = &state.db;
        let num_types = db.number_of_types();
        let process_size = match state.config.target.os {
            OperatingSystem::Linux | OperatingSystem::Freebsd => {
                // Mutexes are smaller on Linux, resulting in a smaller process
                // size, so we have to take that into account when calculating
                // field offsets.
                104
            }
            _ => 120,
        };

        // The size of the data of a process that isn't exposed to the generated
        // code (i.e. because it involves Rust types of which the layout isn't
        // stable).
        let process_private_size = process_size - HEADER_SIZE;

        // Type A might depend on type B, which in turn may depend on type C.
        // This means that to calculate the size of A, we'd first have to
        // calculate it of B and C. We do this using a work list approach: if
        // the size of a type can be calculated immediately we do just that,
        // otherwise we reschedule it for later (re)processing.
        let mut queue = mir.types.keys().cloned().collect::<VecDeque<_>>();
        let mut sized = Sized::new(num_types);

        // These types have a fixed size and don't define any fields. To ensure
        // the work loop terminates, we manually flag them as known.
        for id in [INT_ID, FLOAT_ID, BOOL_ID, NIL_ID] {
            sized.set_has_size(TypeId(id as _));
        }

        while let Some(id) = queue.pop_front() {
            let kind = id.kind(db);

            if id.is_builtin() && id.0 != STRING_ID {
                continue;
            }

            // If the type we're checking is _not_ a type instance then we
            // default to _true_ instead of false, since this means we can
            // trivially calculate the size of that field (e.g. it's a pointer).
            let size_known = if kind.is_enum() {
                id.constructors(db).into_iter().all(|c| {
                    c.arguments(db).iter().all(|&t| sized.has_size(db, t))
                })
            } else {
                id.fields(db)
                    .into_iter()
                    .all(|f| sized.has_size(db, f.value_type(db)))
            };

            if !size_known {
                queue.push_back(id);
                continue;
            }

            if kind.is_enum() {
                let layout = self.instances[id.0 as usize];
                let fields = id.fields(db);
                let mut types = Vec::with_capacity(fields.len() + 1);

                if id.is_heap_allocated(db) {
                    types.push(self.header.into());
                }

                // Add the type for the tag.
                let tag_typ = fields[ENUM_TAG_INDEX].value_type(db);

                types.push(context.llvm_type(db, self, tag_typ));

                // For each constructor argument we generate a field with an
                // opaque type. The size of this type must equal that of the
                // largest type.
                let mut opaque =
                    vec![
                        context.i8_type().array_type(0).as_basic_type_enum();
                        fields.len() - 1
                    ];

                for con in id.constructors(db) {
                    for (idx, &typ) in con.arguments(db).iter().enumerate() {
                        let llvm_typ = context.llvm_type(db, self, typ);
                        let size = self.target_data.get_abi_size(&llvm_typ);
                        let ex = self.target_data.get_abi_size(&opaque[idx]);

                        if size > ex {
                            opaque[idx] = context
                                .i8_type()
                                .array_type(size as _)
                                .as_basic_type_enum();
                        }
                    }
                }

                types.append(&mut opaque);
                sized.set_has_size(id);
                layout.set_body(&types, false);
                continue;
            }

            let layout = self.instances[id.0 as usize];
            let fields = id.fields(db);

            // We add 1 to account for the header that almost all types
            // processed here will have.
            let mut types = Vec::with_capacity(fields.len() + 1);

            if id.is_heap_allocated(db) {
                types.push(self.header.into());
            }

            if kind.is_async() {
                // For processes, the memory layout is as follows:
                //
                //     +--------------------------+
                //     |     header (16 bytes)    |
                //     +--------------------------+
                //     |  private data (N bytes)  |
                //     +--------------------------+
                //     |    user-defined fields   |
                //     +--------------------------+
                types.push(
                    context.i8_type().array_type(process_private_size).into(),
                );
            }

            for field in fields {
                types.push(context.llvm_type(db, self, field.value_type(db)));
            }

            layout.set_body(&types, false);
            sized.set_has_size(id);
        }
    }

    fn define_dynamic_calls(
        &mut self,
        state: &State,
        mir: &Mir,
        context: &'ctx Context,
    ) {
        for calls in mir.dynamic_calls.values() {
            for (id, _) in calls {
                self.methods[id.0 as usize] =
                    Method::regular(state, context, self, *id);
            }
        }
    }

    fn define_methods(
        &mut self,
        state: &State,
        mir: &Mir,
        context: &'ctx Context,
    ) {
        let db = &state.db;

        for mir_typ in mir.types.values() {
            for &id in &mir_typ.methods {
                self.methods[id.0 as usize] = if id.is_async(db) {
                    let args = vec![ArgumentType::Regular(
                        context.pointer_type().as_basic_type_enum(),
                    )];

                    Method {
                        call_convention: CallConvention::Inko,
                        variadic: false,
                        arguments: args,
                        returns: ReturnType::None,
                    }
                } else {
                    Method::regular(state, context, self, id)
                };
            }
        }

        for &id in mir.methods.keys().filter(|m| m.is_static(db)) {
            self.methods[id.0 as usize] =
                Method::regular(state, context, self, id);
        }

        for &id in &mir.extern_methods {
            self.methods[id.0 as usize] =
                Method::regular(state, context, self, id);
        }
    }
}
