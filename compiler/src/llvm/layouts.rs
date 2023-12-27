use crate::llvm::context::Context;
use crate::mir::Mir;
use crate::state::State;
use crate::target::OperatingSystem;
use inkwell::targets::TargetData;
use inkwell::types::{
    BasicMetadataTypeEnum, BasicType, FunctionType, StructType,
};
use inkwell::AddressSpace;
use std::collections::HashMap;
use types::{
    ClassId, MethodId, BOOL_ID, BYTE_ARRAY_ID, FLOAT_ID, INT_ID, NIL_ID,
    STRING_ID,
};

/// The size of an object header.
const HEADER_SIZE: u32 = 16;

pub(crate) struct Method<'ctx> {
    pub(crate) signature: FunctionType<'ctx>,

    /// If the function returns a structure on the stack, its type is stored
    /// here.
    ///
    /// This is needed separately because the signature's return type will be
    /// `void` in this case.
    pub(crate) struct_return: Option<StructType<'ctx>>,
}

/// Types and layout information to expose to all modules.
pub(crate) struct Layouts<'ctx> {
    /// The layout of an empty class.
    ///
    /// This is used for generating dynamic dispatch code, as we don't know the
    /// exact class in such cases.
    pub(crate) empty_class: StructType<'ctx>,

    /// The type to use for Inko methods (used for dynamic dispatch).
    pub(crate) method: StructType<'ctx>,

    /// All MIR classes and their corresponding structure layouts.
    pub(crate) classes: HashMap<ClassId, StructType<'ctx>>,

    /// The structure layouts for all class instances.
    pub(crate) instances: HashMap<ClassId, StructType<'ctx>>,

    /// The structure layout of the runtime's `State` type.
    pub(crate) state: StructType<'ctx>,

    /// The layout of object headers.
    pub(crate) header: StructType<'ctx>,

    /// The layout of the context type passed to async methods.
    pub(crate) context: StructType<'ctx>,

    /// The layout to use for the type that stores the built-in type method
    /// counts.
    pub(crate) method_counts: StructType<'ctx>,

    /// Type information of all the defined methods.
    pub(crate) methods: HashMap<MethodId, Method<'ctx>>,

    /// The layout of messages sent to processes.
    pub(crate) message: StructType<'ctx>,
}

impl<'ctx> Layouts<'ctx> {
    pub(crate) fn new(
        state: &State,
        mir: &Mir,
        context: &'ctx Context,
        target_data: TargetData,
    ) -> Self {
        let db = &state.db;
        let space = AddressSpace::default();
        let mut class_layouts = HashMap::new();
        let mut instance_layouts = HashMap::new();
        let header = context.struct_type(&[
            context.pointer_type().into(), // Class
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
            context.pointer_type().into(), // String class
            context.pointer_type().into(), // ByteArray class
            context.pointer_type().into(), // hash_key0
            context.pointer_type().into(), // hash_key1
            context.i32_type().into(),     // scheduler_epoch
        ]);

        let context_layout = context.struct_type(&[
            state_layout.ptr_type(space).into(), // State
            context.pointer_type().into(),       // Process
            context.pointer_type().into(),       // Arguments pointer
        ]);

        let method_counts_layout = context.struct_type(&[
            context.i16_type().into(), // String
            context.i16_type().into(), // ByteArray
        ]);

        let message_layout = context.struct_type(&[
            context.pointer_type().into(), // Function
            context.i8_type().into(),      // Length
            context.pointer_type().array_type(0).into(), // Arguments
        ]);

        // We generate the bare structs first, that way method signatures can
        // refer to them, regardless of the order in which methods/classes are
        // defined.
        for &id in mir.classes.keys() {
            let name =
                format!("{}.{}", id.module(db).name(db).as_str(), id.name(db));

            let class = context.class_type(&format!("{}.class", name), method);

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
                BOOL_ID | NIL_ID => {
                    let typ = context.opaque_struct(&name);

                    typ.set_body(&[header.into()], false);
                    typ
                }
                BYTE_ARRAY_ID => context.builtin_type(
                    &name,
                    header,
                    context.rust_vec_type().into(),
                ),
                _ => {
                    // First we forward-declare the structures, as fields
                    // may need to refer to other classes regardless of
                    // ordering.
                    context.opaque_struct(&name)
                }
            };

            class_layouts.insert(id, class);
            instance_layouts.insert(id, instance);
        }

        let mut layouts = Self {
            empty_class: context.class_type("", method),
            method,
            classes: class_layouts,
            instances: instance_layouts,
            state: state_layout,
            header,
            context: context_layout,
            method_counts: method_counts_layout,
            methods: HashMap::new(),
            message: message_layout,
        };

        let process_size = match state.config.target.os {
            OperatingSystem::Linux | OperatingSystem::Freebsd => {
                // Mutexes are smaller on Linux, resulting in a smaller process
                // size, so we have to take that into account when calculating
                // field offsets.
                120
            }
            _ => 136,
        };

        // The size of the data of a process that isn't exposed to the generated
        // code (i.e. because it involves Rust types of which the layout isn't
        // stable).
        let process_private_size = process_size - HEADER_SIZE - 4;

