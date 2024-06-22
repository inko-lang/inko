use crate::llvm::context::Context;
use crate::mir::Mir;
use crate::state::State;
use crate::target::OperatingSystem;
use inkwell::targets::TargetData;
use inkwell::types::{
    BasicMetadataTypeEnum, BasicType, FunctionType, StructType,
};
use inkwell::AddressSpace;
use types::{
    CallConvention, ClassId, BOOL_ID, BYTE_ARRAY_ID, FLOAT_ID, INT_ID, NIL_ID,
    STRING_ID,
};

/// The size of an object header.
const HEADER_SIZE: u32 = 16;

#[derive(Copy, Clone)]
pub(crate) struct Method<'ctx> {
    pub(crate) signature: FunctionType<'ctx>,

    /// The calling convention to use for this method.
    pub(crate) call_convention: CallConvention,

    /// If the function returns a structure on the stack, its type is stored
    /// here.
    ///
    /// This is needed separately because the signature's return type will be
    /// `void` in this case.
    pub(crate) struct_return: Option<StructType<'ctx>>,
}

/// Types and layout information to expose to all modules.
pub(crate) struct Layouts<'ctx> {
    pub(crate) target_data: &'ctx TargetData,

    /// The layout of an empty class.
    ///
    /// This is used for generating dynamic dispatch code, as we don't know the
    /// exact class in such cases.
    pub(crate) empty_class: StructType<'ctx>,

    /// The type to use for Inko methods (used for dynamic dispatch).
    pub(crate) method: StructType<'ctx>,

    /// All MIR classes and their corresponding structure layouts.
    ///
    /// This `Vec` is indexed using `ClassId` values.
    pub(crate) classes: Vec<StructType<'ctx>>,

    /// The structure layouts for all class instances.
    ///
    /// This `Vec` is indexed using `ClassId` values.
    pub(crate) instances: Vec<StructType<'ctx>>,

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
    ///
    /// This `Vec` is indexed using `MethodId` values.
    pub(crate) methods: Vec<Method<'ctx>>,

    /// The layout of messages sent to processes.
    pub(crate) message: StructType<'ctx>,

    /// The layout of a process' stack data.
    pub(crate) process_stack_data: StructType<'ctx>,
}

impl<'ctx> Layouts<'ctx> {
    pub(crate) fn new(
        state: &State,
        mir: &Mir,
        context: &'ctx Context,
        target_data: &'ctx TargetData,
    ) -> Self {
        let db = &state.db;
        let space = AddressSpace::default();
        let empty_struct = context.struct_type(&[]);
        let num_classes = db.number_of_classes();

        // Instead of using a HashMap, we use a Vec that's indexed using a
        // ClassId. This works since class IDs are sequential numbers starting
        // at zero.
        //
        // This may over-allocate the number of classes, depending on how many
        // are removed through optimizations, but at worst we'd waste a few KiB.
        let mut classes = vec![empty_struct; num_classes];
        let mut instances = vec![empty_struct; num_classes];
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
            context.pointer_type().into(), // Arguments pointer
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

        let stack_data_layout = context.struct_type(&[
            context.pointer_type().into(), // Process
            context.pointer_type().into(), // Thread
            context.i32_type().into(),     // Epoch
        ]);

        // We generate the bare structs first, that way method signatures can
        // refer to them, regardless of the order in which methods/classes are
        // defined.
        for &id in mir.classes.keys() {
            let instance = match id.0 {
                INT_ID => {
                    context.builtin_type(header, context.i64_type().into())
                }
                FLOAT_ID => {
                    context.builtin_type(header, context.f64_type().into())
                }
                BOOL_ID | NIL_ID => {
                    let typ = context.opaque_struct("");

                    typ.set_body(&[header.into()], false);
                    typ
                }
                BYTE_ARRAY_ID => {
                    context.builtin_type(header, context.rust_vec_type().into())
                }
                _ => {
                    // First we forward-declare the structures, as fields may
                    // need to refer to other classes regardless of ordering.
                    context.opaque_struct("")
                }
            };

            classes[id.0 as usize] = context.class_type(method);
            instances[id.0 as usize] = instance;
        }

        // This may over-allocate if many methods are removed through
        // optimizations, but that's OK as in the worst case we just waste a few
        // KiB.
        let num_methods = db.number_of_methods();
        let dummy_method = Method {
            call_convention: CallConvention::Inko,
            signature: context.void_type().fn_type(&[], false),
            struct_return: None,
        };

        let mut layouts = Self {
            target_data,
            empty_class: context.class_type(method),
            method,
            classes,
            instances,
            state: state_layout,
            header,
            context: context_layout,
            method_counts: method_counts_layout,
            methods: vec![dummy_method; num_methods],
            message: message_layout,
            process_stack_data: stack_data_layout,
        };

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

            let layout = layouts.instances[id.0 as usize];
            let kind = id.kind(db);
            let fields = id.fields(db);

            // We add 1 to account for the header that almost all classes
            // processed here will have.
            let mut types = Vec::with_capacity(fields.len() + 1);

            if kind.is_extern() {
                for field in fields {
                    let typ =
                        context.llvm_type(db, &layouts, field.value_type(db));

                    types.push(typ);
                }
            } else {
                types.push(header.into());

                // For processes, the memory layout is as follows:
                //
                //     +--------------------------+
                //     |     header (16 bytes)    |
                //     +--------------------------+
                //     |  private data (N bytes)  |
                //     +--------------------------+
                //     |    user-defined fields   |
                //     +--------------------------+
                if kind.is_async() {
                    types.push(
                        context
                            .i8_type()
                            .array_type(process_private_size)
                            .into(),
                    );
                }

                for field in fields {
                    let typ =
                        context.llvm_type(db, &layouts, field.value_type(db));

                    types.push(typ);
                }
            }

            layout.set_body(&types, false);
        }

