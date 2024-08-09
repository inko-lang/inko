use crate::llvm::constants::{CLOSURE_CALL_INDEX, DROPPER_INDEX};
use crate::llvm::method_hasher::MethodHasher;
use crate::mir::Mir;
use std::cmp::max;
use std::fmt::Write as _;
use types::{Database, MethodId, Shape, CALL_METHOD, DROPPER_METHOD};

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
    let mut key = method.name(db).clone();

    for shape in shapes {
        let _ = write!(key, "{}", shape);
    }

    key
}

#[derive(Copy, Clone)]
pub(crate) struct Method {
    /// The index of this method in the owning class' method table.
    pub(crate) index: u16,

    /// The hash code to use for dynamic dispatch.
    pub(crate) hash: u64,

    /// A flag that indicates this method has the same hash code as another
    /// method, and that probing is necessary when performing dynamic dispatch.
    pub(crate) collision: bool,
}

pub(crate) struct Methods {
    /// All methods along with their details such as their indexes and hash
    /// codes.
    ///
    /// This `Vec` is indexed using `MethodId` values.
    pub(crate) info: Vec<Method>,

    /// The number of method slots for each class.
    ///
    /// This `Vec` is indexed using `ClassId` values.
    pub(crate) counts: Vec<usize>,
}

impl Methods {
    pub(crate) fn new(db: &Database, mir: &Mir) -> Methods {
        let dummy_method = Method { index: 0, hash: 0, collision: false };
        let mut info = vec![dummy_method; db.number_of_methods()];
        let mut counts = vec![0; db.number_of_classes()];
        let mut method_hasher = MethodHasher::new();

        // This information is defined first so we can update the `collision`
        // flag when generating this information for method implementations.
        for calls in mir.dynamic_calls.values() {
            for (method, shapes) in calls {
                let hash = method_hasher.hash(hash_key(db, *method, shapes));

                info[method.0 as usize] =
                    Method { index: 0, hash, collision: false };
            }
        }

        // `mir.classes` is a HashMap, and the order of iterating over a HashMap
        // isn't consistent. Should there be conflicting hashes, the order in
        // which classes (and thus methods) are processed may affect the hash
        // code. By sorting the list of IDs first and iterating over that, we
        // ensure we always process the data in a consistent order.
        let mut ids = mir.classes.keys().cloned().collect::<Vec<_>>();

        ids.sort_by_key(|i| i.name(db));

        for id in ids {
            let mir_class = &mir.classes[&id];

            // We size classes larger than actually needed in an attempt to
            // reduce collisions when performing dynamic dispatch.
            let methods_len = max(
                round_methods(mir_class.instance_methods_count(db))
                    * METHOD_TABLE_FACTOR,
                METHOD_TABLE_MIN_SIZE,
            );

            counts[id.0 as usize] = methods_len;

            let mut buckets = vec![false; methods_len];
            let max_bucket = methods_len.saturating_sub(1);

            // The slot for the dropper method has to be set first to ensure
            // other methods are never hashed into this slot, regardless of the
            // order we process them in.
            if !buckets.is_empty() {
                buckets[DROPPER_INDEX as usize] = true;
            }

            let is_closure = mir_class.id.is_closure(db);

            // Define the method signatures once (so we can cheaply retrieve
            // them whenever needed), and assign the methods to their method
            // table slots.
            for &method in &mir_class.methods {
                let name = method.name(db);
                let hash =
                    method_hasher.hash(hash_key(db, method, method.shapes(db)));

                let mut collision = false;
                let index = if is_closure {
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
                    if let Some(orig) = method.original_method(db) {
                        if let Some(calls) = mir.dynamic_calls.get(&orig) {
                            for (id, _) in calls {
                                info[id.0 as usize].collision = true;
                            }
                        }
                    }
                }

                info[method.0 as usize] =
                    Method { index: index as u16, hash, collision };
            }
        }

        Methods { info, counts }
    }
}