        for id in mir.classes.keys() {
            // String is a built-in class, but it's defined like a regular one,
            // so we _don't_ want to skip it here.
            //
            // Channel is a generic class and as such is specialized, so the
            // builtin check doesn't cover it and we process it as normal, as
            // intended.
            if id.is_builtin() && id.0 != STRING_ID {
                continue;
            }

            let layout = layouts.instances[id];
            let kind = id.kind(db);
            let mut fields = Vec::new();

            if kind.is_extern() {
                for field in id.fields(db) {
                    let typ =
                        context.llvm_type(db, &layouts, field.value_type(db));

                    fields.push(typ);
                }
            } else {
                fields.push(header.into());

                // For processes, the memory layout is as follows:
                //
                //     +--------------------------+
                //     |     header (16 bytes)    |
                //     +--------------------------+
                //     |   start epoch (4 bytes)  |
                //     +--------------------------+
                //     |  private data (N bytes)  |
                //     +--------------------------+
                //     |    user-defined fields   |
                //     +--------------------------+
                if kind.is_async() {
                    fields.push(context.i32_type().into());
                    fields.push(
                        context
                            .i8_type()
                            .array_type(process_private_size)
                            .into(),
                    );
                }

                for field in id.fields(db) {
                    let typ =
                        context.llvm_type(db, &layouts, field.value_type(db));

                    fields.push(typ);
                }
            }

            layout.set_body(&fields, false);
        }

        for calls in mir.dynamic_calls.values() {
            for (method, _) in calls {
                let mut args: Vec<BasicMetadataTypeEnum> = vec![
                    state_layout.ptr_type(space).into(), // State
                    context.pointer_type().into(),       // Process
                    context.pointer_type().into(),       // Receiver
                ];

                for arg in method.arguments(db) {
                    args.push(
                        context.llvm_type(db, &layouts, arg.value_type).into(),
                    );
                }

                let signature = context
                    .return_type(db, &layouts, *method)
                    .map(|t| t.fn_type(&args, false))
                    .unwrap_or_else(|| {
                        context.void_type().fn_type(&args, false)
                    });

                layouts
                    .methods
                    .insert(*method, Method { signature, struct_return: None });
            }
        }

        // Now that all the LLVM structs are defined, we can process all
        // methods.
        for mir_class in mir.classes.values() {
            // Define the method signatures once (so we can cheaply retrieve
            // them whenever needed), and assign the methods to their method
            // table slots.
            for &method in &mir_class.methods {
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

                    // For instance methods, the receiver is passed as an
                    // explicit argument before any user-defined arguments.
                    if method.is_instance_method(db) {
                        args.push(
                            context
                                .llvm_type(db, &layouts, method.receiver(db))
                                .into(),
                        );
                    }

                    for arg in method.arguments(db) {
                        args.push(
                            context
                                .llvm_type(db, &layouts, arg.value_type)
                                .into(),
                        );
                    }

                    context
                        .return_type(db, &layouts, method)
                        .map(|t| t.fn_type(&args, false))
                        .unwrap_or_else(|| {
                            context.void_type().fn_type(&args, false)
                        })
                };

                layouts.methods.insert(
                    method,
                    Method { signature: typ, struct_return: None },
                );
            }
        }

        for &method in mir.methods.keys().filter(|m| m.is_static(db)) {
            let mut args: Vec<BasicMetadataTypeEnum> = vec![
                state_layout.ptr_type(space).into(), // State
                context.pointer_type().into(),       // Process
            ];

            for arg in method.arguments(db) {
                args.push(
                    context.llvm_type(db, &layouts, arg.value_type).into(),
                );
            }

            let typ = context
                .return_type(db, &layouts, method)
                .map(|t| t.fn_type(&args, false))
                .unwrap_or_else(|| context.void_type().fn_type(&args, false));

            layouts
                .methods
                .insert(method, Method { signature: typ, struct_return: None });
        }

        for &method in &mir.extern_methods {
            let mut args: Vec<BasicMetadataTypeEnum> =
                Vec::with_capacity(method.number_of_arguments(db) + 1);

            // The regular return type, and the type of the structure to pass
            // with the `sret` attribute. If `ret` is `None`, it means the
            // function returns `void`. If `sret` is `None`, it means the
            // function doesn't return a struct.
            let mut ret = None;
            let mut sret = None;

            if let Some(typ) = context.return_type(db, &layouts, method) {
                // The C ABI mandates that structures are either passed through
                // registers (if small enough), or using a pointer. LLVM doesn't
                // detect when this is needed for us, so sadly we (and everybody
                // else using LLVM) have to do this ourselves.
                //
                // In the future we may want/need to also handle this for Inko
                // methods, but for now they always return pointers.
                if typ.is_struct_type() {
                    let typ = typ.into_struct_type();

                    if target_data.get_bit_size(&typ)
                        > state.config.target.pass_struct_size()
                    {
                        args.push(typ.ptr_type(AddressSpace::default()).into());
                        sret = Some(typ);
                    } else {
                        ret = Some(typ.as_basic_type_enum());
                    }
                } else {
                    ret = Some(typ);
                }
            }

            for arg in method.arguments(db) {
                args.push(
                    context.llvm_type(db, &layouts, arg.value_type).into(),
                );
            }

            let variadic = method.is_variadic(db);
            let sig =
                ret.map(|t| t.fn_type(&args, variadic)).unwrap_or_else(|| {
                    context.void_type().fn_type(&args, variadic)
                });

            layouts
                .methods
                .insert(method, Method { signature: sig, struct_return: sret });
        }

        layouts
    }
}