        for calls in mir.dynamic_calls.values() {
            for (method, _) in calls {
                let mut args: Vec<BasicMetadataTypeEnum> = vec![
                    context.pointer_type().into(), // Receiver
                ];

                for &typ in method.argument_types(db) {
                    args.push(context.llvm_type(db, &layouts, typ).into());
                }

                let signature = context
                    .return_type(db, &layouts, *method)
                    .map(|t| t.fn_type(&args, false))
                    .unwrap_or_else(|| {
                        context.void_type().fn_type(&args, false)
                    });

                layouts.methods[method.0 as usize] = Method {
                    call_convention: CallConvention::new(method.is_extern(db)),
                    signature,
                    struct_return: None,
                };
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
                    let mut args: Vec<BasicMetadataTypeEnum> = Vec::new();

                    // For instance methods, the receiver is passed as an
                    // explicit argument before any user-defined arguments.
                    if method.is_instance(db) {
                        args.push(
                            context
                                .llvm_type(db, &layouts, method.receiver(db))
                                .into(),
                        );
                    }

                    for &typ in method.argument_types(db) {
                        args.push(context.llvm_type(db, &layouts, typ).into());
                    }

                    context
                        .return_type(db, &layouts, method)
                        .map(|t| t.fn_type(&args, false))
                        .unwrap_or_else(|| {
                            context.void_type().fn_type(&args, false)
                        })
                };

                layouts.methods[method.0 as usize] = Method {
                    call_convention: CallConvention::new(method.is_extern(db)),
                    signature: typ,
                    struct_return: None,
                };
            }
        }

        for &method in mir.methods.keys().filter(|m| m.is_static(db)) {
            let mut args: Vec<BasicMetadataTypeEnum> = Vec::new();

            for &typ in method.argument_types(db) {
                args.push(context.llvm_type(db, &layouts, typ).into());
            }

            let typ = context
                .return_type(db, &layouts, method)
                .map(|t| t.fn_type(&args, false))
                .unwrap_or_else(|| context.void_type().fn_type(&args, false));

            layouts.methods[method.0 as usize] = Method {
                call_convention: CallConvention::new(method.is_extern(db)),
                signature: typ,
                struct_return: None,
            };
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

            for &typ in method.argument_types(db) {
                args.push(context.llvm_type(db, &layouts, typ).into());
            }

            let variadic = method.is_variadic(db);
            let sig =
                ret.map(|t| t.fn_type(&args, variadic)).unwrap_or_else(|| {
                    context.void_type().fn_type(&args, variadic)
                });

            layouts.methods[method.0 as usize] = Method {
                call_convention: CallConvention::C,
                signature: sig,
                struct_return: sret,
            };
        }

        layouts
    }

    pub(crate) fn size_of_class(&self, class: ClassId) -> u64 {
        let layout = &self.instances[class.0 as usize];

        self.target_data.get_bit_size(layout)
            / (self.target_data.get_pointer_byte_size(None) as u64)
    }
}
