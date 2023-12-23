use crate::codegen::method_hasher::MethodHasher;
use crate::mir::Mir;
use crate::state::State;
use crate::target::OperatingSystem;
use cranelift_codegen::ir::types::{I16, I32, I64, I8};
use cranelift_codegen::ir::{Signature, Type};
use std::cmp::max;
use std::collections::HashMap;
use types::{
    ClassId, Database, MethodId, Shape, BOOL_ID, BYTE_ARRAY_ID, CALL_METHOD,
    DROPPER_METHOD, FLOAT_ID, INT_ID, NIL_ID, STRING_ID,
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

fn hash_key(db: &Database, method: MethodId, shapes: &[Shape]) -> String {
    shapes.iter().fold(method.name(db).clone(), |mut name, shape| {
        name.push_str(shape.identifier());
        name
    })
}

pub(crate) struct Field {
    /// The Cranelift type of this field's value.
    typ: Type,

    /// The byte offset relative to the start of the structure.
    offset: u32,
}

pub(crate) struct StructType {
    /// The field types and offsets.
    fields: Vec<Field>,

    /// The base offset to use when retrieving the offset of fields.
    ///
    /// In most cases this is simply zero, but for processes it's used to
    /// account for the private data that sits in between the header and the
    /// first field.
    base_offset: u32,

    /// The size in bytes.
    pub(crate) size: u32,

    /// The alignment in bytes.
    pub(crate) alignment: u32,
}

impl StructType {
    fn new() -> StructType {
        StructType { fields: Vec::new(), base_offset: 0, size: 0, alignment: 0 }
    }

    pub(crate) fn set_fields(&mut self, types: Vec<Type>) {
        // Cranelift only has primitive integer/float types, and for these the
        // alignment always equals the size, hence we can safely use
        // `Type::bits()` here.
        self.alignment = types.iter().map(|t| t.bits() / 8).max().unwrap_or(0);

        let mut offset = 0_isize;

        for typ in types {
            let size = typ.bits() as isize / 8;

            offset = (offset + (size - 1)) & -size;
            self.fields.push(Field { typ, offset: offset as u32 });
            offset += size;
        }

        self.size = offset as u32;
    }

    pub(crate) fn offset_of(&self, index: usize) -> u32 {
        self.base_offset + self.fields[index].offset
    }
}

pub(crate) struct MethodInfo {
    pub(crate) index: u16,
    pub(crate) hash: u64,
    pub(crate) collision: bool,
    pub(crate) signature: Signature,

    /// If the function returns a structure on the stack, its type is stored
    /// here.
    ///
    /// This is needed separately because the signature's return type will be
    /// `void` in this case.
    pub(crate) struct_return: Option<StructType>,
}

/// Types and layout information to expose to all modules.
pub(crate) struct Layouts {
    /// The number of methods of each class.
    pub(crate) method_counts: HashMap<ClassId, StructType>,

    /// The structure layouts for all class instances.
    pub(crate) instances: HashMap<ClassId, StructType>,

    /// Information about methods defined on classes, such as their signatures
    /// and hash codes.
    pub(crate) methods: HashMap<MethodId, MethodInfo>,
}

impl Layouts {
    pub(crate) fn new(state: &State, mir: &Mir, pointer_size: u16) -> Self {
        let db = &state.db;
        let ptr = Type::int(pointer_size).unwrap();
        let mut method_hasher = MethodHasher::new();
        let mut layouts = Layouts {
            method_counts: HashMap::new(),
            instances: HashMap::new(),
            methods: HashMap::new(),
        };

        // The size of a process without any fields.
        let process_size = match state.config.target.os {
            OperatingSystem::Linux | OperatingSystem::Freebsd => {
                // Mutexes are smaller on Linux, resulting in a smaller process
                // size, so we have to take that into account when calculating
                // field offsets.
                120
            }
            _ => 136,
        };

        for (id, mir_class) in &mir.classes {
            // We size classes larger than actually needed in an attempt to
            // reduce collisions when performing dynamic dispatch.
            let methods_len = max(
                round_methods(mir_class.instance_methods_count(db))
                    * METHOD_TABLE_FACTOR,
                METHOD_TABLE_MIN_SIZE,
            );

            let kind = id.kind(db);
            let mut fields = Vec::new();
            let mut layout = StructType::new();

            if kind.is_extern() {
                for field in id.fields(db) {
                    // TODO: actual type
                    let typ = I64;

                    fields.push(typ);
                }
            } else {
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
                layout.base_offset =
                    if kind.is_async() { process_size } else { HEADER_SIZE };

                for field in id.fields(db) {
                    // TODO: actual type
                    let typ = I64;

                    fields.push(typ);
                }
            }

            layout.set_fields(fields);
        }

        layouts
        // // We need to define the method information for trait methods, as
        // // this information is necessary when generating dynamic dispatch code.
        // //
        // // This information is defined first so we can update the `collision`
        // // flag when generating this information for method implementations.
        // for calls in mir.dynamic_calls.values() {
        //     for (method, shapes) in calls {
        //         let hash = method_hasher.hash(hash_key(db, *method, shapes));
        //         let mut args: Vec<BasicMetadataTypeEnum> = vec![
        //             state_layout.ptr_type(space).into(), // State
        //             context.pointer_type().into(),       // Process
        //             context.pointer_type().into(),       // Receiver
        //         ];
        //
        //         for arg in method.arguments(db) {
        //             args.push(
        //                 context.llvm_type(db, &layouts, arg.value_type).into(),
        //             );
        //         }
        //
        //         let signature = context
        //             .return_type(db, &layouts, *method)
        //             .map(|t| t.fn_type(&args, false))
        //             .unwrap_or_else(|| {
        //                 context.void_type().fn_type(&args, false)
        //             });
        //
        //         layouts.methods.insert(
        //             *method,
        //             MethodInfo {
        //                 index: 0,
        //                 hash,
        //                 signature,
        //                 collision: false,
        //                 struct_return: None,
        //             },
        //         );
        //     }
        // }
        //
        // // Now that all the LLVM structs are defined, we can process all
        // // methods.
        // for (mir_class, methods_len) in
        //     mir.classes.values().zip(method_table_sizes.into_iter())
        // {
        //     let mut buckets = vec![false; methods_len];
        //     let max_bucket = methods_len.saturating_sub(1);
        //
        //     // The slot for the dropper method has to be set first to ensure
        //     // other methods are never hashed into this slot, regardless of the
        //     // order we process them in.
        //     if !buckets.is_empty() {
        //         buckets[DROPPER_INDEX as usize] = true;
        //     }
        //
        //     let is_closure = mir_class.id.is_closure(db);
        //
        //     // Define the method signatures once (so we can cheaply retrieve
        //     // them whenever needed), and assign the methods to their method
        //     // table slots.
        //     for &method in &mir_class.methods {
        //         let name = method.name(db);
        //         let hash =
        //             method_hasher.hash(hash_key(db, method, method.shapes(db)));
        //
        //         let mut collision = false;
        //         let index = if is_closure {
        //             // For closures we use a fixed layout so we can call its
        //             // methods using virtual dispatch instead of dynamic
        //             // dispatch.
        //             match method.name(db).as_str() {
        //                 DROPPER_METHOD => DROPPER_INDEX as usize,
        //                 CALL_METHOD => CLOSURE_CALL_INDEX as usize,
        //                 _ => unreachable!(),
        //             }
        //         } else if name == DROPPER_METHOD {
        //             // Droppers always go in slot 0 so we can efficiently call
        //             // them even when types aren't statically known.
        //             DROPPER_INDEX as usize
        //         } else {
        //             let mut index = hash as usize & (methods_len - 1);
        //
        //             while buckets[index] {
        //                 collision = true;
        //                 index = (index + 1) & max_bucket;
        //             }
        //
        //             index
        //         };
        //
        //         buckets[index] = true;
        //
        //         // We track collisions so we can generate more optimal dynamic
        //         // dispatch code if we statically know one method never collides
        //         // with another method in the same class.
        //         if collision {
        //             if let Some(orig) = method.original_method(db) {
        //                 if let Some(calls) = mir.dynamic_calls.get(&orig) {
        //                     for (id, _) in calls {
        //                         if let Some(layout) =
        //                             layouts.methods.get_mut(id)
        //                         {
        //                             layout.collision = true;
        //                         }
        //                     }
        //                 }
        //             }
        //         }
        //
        //         let typ = if method.is_async(db) {
        //             context.void_type().fn_type(
        //                 &[context_layout.ptr_type(space).into()],
        //                 false,
        //             )
        //         } else {
        //             let mut args: Vec<BasicMetadataTypeEnum> = vec![
        //                 state_layout.ptr_type(space).into(), // State
        //                 context.pointer_type().into(),       // Process
        //             ];
        //
        //             // For instance methods, the receiver is passed as an
        //             // explicit argument before any user-defined arguments.
        //             if method.is_instance_method(db) {
        //                 args.push(
        //                     context
        //                         .llvm_type(db, &layouts, method.receiver(db))
        //                         .into(),
        //                 );
        //             }
        //
        //             for arg in method.arguments(db) {
        //                 args.push(
        //                     context
        //                         .llvm_type(db, &layouts, arg.value_type)
        //                         .into(),
        //                 );
        //             }
        //
        //             context
        //                 .return_type(db, &layouts, method)
        //                 .map(|t| t.fn_type(&args, false))
        //                 .unwrap_or_else(|| {
        //                     context.void_type().fn_type(&args, false)
        //                 })
        //         };
        //
        //         layouts.methods.insert(
        //             method,
        //             MethodInfo {
        //                 index: index as u16,
        //                 hash,
        //                 signature: typ,
        //                 collision,
        //                 struct_return: None,
        //             },
        //         );
        //     }
        // }
        //
        // for &method in mir.methods.keys().filter(|m| m.is_static(db)) {
        //     let mut args: Vec<BasicMetadataTypeEnum> = vec![
        //         state_layout.ptr_type(space).into(), // State
        //         context.pointer_type().into(),       // Process
        //     ];
        //
        //     for arg in method.arguments(db) {
        //         args.push(
        //             context.llvm_type(db, &layouts, arg.value_type).into(),
        //         );
        //     }
        //
        //     let typ = context
        //         .return_type(db, &layouts, method)
        //         .map(|t| t.fn_type(&args, false))
        //         .unwrap_or_else(|| context.void_type().fn_type(&args, false));
        //
        //     layouts.methods.insert(
        //         method,
        //         MethodInfo {
        //             index: 0,
        //             hash: 0,
        //             signature: typ,
        //             collision: false,
        //             struct_return: None,
        //         },
        //     );
        // }
        //
        // for &method in &mir.extern_methods {
        //     let mut args: Vec<BasicMetadataTypeEnum> =
        //         Vec::with_capacity(method.number_of_arguments(db) + 1);
        //
        //     // The regular return type, and the type of the structure to pass
        //     // with the `sret` attribute. If `ret` is `None`, it means the
        //     // function returns `void`. If `sret` is `None`, it means the
        //     // function doesn't return a struct.
        //     let mut ret = None;
        //     let mut sret = None;
        //
        //     if let Some(typ) = context.return_type(db, &layouts, method) {
        //         // The C ABI mandates that structures are either passed through
        //         // registers (if small enough), or using a pointer. LLVM doesn't
        //         // detect when this is needed for us, so sadly we (and everybody
        //         // else using LLVM) have to do this ourselves.
        //         //
        //         // In the future we may want/need to also handle this for Inko
        //         // methods, but for now they always return pointers.
        //         if typ.is_struct_type() {
        //             let typ = typ.into_struct_type();
        //
        //             if typ.size() > state.config.target.pass_struct_size() {
        //                 args.push(typ.ptr_type(AddressSpace::default()).into());
        //                 sret = Some(typ);
        //             } else {
        //                 ret = Some(typ.as_basic_type_enum());
        //             }
        //         } else {
        //             ret = Some(typ);
        //         }
        //     }
        //
        //     for arg in method.arguments(db) {
        //         args.push(
        //             context.llvm_type(db, &layouts, arg.value_type).into(),
        //         );
        //     }
        //
        //     let variadic = method.is_variadic(db);
        //     let sig =
        //         ret.map(|t| t.fn_type(&args, variadic)).unwrap_or_else(|| {
        //             context.void_type().fn_type(&args, variadic)
        //         });
        //
        //     layouts.methods.insert(
        //         method,
        //         MethodInfo {
        //             index: 0,
        //             hash: 0,
        //             signature: sig,
        //             collision: false,
        //             struct_return: sret,
        //         },
        //     );
        // }
        //
        // layouts
    }

    pub(crate) fn methods(&self, class: ClassId) -> u32 {
        // self.classes.get(&class).map_or(0, |c| {
        //     c.get_field_type_at_index(3).unwrap().into_array_type().len()
        // })
        0 // TODO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::types::{I16, I32, I64, I8};

    #[test]
    fn test_struct_type_size() {
        let mut val = StructType::new();

        val.set_fields(vec![I8, I8]);
        assert_eq!(val.alignment, 1);
        assert_eq!(val.size, 2);

        val.set_fields(vec![I16, I16]);
        assert_eq!(val.alignment, 2);
        assert_eq!(val.size, 4);

        val.set_fields(vec![I8, I16]);
        assert_eq!(val.alignment, 2);
        assert_eq!(val.size, 4);

        val.set_fields(vec![I32, I64]);
        assert_eq!(val.alignment, 8);
        assert_eq!(val.size, 16);

        val.set_fields(vec![I32, I32, I64]);
        assert_eq!(val.alignment, 8);
        assert_eq!(val.size, 16);

        val.set_fields(vec![I8, I16, I8, I64]);
        assert_eq!(val.alignment, 8);
        assert_eq!(val.size, 16);
    }

    #[test]
    fn test_struct_offset_of() {
        let mut val = StructType::new();

        val.set_fields(vec![I8, I16, I8, I64]);

        assert_eq!(val.offset_of(0), 0);
        assert_eq!(val.offset_of(1), 2);
        assert_eq!(val.offset_of(2), 4);
        assert_eq!(val.offset_of(3), 8);
    }
}
