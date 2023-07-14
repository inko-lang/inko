use crate::llvm::constants::{CLOSURE_CALL_INDEX, DROPPER_INDEX};
use crate::llvm::context::Context;
use crate::llvm::method_hasher::MethodHasher;
use crate::mir::Mir;
use crate::state::State;
use crate::target::OperatingSystem;
use inkwell::targets::TargetData;
use inkwell::types::{
    BasicMetadataTypeEnum, BasicType, FunctionType, StructType,
};
use inkwell::AddressSpace;
use std::cmp::max;
use std::collections::HashMap;
use types::{
    ClassId, MethodId, MethodSource, BOOLEAN_ID, BYTE_ARRAY_ID, CALL_METHOD,
    CHANNEL_ID, DROPPER_METHOD, FLOAT_ID, INT_ID, NIL_ID,
};

/// The size of an object header.
const HEADER_SIZE: u32 = 16;

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

/// The minimum number of slots in a method table.
///
/// This value is used to ensure that even types with few methods have as few
/// collisions as possible.
///
/// This value _must_ be a power of two.
const METHOD_TABLE_MIN_SIZE: usize = 64;

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

pub(crate) struct MethodInfo<'ctx> {
    pub(crate) index: u16,
    pub(crate) hash: u64,
    pub(crate) collision: bool,
    pub(crate) signature: FunctionType<'ctx>,

    /// If the function returns a structure on the stack, its type is stored
    /// here.
    ///
    /// This is needed separately because the signature's return type will be
    /// `void` in this case.
    pub(crate) struct_return: Option<StructType<'ctx>>,
    pub(crate) colliding: Vec<ClassId>,
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

    /// Information about methods defined on classes, such as their signatures
    /// and hash codes.
    pub(crate) methods: HashMap<MethodId, MethodInfo<'ctx>>,

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
        let mut methods = HashMap::new();
        let header = context.struct_type(&[
            context.pointer_type().into(), // Class
            context.i8_type().into(),      // Kind
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
            context.pointer_type().into(), // true
            context.pointer_type().into(), // false
            context.pointer_type().into(), // nil
            context.pointer_type().into(), // Int class
            context.pointer_type().into(), // Float class
            context.pointer_type().into(), // String class
            context.pointer_type().into(), // Array class
            context.pointer_type().into(), // Bool class
            context.pointer_type().into(), // Nil class
            context.pointer_type().into(), // ByteArray class
            context.pointer_type().into(), // Channel class
            context.pointer_type().into(), // hash_key0
            context.pointer_type().into(), // hash_key1
        ]);

        let context_layout = context.struct_type(&[
            state_layout.ptr_type(space).into(), // State
            context.pointer_type().into(),       // Process
            context.pointer_type().into(),       // Arguments pointer
        ]);

        let method_counts_layout = context.struct_type(&[
            context.i16_type().into(), // Int
            context.i16_type().into(), // Float
            context.i16_type().into(), // String
            context.i16_type().into(), // Bool
            context.i16_type().into(), // Nil
            context.i16_type().into(), // ByteArray
            context.i16_type().into(), // Channel
        ]);

        let message_layout = context.struct_type(&[
            context.pointer_type().into(), // Function
            context.i8_type().into(),      // Length
            context.pointer_type().array_type(0).into(), // Arguments
        ]);

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

                let signature = context.pointer_type().fn_type(&args, false);

                methods.insert(
                    method,
                    MethodInfo {
                        index: 0,
                        hash,
                        signature,
                        collision: false,
                        colliding: Vec::new(),
                        struct_return: None,
                    },
                );
            }
        }

        let mut method_table_sizes = Vec::with_capacity(mir.classes.len());

        // We generate the bare structs first, that way method signatures can
        // refer to them, regardless of the order in which methods/classes are
        // defined.
        for (id, mir_class) in &mir.classes {
            // We size classes larger than actually needed in an attempt to
            // reduce collisions when performing dynamic dispatch.
            let methods_len = max(
                round_methods(mir_class.methods.len()) * METHOD_TABLE_FACTOR,
                METHOD_TABLE_MIN_SIZE,
            );

            method_table_sizes.push(methods_len);

            let name =
                format!("{}::{}", id.module(db).name(db).as_str(), id.name(db));
            let class = context.class_type(
                methods_len,
                &format!("{}::class", name),
                method,
            );
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
                BOOLEAN_ID | NIL_ID => {
                    let typ = context.opaque_struct(&name);

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
                    context.opaque_struct(&name)
                }
            };

            class_layouts.insert(*id, class);
            instance_layouts.insert(*id, instance);
        }

        let mut layouts = Self {
            empty_class: context.class_type(0, "", method),
            method,
            classes: class_layouts,
            instances: instance_layouts,
            state: state_layout,
            header,
            context: context_layout,
            method_counts: method_counts_layout,
            methods,
            message: message_layout,
        };

        let process_size = match state.config.target.os {
            OperatingSystem::Linux | OperatingSystem::Freebsd => {
                // Mutexes are smaller on Linux, resulting in a smaller process
                // size, so we have to take that into account when calculating
                // field offsets.
                112
            }
            _ => 128,
        };

        for id in mir.classes.keys() {
            // String _is_ builtin, but we still process it such that the
            // standard library can define fields for it.
            if id.is_builtin() && *id != ClassId::string() {
                continue;
            }

            let layout = layouts.instances[id];
            let kind = id.kind(db);
            let mut fields = Vec::new();

            if kind.is_extern() {
                for field in id.fields(db) {
                    let typ = context
                        .foreign_type(db, &layouts, field.value_type(db))
                        .unwrap_or_else(|| {
                            context.pointer_type().as_basic_type_enum()
                        });

                    fields.push(typ);
                }
            } else {
                fields.push(header.into());

                // For processes we need to take into account the space between
                // the header and the first field. We don't actually care about
                // that state in the generated code, so we just insert a single
                // member that covers it.
                if kind.is_async() {
                    fields.push(
                        context
                            .i8_type()
                            .array_type(process_size - HEADER_SIZE)
                            .into(),
                    );
                }

                for field in id.fields(db) {
                    let typ = context
                        .foreign_type(db, &layouts, field.value_type(db))
                        .unwrap_or_else(|| {
                            context.pointer_type().as_basic_type_enum()
                        });

                    fields.push(typ);
                }
            }

            layout.set_body(&fields, false);
        }

        // Now that all the LLVM structs are defined, we can process all
        // methods.
        for (mir_class, methods_len) in
            mir.classes.values().zip(method_table_sizes.into_iter())
        {
            let mut buckets = vec![false; methods_len];
            let max_bucket = methods_len.saturating_sub(1);

            // The slot for the dropper method has to be set first to ensure
            // other methods are never hashed into this slot, regardless of the
            // order we process them in.
            if !buckets.is_empty() {
                buckets[DROPPER_INDEX as usize] = true;
            }

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
                        DROPPER_METHOD => DROPPER_INDEX as usize,
                        CALL_METHOD => CLOSURE_CALL_INDEX as usize,
                        _ => unreachable!(),
                    }
                } else if name == DROPPER_METHOD {
                    // Droppers always go in slot 0 so we can efficiently call
                    // them even when types aren't statically known.
                    DROPPER_INDEX as usize
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
                        layouts.methods.get_mut(&orig).unwrap().collision =
                            true;

                        layouts
                            .methods
                            .get_mut(&orig)
                            .unwrap()
                            .colliding
                            .push(mir_class.id);
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

                    // For instance methods, the receiver is passed as an
                    // explicit argument before any user-defined arguments.
                    if method.is_instance_method(db) {
                        args.push(context.pointer_type().into());
                    }

                    for arg in method.arguments(db) {
                        let typ = context
                            .foreign_type(db, &layouts, arg.value_type)
                            .unwrap_or_else(|| {
                                context.pointer_type().as_basic_type_enum()
                            });

                        args.push(typ.into());
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
                    MethodInfo {
                        index: index as u16,
                        hash,
                        signature: typ,
                        collision,
                        colliding: Vec::new(),
                        struct_return: None,
                    },
                );
            }
        }

        for mod_id in mir.modules.keys() {
            for &method in mod_id.extern_methods(db).values() {
                let mut args: Vec<BasicMetadataTypeEnum> =
                    Vec::with_capacity(method.number_of_arguments(db) + 1);

                // The regular return type, and the type of the structure to
                // pass with the `sret` attribute. If `ret` is `None`, it means
                // the function returns `void`. If `sret` is `None`, it means
                // the function doesn't return a struct.
                let mut ret = None;
                let mut sret = None;

                if let Some(typ) = context.return_type(db, &layouts, method) {
                    // The C ABI mandates that structures are either passed
                    // through registers (if small enough), or using a pointer.
                    // LLVM doesn't detect when this is needed for us, so sadly
                    // we (and everybody else using LLVM) have to do this
                    // ourselves.
                    //
                    // In the future we may want/need to also handle this for
                    // Inko methods, but for now they always return pointers.
                    if typ.is_struct_type() {
                        let typ = typ.into_struct_type();

                        if target_data.get_bit_size(&typ)
                            > state.config.target.pass_struct_size()
                        {
                            args.push(
                                typ.ptr_type(AddressSpace::default()).into(),
                            );
                            sret = Some(typ);
                        } else {
                            ret = Some(typ.as_basic_type_enum());
                        }
                    } else {
                        ret = Some(typ);
                    }
                }

                for arg in method.arguments(db) {
                    let raw = arg.value_type;
                    let arg = context
                        .foreign_type(db, &layouts, raw)
                        .unwrap_or_else(|| {
                            // When an Int is expected as the argument, we read
                            // it into an i64 so it's a bit easier to pass
                            // around. This won't be needed any more when
                            // https://github.com/inko-lang/inko/issues/525 is
                            // implemented.
                            if raw.is_int(db) {
                                context.i64_type().as_basic_type_enum()
                            } else {
                                context.pointer_type().as_basic_type_enum()
                            }
                        });

                    args.push(arg.into());
                }

                let sig =
                    ret.map(|t| t.fn_type(&args, false)).unwrap_or_else(|| {
                        context.void_type().fn_type(&args, false)
                    });

                layouts.methods.insert(
                    method,
                    MethodInfo {
                        index: 0,
                        hash: 0,
                        signature: sig,
                        collision: false,
                        colliding: Vec::new(),
                        struct_return: sret,
                    },
                );
            }
        }

        layouts
    }

    pub(crate) fn methods(&self, class: ClassId) -> u32 {
        self.classes[&class]
            .get_field_type_at_index(3)
            .unwrap()
            .into_array_type()
            .len()
    }
}
