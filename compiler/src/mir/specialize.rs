use crate::mir::{
    Block, BlockId, Borrow, CallDynamic, CallInstance, CastType, Drop,
    Instruction, InstructionLocation, Method, Mir, RegisterId, Type as MirType,
};
use crate::state::State;
use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::swap;
use types::check::TypeChecker;
use types::specialize::{ordered_shapes_from_map, TypeSpecializer};
use types::{
    Block as _, Database, InternedTypeArguments, MethodId, Shape,
    TypeArguments, TypeId, TypeInstance, TypeParameterId, TypeRef, CALL_METHOD,
    DECREMENT_METHOD, DROPPER_METHOD, INCREMENT_METHOD,
};

fn argument_shape(
    db: &Database,
    interned: &mut InternedTypeArguments,
    shapes: &HashMap<TypeParameterId, Shape>,
    arguments: &TypeArguments,
    parameter: TypeParameterId,
) -> Shape {
    arguments.get_recursive(db, parameter).unwrap().shape(db, interned, shapes)
}

fn specialize_constants(
    db: &mut Database,
    mir: &mut Mir,
    interned: &mut InternedTypeArguments,
) {
    let mut types = Vec::new();
    let shapes = HashMap::new();

    // Constants never need access to the self type, so we just use a dummy
    // value here.
    let stype = TypeInstance::new(TypeId::nil());

    for &id in mir.constants.keys() {
        let old_typ = id.value_type(db);
        let new_typ =
            TypeSpecializer::new(db, interned, &shapes, &mut types, stype)
                .specialize(old_typ);

        id.set_value_type(db, new_typ);
    }

    for typ in types {
        mir.types.insert(typ, MirType::new(typ));

        let mod_id = typ.module(db);

        mir.modules.get_mut(&mod_id).unwrap().types.push(typ);
    }
}

/// Returns `true` if the given shapes are compatible with the method bounds, if
/// there are any.
///
/// It's possible to trigger method specialization for types such as
/// `Result[Int32, String]`. Since foreign types don't implement traits, don't
/// have headers and thus don't support dynamic dispatch, we have to skip
/// generating methods for such cases, otherwise we may generate incorrect code.
fn shapes_compatible_with_bounds(
    db: &Database,
    method: MethodId,
    shapes: &HashMap<TypeParameterId, Shape>,
) -> bool {
    let bounds = method.bounds(db);

    for (&param, &shape) in shapes {
        if let Some(bound) = bounds.get(param) {
            // Foreign types don't support traits, so these are never
            // compatible.
            if shape.is_foreign() {
                return false;
            }

            // When encountering a shape for a specific type, we'll end up
            // trying to devirtualize calls in the method to specialize. This is
            // only possible if the type is compatible with the bounds, i.e. all
            // the required traits are implemented.
            //
            // This ultimately ensures that we don't try to specialize some
            // method over e.g. `Option[Int32]` where it's exected `Int32`
            // implements a certain trait when it doesn't.
            let Some(ins) = shape.as_stack_instance() else { continue };

            if !TypeChecker::new(db).type_compatible_with_bound(ins, bound) {
                return false;
            }
        }
    }

    true
}

struct Job {
    /// The type of `self` within the method.
    self_type: TypeInstance,

    /// The ID of the method that's being specialized.
    method: MethodId,

    /// The shapes of the method (including its receiver), in the same order as
    /// the type parameters.
    shapes: HashMap<TypeParameterId, Shape>,
}

struct Work {
    jobs: VecDeque<Job>,

    /// The methods that have been processed by crawling through the program's
    /// code (starting at the entry method).
    ///
    /// This is used to prevent processing the same method multiple times.
    done: HashSet<MethodId>,
}

impl Work {
    fn new() -> Work {
        Work { jobs: VecDeque::new(), done: HashSet::new() }
    }

    fn push(
        &mut self,
        self_type: TypeInstance,
        method: MethodId,
        shapes: HashMap<TypeParameterId, Shape>,
    ) -> bool {
        if self.done.insert(method) {
            self.jobs.push_back(Job { self_type, method, shapes });
            true
        } else {
            false
        }
    }

    fn pop(&mut self) -> Option<Job> {
        self.jobs.pop_front()
    }
}

#[derive(Eq, PartialEq, Hash)]
struct DynamicCall {
    method: MethodId,

    /// The shapes for the method's type parameters.
    ///
    /// This also acts as the specialization key.
    key: Vec<Shape>,

    /// Extra shapes to expose to the method.
    ///
    /// These are shapes from the type arguments of a trait implementation, type
    /// parameters inherited from parent traits.
    ///
    /// We use a Vec here as `HashMap` doesn't implement `Hash`.
    shapes: Vec<(TypeParameterId, Shape)>,
}

/// A type that tracks types along with their methods that are called using
/// dynamic dispatch, and the shapes of those calls.
///
/// When specializing types and methods, we may encounter a dynamic dispatch
/// call site before specialized types are created. An example would be this:
///
///     import std.string.ToString
///
///     impl ToString for Array {
///       fn pub to_string -> String {
///         ''
///       }
///     }
///
///     fn to_string(value: ref ToString) -> String {
///       value.to_string
///     }
///
///     type async Main {
///       fn async main {
///         to_string([10, 20])
///         [10.2]
///       }
///     }
///
/// Here `to_string` isn't generic, but `value` may be passed generic values
/// (e.g. `Array[Int]`). Because this method isn't generic, and the ownership of
/// `value` is `ref T`, there's only one version of `to_string` for all
/// references passed to it (`ref Array[Int]`, `ref Array[String]`, etc). This
/// means that when we encounter `[10.2]` and specialize its type, we also have
/// to specialize `Array.to_string` for `Array[Float]`, because it _might_ be
/// passed to `to_string` at some later point.
///
/// When such new types are created, we use this data to figure out which
/// methods may be called on it through dynamic dispatch, and schedule them for
/// specialization if necessary.
struct DynamicCalls {
    /// The values are an _ordered_ hash set to ensure the data is always
    /// processed in a deterministic order. This is important in order to
    /// maintain the incremental compilation caches.
    mapping: HashMap<TypeId, IndexSet<DynamicCall>>,
}

impl DynamicCalls {
    fn new() -> DynamicCalls {
        DynamicCalls { mapping: HashMap::new() }
    }

    fn add(
        &mut self,
        type_id: TypeId,
        method: MethodId,
        key: Vec<Shape>,
        shapes: Vec<(TypeParameterId, Shape)>,
    ) {
        self.mapping.entry(type_id).or_default().insert(DynamicCall {
            method,
            key,
            shapes,
        });
    }

    fn get(&self, type_id: TypeId) -> Option<&IndexSet<DynamicCall>> {
        self.mapping.get(&type_id)
    }
}

/// A compiler pass that specializes generic types.
pub(crate) struct Specialize<'a, 'b> {
    self_type: TypeInstance,
    method: MethodId,
    state: &'a mut State,
    work: &'b mut Work,
    interned: &'b mut InternedTypeArguments,
    shapes: HashMap<TypeParameterId, Shape>,

    /// Regular methods that have been processed.
    regular_methods: Vec<MethodId>,

    /// Method specializations created while processing the body of the method.
    ///
    /// The tuple stores the following:
    ///
    /// 1. The ID of the original/old method the specialization is based on.
    /// 1. The ID of the newly specialized method.
    specialized_methods: Vec<(MethodId, MethodId)>,

    /// Classes created when specializing types.
    ///
    /// These types are tracked so we can generate their droppers, and
    /// specialize any implemented trait methods that are called through dynamic
    /// dispatch.
    types: Vec<TypeId>,
}

impl<'a, 'b> Specialize<'a, 'b> {
    pub(crate) fn run_all(state: &'a mut State, mir: &'a mut Mir) {
        // As part of specialization we create specializations for generics, and
        // discover all the types that are in use. This ensures that once we're
        // done, anything that we didn't encounter is removed, ensuring future
        // passes don't operate on unused types and methods.
        for typ in mir.types.values_mut() {
            typ.methods.clear();
        }

        for module in mir.modules.values_mut() {
            module.types.clear();
            module.methods.clear();
        }

        let mut work = Work::new();
        let mut dcalls = DynamicCalls::new();
        let mut intern = InternedTypeArguments::new();
        let main_type = state.db.main_type().unwrap();
        let main_method = state.db.main_method().unwrap();
        let main_mod = main_type.module(&state.db);

        work.push(TypeInstance::new(main_type), main_method, HashMap::new());

        // The main() method isn't called explicitly, so we have to manually
        // record it in the main type.
        mir.types.get_mut(&main_type).unwrap().methods.push(main_method);
        mir.modules.get_mut(&main_mod).unwrap().methods.push(main_method);

        while let Some(job) = work.pop() {
            Specialize {
                state,
                interned: &mut intern,
                self_type: job.self_type,
                method: job.method,
                shapes: job.shapes,
                work: &mut work,
                regular_methods: Vec::new(),
                specialized_methods: Vec::new(),
                types: Vec::new(),
            }
            .run(mir, &mut dcalls);
        }

        // Constants may contain arrays, so we need to make sure those use the
        // correct types.
        //
        // This is done _after_ processing methods. This way we don't need to
        // handle generating droppers for constants, because if we encounter
        // e.g. `Array[Int]` then one of the following is true:
        //
        // 1. The type is used elsewhere, and thus is already specialized and a
        //    dropper is already generated.
        // 2. The type isn't used anywhere else (highly unlikely). In this case
        //    we don't need to generate a dropper, because constants are never
        //    dropped.
        specialize_constants(&mut state.db, mir, &mut intern);

        // Specialization may create many new methods, and in the process makes
        // the original generic methods redundant and unused. In fact, compiling
        // them as-is could result in incorrect code being generated. As such,
        // we end up removing all methods we haven't processed (i.e they're
        // unused).
        let mut old = IndexMap::new();

        swap(&mut mir.methods, &mut old);

        for method in old.into_values() {
            if work.done.contains(&method.id) {
                mir.methods.insert(method.id, method);
            }
        }

        // The specialization source is also set for regular types that we
        // encounter. Thus, this filters out any types that we don't encounter
        // anywhere; generic or not.
        mir.types.retain(|id, _| id.specialization_source(&state.db).is_some());

        // We don't need the type arguments after this point.
        mir.type_arguments = Vec::new();
    }

    fn run(&mut self, mir: &mut Mir, dynamic_calls: &mut DynamicCalls) {
        self.process_instructions(mir, dynamic_calls);
        self.process_specialized_types(mir, dynamic_calls);
        self.expand_instructions(mir);
        self.add_methods(mir);
    }

    fn process_instructions(
        &mut self,
        mir: &mut Mir,
        dynamic_calls: &mut DynamicCalls,
    ) {
        let method = mir.methods.get_mut(&self.method).unwrap();

        // Rather than specializing the registers of instructions that may
        // produce generic types, we just specialize all of them. The type
        // specializer bails out if this isn't needed anyway, and this makes our
        // code not prone to accidentally forgetting to specialize a register
        // when adding or changing MIR instructions.
        for reg in method.registers.iter_mut() {
            reg.value_type = TypeSpecializer::new(
                &mut self.state.db,
                self.interned,
                &self.shapes,
                &mut self.types,
                self.self_type,
            )
            .specialize(reg.value_type);
        }

        for block in &mut method.body.blocks {
            for instruction in &mut block.instructions {
                match instruction {
                    Instruction::Borrow(ins) => {
                        let src = method.registers.value_type(ins.value);
                        let reg = method.registers.value_type(ins.register);
                        let db = &self.state.db;

                        method.registers.get_mut(ins.register).value_type =
                            if reg.is_ref(db) {
                                src.as_ref(db)
                            } else if reg.is_mut(db) {
                                src.force_as_mut(db)
                            } else {
                                src
                            };
                    }
                    Instruction::CallExtern(ins) => {
                        mir.extern_methods.insert(ins.method);
                    }
                    Instruction::CallStatic(ins) => {
                        let rec = ins.method.receiver(&self.state.db);
                        let cls = rec.type_id(&self.state.db).unwrap();
                        let targs = ins
                            .type_arguments
                            .and_then(|i| mir.type_arguments.get(i));

                        ins.method = self.call_static(cls, ins.method, targs);
                    }
                    Instruction::CallInstance(ins) => {
                        let rec = method.registers.value_type(ins.receiver);
                        let cls = rec.type_id(&self.state.db).unwrap();
                        let targs = ins
                            .type_arguments
                            .and_then(|i| mir.type_arguments.get(i));

                        ins.method = self.call_static(cls, ins.method, targs);
                    }
                    Instruction::Send(ins) => {
                        let rec = method.registers.value_type(ins.receiver);
                        let cls = rec.type_id(&self.state.db).unwrap();
                        let targs = ins
                            .type_arguments
                            .and_then(|i| mir.type_arguments.get(i));

                        ins.method = self.call_static(cls, ins.method, targs);
                    }
                    Instruction::CallDynamic(call) => match method
                        .registers
                        .value_type(call.receiver)
                        .as_type_instance(&self.state.db)
                    {
                        // As part of specialization, we may encounter a dynamic
                        // call that's now acting on a type instance. We need
                        // to change the instruction in this case, otherwise we
                        // may compile code that performs dynamic dispatch on
                        // unboxed values (e.g. Int), which isn't supported.
                        // This is similar to devirtualization, except it's
                        // _required_ for correct code; not an optional
                        // optimization.
                        Some(ins) => {
                            let targs = call
                                .type_arguments
                                .and_then(|i| mir.type_arguments.get(i));

                            *instruction = self
                                .devirtualize_call_dynamic(call, ins, targs);
                        }
                        _ => {
                            let targs = call
                                .type_arguments
                                .and_then(|i| mir.type_arguments.get(i));

                            let (method, shapes) = self.call_dynamic(
                                call.method,
                                targs,
                                dynamic_calls,
                            );

                            mir.dynamic_calls
                                .entry(call.method)
                                .or_default()
                                .insert((method, shapes));

                            call.method = method;
                        }
                    },
                    Instruction::Allocate(ins) => {
                        let old = ins.type_id;
                        let new = method
                            .registers
                            .value_type(ins.register)
                            .type_id(&self.state.db)
                            .unwrap();

                        ins.type_id = new;
                        self.schedule_regular_dropper(old, new);
                        self.schedule_regular_inline_type_methods(new);
                    }
                    Instruction::Free(ins) => {
                        let cls = method
                            .registers
                            .value_type(ins.register)
                            .type_id(&self.state.db)
                            .unwrap();

                        ins.type_id = cls;
                    }
                    Instruction::Spawn(ins) => {
                        let old = ins.type_id;
                        let new = method
                            .registers
                            .value_type(ins.register)
                            .type_id(&self.state.db)
                            .unwrap();

                        ins.type_id = new;
                        self.schedule_regular_dropper(old, new);
                    }
                    Instruction::SetField(ins) => {
                        let db = &mut self.state.db;

                        ins.type_id = method
                            .registers
                            .value_type(ins.receiver)
                            .type_id(db)
                            .unwrap();

                        ins.field = ins
                            .type_id
                            .field_by_index(db, ins.field.index(db))
                            .unwrap();
                    }
                    Instruction::GetField(ins) => {
                        let db = &mut self.state.db;

                        ins.type_id = method
                            .registers
                            .value_type(ins.receiver)
                            .type_id(db)
                            .unwrap();

                        ins.field = ins
                            .type_id
                            .field_by_index(db, ins.field.index(db))
                            .unwrap();
                    }
                    Instruction::FieldPointer(ins) => {
                        let db = &mut self.state.db;

                        ins.type_id = method
                            .registers
                            .value_type(ins.receiver)
                            .type_id(db)
                            .unwrap();

                        ins.field = ins
                            .type_id
                            .field_by_index(db, ins.field.index(db))
                            .unwrap();
                    }
                    Instruction::MethodPointer(ins) => {
                        let rec = ins.method.receiver(&self.state.db);
                        let cls = rec.type_id(&self.state.db).unwrap();

                        ins.method = self.call_static(cls, ins.method, None);
                    }
                    Instruction::Cast(ins) => {
                        let from = method.registers.value_type(ins.source);
                        let to = method.registers.value_type(ins.register);

                        // As a result of specialization we may need to change
                        // the cast types, such as when a type parameter is
                        // specialized as an Int.
                        ins.from = CastType::from(&self.state.db, from);
                        ins.to = CastType::from(&self.state.db, to);
                    }
                    Instruction::SizeOf(ins) => {
                        ins.argument = TypeSpecializer::new(
                            &mut self.state.db,
                            self.interned,
                            &self.shapes,
                            &mut self.types,
                            self.self_type,
                        )
                        .specialize(ins.argument);
                    }
                    _ => {}
                }
            }
        }
    }

    fn expand_instructions(&mut self, mir: &mut Mir) {
        let method = mir.methods.get_mut(&self.method).unwrap();

        ExpandDrop {
            db: &self.state.db,
            intern: self.interned,
            method,
            shapes: &self.shapes,
        }
        .run();

        ExpandBorrow {
            db: &self.state.db,
            intern: self.interned,
            method,
            shapes: &self.shapes,
        }
        .run();
    }

    fn process_specialized_types(
        &mut self,
        mir: &mut Mir,
        dynamic_calls: &mut DynamicCalls,
    ) {
        while let Some(typ) = self.types.pop() {
            mir.types.entry(typ).or_insert_with(|| MirType::new(typ));

            let mod_id = typ.module(&self.state.db);
            let module = mir.modules.get_mut(&mod_id).unwrap();
            let kind = typ.kind(&self.state.db);

            module.types.push(typ);

            if kind.is_extern() {
                // We don't generate methods for extern types, nor can they be
                // used for receivers as method calls.
                continue;
            }

            // New types are only added for types to specialize, so the source
            // is always set at this point.
            let orig = typ.specialization_source(&self.state.db).unwrap();

            self.generate_dropper(orig, typ);
            self.generate_inline_type_methods(orig, typ);

            if orig == typ {
                // For regular types the rest of the work doesn't apply.
                continue;
            }

            if kind.is_closure() {
                self.generate_closure_methods(orig, typ);
            }

            if let Some(calls) = dynamic_calls.get(orig) {
                let mut type_shapes = HashMap::new();

                for (param, &shape) in typ
                    .type_parameters(&self.state.db)
                    .into_iter()
                    .zip(typ.shapes(&self.state.db))
                {
                    type_shapes.insert(param, shape);
                }

                for call in calls {
                    let mut shapes = type_shapes.clone();

                    for &(par, shape) in &call.shapes {
                        shapes.insert(par, shape);
                    }

                    for (par, &shape) in call
                        .method
                        .type_parameters(&self.state.db)
                        .into_iter()
                        .zip(&call.key)
                    {
                        shapes.insert(par, shape);
                    }

                    if !shapes_compatible_with_bounds(
                        &self.state.db,
                        call.method,
                        &shapes,
                    ) {
                        continue;
                    }

                    self.add_implementation_shapes(call.method, &mut shapes);
                    self.add_method_bound_shapes(call.method, &mut shapes);
                    self.specialize_method(typ, call.method, &shapes, None);
                }
            }
        }
    }

    fn add_methods(&mut self, mir: &mut Mir) {
        for &method in &self.regular_methods {
            self.track_method(method, mir);
        }

        for &(old, new) in &self.specialized_methods {
            let mut method = mir.methods.get(&old).unwrap().clone();

            method.id = new;
            mir.methods.insert(new, method);
            self.track_method(new, mir);
        }
    }

    fn track_method(&self, method: MethodId, mir: &mut Mir) {
        let typ =
            method.receiver(&self.state.db).type_id(&self.state.db).unwrap();

        mir.modules
            .get_mut(&typ.module(&self.state.db))
            .unwrap()
            .methods
            .push(method);

        // Static methods aren't tracked in any types, so we can skip the rest.
        if method.is_instance(&self.state.db) {
            mir.types.get_mut(&typ).unwrap().methods.push(method);
        }
    }

    fn call_static(
        &mut self,
        type_id: TypeId,
        method: MethodId,
        type_arguments: Option<&TypeArguments>,
    ) -> MethodId {
        let mut shapes = type_arguments
            .map(|args| self.type_argument_shapes(method, args))
            .unwrap_or_default();

        self.add_implementation_shapes(method, &mut shapes);

        // When specializing types, we generate dropper methods. These methods
        // call the type's drop method if it exists. Because those calls are
        // generated, they won't have any `TypeArguments` to expose to the
        // instruction. As such, we have to handle such calls explicitly.
        if shapes.is_empty() && type_id.is_generic(&self.state.db) {
            for (par, &shape) in type_id
                .type_parameters(&self.state.db)
                .into_iter()
                .zip(type_id.shapes(&self.state.db))
            {
                shapes.insert(par, shape);
            }
        }

        self.specialize_method(type_id, method, &shapes, None)
    }

    fn call_dynamic(
        &mut self,
        method: MethodId,
        type_arguments: Option<&TypeArguments>,
        dynamic_calls: &mut DynamicCalls,
    ) -> (MethodId, Vec<Shape>) {
        let trait_id = method
            .receiver(&self.state.db)
            .as_trait_instance(&self.state.db)
            .unwrap()
            .instance_of();

        let mut base_shapes = type_arguments
            .map(|args| self.type_argument_shapes(method, args))
            .unwrap_or_default();

        for shape in base_shapes.values_mut() {
            TypeSpecializer::specialize_shape(
                &mut self.state.db,
                self.interned,
                &self.shapes,
                &mut self.types,
                self.self_type,
                shape,
            );
        }

        let mut method_params = HashSet::new();
        let mut method_shapes = Vec::new();

        for par in method.type_parameters(&self.state.db) {
            method_params.insert(par);
            method_shapes.push(*base_shapes.get(&par).unwrap());
        }

        // These are the shapes of the trait or any parent traits. We filter out
        // method parameter shapes because we derive those from the method key.
        let extra_shapes: Vec<_> = base_shapes
            .iter()
            .filter(|(k, _)| !method_params.contains(*k))
            .map(|(&k, &v)| (k, v))
            .collect();

        for typ in trait_id.implemented_by(&self.state.db).clone() {
            let method_impl = typ
                .method(&self.state.db, method.name(&self.state.db))
                .unwrap();

            dynamic_calls.add(
                typ,
                method_impl,
                method_shapes.clone(),
                extra_shapes.clone(),
            );

            // We need to include the base shapes mapping, as that includes the
            // shapes for the trait's type parameters, which may be used in e.g.
            // the method's return type.
            let mut shapes = base_shapes.clone();

            // The parameters of the implementation aren't the exact same (as
            // in, the same memory location), so we need to map shapes based on
            // the order of type parameters.
            for (src_par, target_par) in method
                .type_parameters(&self.state.db)
                .into_iter()
                .zip(method_impl.type_parameters(&self.state.db))
            {
                shapes.insert(target_par, *base_shapes.get(&src_par).unwrap());
            }

            if typ.is_generic(&self.state.db) {
                let params = typ.type_parameters(&self.state.db);

                // We need/want to ensure that each specialization has its own
                // set of shapes. Even if this isn't technically required since
                // we always overwrite the same parameters with new shapes (or
                // at least should), this reduces the chances of running into
                // unexpected bugs.
                let mut shapes = shapes.clone();

                for (key, typ) in typ.specializations(&self.state.db).clone() {
                    // A dynamic call won't include shapes/type arguments for
                    // type parameters of the specialized type, so we have to
                    // inject those here.
                    //
                    // We don't need to clone `shapes` here because the type
                    // parameters are the same for every specialization of the
                    // base type, so we don't accidentally end up using the
                    // wrong shape on a future iteration of the surrounding
                    // loop.
                    for (&param, shape) in params.iter().zip(key.shapes) {
                        shapes.insert(param, shape);
                    }

                    if !shapes_compatible_with_bounds(
                        &self.state.db,
                        method_impl,
                        &shapes,
                    ) {
                        continue;
                    }

                    // We have to repeat these two calls for every specialized
                    // type, because the shapes referred to through bounds or
                    // type arguments may differ per specialization.
                    self.add_implementation_shapes(method_impl, &mut shapes);
                    self.add_method_bound_shapes(method_impl, &mut shapes);
                    self.specialize_method(typ, method_impl, &shapes, None);
                }
            } else {
                self.add_implementation_shapes(method_impl, &mut shapes);
                self.add_method_bound_shapes(method_impl, &mut shapes);
                self.specialize_method(typ, method_impl, &shapes, None);
            }
        }

        // Most dynamically dispatched methods are likely not generic, meaning
        // that the list of shapes for the method parameters is empty. We want
        // to reuse previously specialized method types for the same receivers,
        // so we need to generate a key based on all type arguments present.
        // This ensures that multiple call sites of `next` on `Iter[Int]`
        // produce the same method ID, while call sites of `next` on
        // `Iter[String]` produce a different method ID.
        let mut spec_key = ordered_shapes_from_map(&base_shapes);

        self.prepare_key(&mut spec_key);

        if let Some(new) = method.specialization(&self.state.db, &spec_key) {
            return (new, method_shapes);
        }

        let new_method = self.specialize_method_type(
            method.receiver(&self.state.db),
            method,
            spec_key,
            &base_shapes,
        );

        (new_method, method_shapes)
    }

    fn devirtualize_call_dynamic(
        &mut self,
        call: &CallDynamic,
        receiver: TypeInstance,
        type_arguments: Option<&TypeArguments>,
    ) -> Instruction {
        let typ = receiver.instance_of();
        let method_impl = typ
            .specialization_source(&self.state.db)
            .unwrap_or(typ)
            .method(&self.state.db, call.method.name(&self.state.db))
            .unwrap();

        let mut shapes = type_arguments
            .map(|args| self.type_argument_shapes(call.method, args))
            .unwrap_or_default();

        for (param, &shape) in typ
            .type_parameters(&self.state.db)
            .into_iter()
            .zip(typ.shapes(&self.state.db))
        {
            shapes.insert(param, shape);
        }

        for (src_par, target_par) in call
            .method
            .type_parameters(&self.state.db)
            .into_iter()
            .zip(method_impl.type_parameters(&self.state.db))
        {
            shapes.insert(target_par, *shapes.get(&src_par).unwrap());
        }

        self.add_implementation_shapes(method_impl, &mut shapes);
        self.add_method_bound_shapes(method_impl, &mut shapes);

        let new = self.specialize_method(typ, method_impl, &shapes, None);

        Instruction::CallInstance(Box::new(CallInstance {
            register: call.register,
            receiver: call.receiver,
            method: new,
            arguments: call.arguments.clone(),
            type_arguments: None,
            location: call.location,
        }))
    }

    fn specialize_method(
        &mut self,
        type_id: TypeId,
        method: MethodId,
        shapes: &HashMap<TypeParameterId, Shape>,
        custom_self_type: Option<TypeInstance>,
    ) -> MethodId {
        let ins = TypeInstance::new(type_id);
        let stype = custom_self_type.unwrap_or(ins);

        // Regular methods on regular types don't need to be specialized.
        if !type_id.is_generic(&self.state.db)
            && !type_id.is_closure(&self.state.db)
            && !method.is_generic(&self.state.db)
        {
            if self.work.push(stype, method, shapes.clone()) {
                self.update_method_type(method, shapes);
                self.regular_methods.push(method);
            }

            return method;
        }

        let mut key: Vec<Shape> = type_id
            .type_parameters(&self.state.db)
            .into_iter()
            .chain(method.type_parameters(&self.state.db))
            .map(|p| *shapes.get(&p).unwrap())
            .collect();

        self.prepare_key(&mut key);

        if let Some(new) = method.specialization(&self.state.db, &key) {
            return new;
        }

        let new_rec = method.receiver_for_type_instance(&self.state.db, ins);
        let new = self.specialize_method_type(new_rec, method, key, shapes);

        self.work.push(stype, new, shapes.clone());
        self.specialized_methods.push((method, new));
        new
    }

    fn schedule_regular_dropper(&mut self, original: TypeId, type_id: TypeId) {
        if type_id.is_generic(&self.state.db)
            || type_id.is_closure(&self.state.db)
        {
            return;
        }

        self.generate_dropper(original, type_id);
    }

    fn schedule_regular_inline_type_methods(&mut self, type_id: TypeId) {
        if type_id.is_generic(&self.state.db)
            || !type_id.is_inline_type(&self.state.db)
        {
            return;
        }

        let methods = [INCREMENT_METHOD, DECREMENT_METHOD];

        for name in methods {
            let method = type_id.method(&self.state.db, name).unwrap();
            let stype = TypeInstance::new(type_id);

            if self.work.push(stype, method, HashMap::new()) {
                self.regular_methods.push(method);
            }
        }
    }

    fn generate_dropper(&mut self, original: TypeId, type_id: TypeId) {
        let name = DROPPER_METHOD;

        // `copy` types won't have droppers, so there's nothing to do here.
        let Some(method) = original.method(&self.state.db, name) else {
            return;
        };

        // References to `self` in closures should point to the type of the
        // scope the closure is defined in, not the closure itself.
        let stype = if type_id.is_closure(&self.state.db) {
            self.self_type
        } else {
            TypeInstance::new(type_id)
        };

        if original == type_id {
            if self.work.push(stype, method, HashMap::new()) {
                self.regular_methods.push(method);
            }

            return;
        }

        let shapes = if type_id.is_closure(&self.state.db) {
            self.shapes.clone()
        } else {
            type_id
                .type_parameters(&self.state.db)
                .into_iter()
                .zip(type_id.shapes(&self.state.db).clone())
                .collect()
        };

        let new = self.specialize_method(type_id, method, &shapes, Some(stype));

        type_id.add_method(&mut self.state.db, name.to_string(), new);
    }

    fn generate_inline_type_methods(
        &mut self,
        original: TypeId,
        type_id: TypeId,
    ) {
        if !original.is_inline_type(&self.state.db) {
            return;
        }

        let methods = [INCREMENT_METHOD, DECREMENT_METHOD];

        for name in methods {
            let method = original.method(&self.state.db, name).unwrap();

            if original == type_id {
                let stype = TypeInstance::new(type_id);

                if self.work.push(stype, method, HashMap::new()) {
                    self.regular_methods.push(method);
                }

                continue;
            }

            let shapes = type_id
                .type_parameters(&self.state.db)
                .into_iter()
                .zip(type_id.shapes(&self.state.db).clone())
                .collect();

            let new = self.specialize_method(type_id, method, &shapes, None);
            let name = method.name(&self.state.db).clone();

            type_id.add_method(&mut self.state.db, name, new);
        }
    }

    fn generate_closure_methods(&mut self, original: TypeId, type_id: TypeId) {
        // Closures may capture generic types from the surrounding method, so we
        // have to expose the surrounding method's shapes to the closure.
        let shapes = self.shapes.clone();
        let method = original.method(&self.state.db, CALL_METHOD).unwrap();

        // Within a closure's `call` method, explicit references to or captures
        // of `self` should refer to the type of `self` as used by the method in
        // which the closure is defined, instead of pointing to the closure's
        // type.
        self.specialize_method(type_id, method, &shapes, Some(self.self_type));
    }

    /// Creates a new specialized method, using an existing method as its
    /// template/blueprint.
    ///
    /// This method is meant to be used for generic methods defined on regular
    /// types, or regular methods defined on generic types.
    fn specialize_method_type(
        &mut self,
        receiver: TypeRef,
        method: MethodId,
        mut key: Vec<Shape>,
        shapes: &HashMap<TypeParameterId, Shape>,
    ) -> MethodId {
        // For static methods we include the type's type parameter shapes such
        // that we can generate unique names using just the shapes for
        // non-generic static methods. If we didn't do this, then two different
        // instances of e.g. `Result.Ok` would produce the same symbol name
        let shape_params = if method.is_static(&self.state.db) {
            let typ = receiver.type_id(&self.state.db).unwrap();
            let mut params = typ.type_parameters(&self.state.db);

            params.append(&mut method.type_parameters(&self.state.db));
            params
        } else {
            method.type_parameters(&self.state.db)
        };

        let mut method_shapes: Vec<_> = shape_params
            .into_iter()
            .map(|p| *shapes.get(&p).unwrap())
            .collect();

        let new = method.clone_for_specialization(&mut self.state.db);
        let old_ret = method.return_type(&self.state.db);

        for arg in method.arguments(&self.state.db) {
            let arg_type = TypeSpecializer::new(
                &mut self.state.db,
                self.interned,
                shapes,
                &mut self.types,
                self.self_type,
            )
            .specialize(arg.value_type);

            let raw_var_type = arg.variable.value_type(&self.state.db);
            let var_loc = arg.variable.location(&self.state.db);
            let var_type = TypeSpecializer::new(
                &mut self.state.db,
                self.interned,
                shapes,
                &mut self.types,
                self.self_type,
            )
            .specialize(raw_var_type);

            new.new_argument(
                &mut self.state.db,
                arg.name,
                var_type,
                arg_type,
                var_loc,
            );
        }

        let new_ret = TypeSpecializer::new(
            &mut self.state.db,
            self.interned,
            shapes,
            &mut self.types,
            self.self_type,
        )
        .specialize(old_ret);

        self.prepare_key(&mut method_shapes);
        self.prepare_key(&mut key);

        let bounds = method.bounds(&self.state.db).clone();

        new.set_return_type(&mut self.state.db, new_ret);
        new.set_receiver(&mut self.state.db, receiver);
        new.set_shapes(&mut self.state.db, method_shapes);
        new.set_bounds(&mut self.state.db, bounds);

        if !key.is_empty() {
            method.add_specialization(&mut self.state.db, key, new);
        }

        new
    }

    /// Updates a regular method on a regular type such that its argument and
    /// return types are specialized (if needed).
    fn update_method_type(
        &mut self,
        method: MethodId,
        shapes: &HashMap<TypeParameterId, Shape>,
    ) {
        for (idx, arg) in
            method.arguments(&self.state.db).into_iter().enumerate()
        {
            let arg_type = TypeSpecializer::new(
                &mut self.state.db,
                self.interned,
                shapes,
                &mut self.types,
                self.self_type,
            )
            .specialize(arg.value_type);

            let raw_var_type = arg.variable.value_type(&self.state.db);
            let var_type = TypeSpecializer::new(
                &mut self.state.db,
                self.interned,
                shapes,
                &mut self.types,
                self.self_type,
            )
            .specialize(raw_var_type);

            method.update_argument_types(
                &mut self.state.db,
                idx,
                var_type,
                arg_type,
            );
        }

        let old_ret = method.return_type(&self.state.db);
        let new_ret = TypeSpecializer::new(
            &mut self.state.db,
            self.interned,
            shapes,
            &mut self.types,
            self.self_type,
        )
        .specialize(old_ret);

        method.set_return_type(&mut self.state.db, new_ret);
    }

    fn type_argument_shapes(
        &mut self,
        method: MethodId,
        arguments: &TypeArguments,
    ) -> HashMap<TypeParameterId, Shape> {
        let mut shapes = HashMap::new();

        for (&par, &bound) in method.bounds(&self.state.db).iter() {
            let shape = argument_shape(
                &self.state.db,
                self.interned,
                &self.shapes,
                arguments,
                par,
            );

            shapes.insert(bound, shape);
            shapes.insert(par, shape);
        }

        for &par in arguments.keys() {
            let arg = arguments.get_recursive(&self.state.db, par).unwrap();

            if let Some(id) = arg.as_type_parameter(&self.state.db) {
                if shapes.contains_key(&id) {
                    // We can reach this point if our parameter is assigned to a
                    // type parameter bound, and the shape for that is already
                    // generated by the above loop.
                    continue;
                }
            }

            shapes.insert(
                par,
                arg.shape(&self.state.db, self.interned, &self.shapes),
            );
        }

        // Calls may refer to type parameters from the surrounding scope, such
        // as when a return type of an inner call is inferred according to the
        // surrounding method's return type. As such, we need to include the
        // outer shapes as well.
        //
        // We do this _last_ such that these shapes don't affect the above
        // presence check.
        for (&par, shape) in &self.shapes {
            shapes.entry(par).or_insert(*shape);
        }

        shapes
    }

    fn add_method_bound_shapes(
        &self,
        method: MethodId,
        shapes: &mut HashMap<TypeParameterId, Shape>,
    ) {
        for (par, &bound) in method.bounds(&self.state.db).iter() {
            shapes.insert(bound, *shapes.get(par).unwrap());
        }
    }

    fn add_implementation_shapes(
        &mut self,
        method: MethodId,
        shapes: &mut HashMap<TypeParameterId, Shape>,
    ) {
        if let Some(tins) = method.implemented_trait_instance(&self.state.db) {
            // Regular types may implement generic traits, such as Int
            // implementing Equal[Int]. The traits may provide default methods
            // that use the trait's type parameters in their signature or body
            // (e.g. `Equal.!=`). We need to make sure we map those parameters
            // to their shapes.
            if tins.instance_of().is_generic(&self.state.db) {
                let args = tins.type_arguments(&self.state.db).unwrap();

                for &par in args.keys() {
                    shapes.insert(
                        par,
                        argument_shape(
                            &self.state.db,
                            self.interned,
                            shapes,
                            args,
                            par,
                        ),
                    );
                }
            }

            // Similarly, trait methods may end up depending on type parameters
            // from parent traits, such as when a default trait method calls
            // a method from a parent trait, and said method returns a type
            // parameter.
            let args =
                tins.instance_of().inherited_type_arguments(&self.state.db);

            for &par in args.keys() {
                shapes.insert(
                    par,
                    argument_shape(
                        &self.state.db,
                        self.interned,
                        shapes,
                        args,
                        par,
                    ),
                );
            }
        }
    }

    /// Prepares a specialization key so it can be used for consistent
    /// lookups/hashing.
    ///
    /// This is necessary because in certain cases we may produce e.g. a
    /// `Shape::Inline(ins)` shape where `ins` refers to an unspecialized type
    /// ID. Rather than try and handle that case in a variety of places that
    /// produce shapes, we handle this in this single place just before we
    /// actually use the key for a lookup.
    ///
    /// This preparation in turn is necessary so two different references to the
    /// same type, one using as specialized type ID and one using the raw one,
    /// result in the same lookup result.
    fn prepare_key(&mut self, key: &mut Vec<Shape>) {
        TypeSpecializer::specialize_shapes(
            &mut self.state.db,
            self.interned,
            &self.shapes,
            &mut self.types,
            self.self_type,
            key,
        );
    }
}

/// A type that expands the raw Drop instruction into dedicated instructions,
/// based on the types/shapes the Drop instruction operates on.
struct ExpandDrop<'a, 'b, 'c> {
    db: &'a Database,
    method: &'b mut Method,
    shapes: &'c HashMap<TypeParameterId, Shape>,
    intern: &'c mut InternedTypeArguments,
}

impl<'a, 'b, 'c> ExpandDrop<'a, 'b, 'c> {
    fn run(mut self) {
        let mut block_idx = 0;

        while block_idx < self.method.body.blocks.len() {
            let bid = BlockId(block_idx);

            if let Some((ins, remaining_ins)) = self.block_mut(bid).split_when(
                |ins| matches!(ins, Instruction::Drop(_)),
                |ins| match ins {
                    Instruction::Drop(i) => i,
                    _ => unreachable!(),
                },
            ) {
                let after = self.add_block();
                let succ = self.block_mut(bid).take_successors();

                self.insert(*ins, bid, after);

                for succ_id in succ {
                    self.method.body.remove_predecessor(succ_id, bid);
                    self.method.body.add_edge(after, succ_id);
                }

                self.block_mut(after).instructions = remaining_ins;
            }

            block_idx += 1;
        }
    }

    fn insert(&mut self, ins: Drop, block_id: BlockId, after_id: BlockId) {
        let loc = ins.location;
        let val = ins.register;
        let typ = self.method.registers.value_type(val);

        match typ.shape(self.db, self.intern, self.shapes) {
            Shape::Int(_, _)
            | Shape::Float(_)
            | Shape::Nil
            | Shape::Boolean
            | Shape::Pointer
            | Shape::Copy(_) => {
                self.ignore_value(block_id, after_id);
            }
            Shape::Mut | Shape::Ref => {
                self.drop_reference(block_id, after_id, val, loc);
            }
            Shape::Atomic | Shape::String => {
                self.drop_atomic(block_id, after_id, val, loc);
            }
            Shape::Owned | Shape::Inline(_) => {
                self.drop_owned(block_id, after_id, val, ins.dropper, loc);
            }
            Shape::InlineRef(t) | Shape::InlineMut(t) => {
                self.drop_stack_borrow(block_id, after_id, val, t, loc);
            }
        }
    }

    fn ignore_value(&mut self, before_id: BlockId, after_id: BlockId) {
        // We don't generate a goto() here because:
        //
        // 1. If there are other instructions in the current block, the cleanup
        //    phase connects the current and next block explicitly for us.
        // 2. If the current block is empty, this prevents a redundant basic
        //    block that only contains a goto to the next block.
        self.method.body.add_edge(before_id, after_id);
    }

    fn drop_reference(
        &mut self,
        before_id: BlockId,
        after_id: BlockId,
        value: RegisterId,
        location: InstructionLocation,
    ) {
        self.block_mut(before_id).decrement(value, location);
        self.block_mut(before_id).goto(after_id, location);
        self.method.body.add_edge(before_id, after_id);
    }

    fn drop_atomic(
        &mut self,
        before_id: BlockId,
        after_id: BlockId,
        value: RegisterId,
        location: InstructionLocation,
    ) {
        let drop_id = self.add_block();
        let check = self.block_mut(before_id);

        check.decrement_atomic(value, drop_id, after_id, location);

        // Atomic values can't be pattern matched into sub-values, so we can
        // call the dropper unconditionally.
        self.call_dropper(drop_id, value, location);
        self.block_mut(drop_id).goto(after_id, location);

        self.method.body.add_edge(before_id, drop_id);
        self.method.body.add_edge(before_id, after_id);
        self.method.body.add_edge(drop_id, after_id);
    }

    fn drop_owned(
        &mut self,
        before_id: BlockId,
        after_id: BlockId,
        value: RegisterId,
        dropper: bool,
        location: InstructionLocation,
    ) {
        if dropper {
            self.call_dropper(before_id, value, location);
        } else {
            let typ = self
                .method
                .registers
                .value_type(value)
                .type_id(self.db)
                .unwrap();

            if typ.is_heap_allocated(self.db) {
                self.block_mut(before_id).check_refs(value, location);
                self.block_mut(before_id).free(value, typ, location);
            }
        }

        self.block_mut(before_id).goto(after_id, location);
        self.method.body.add_edge(before_id, after_id);
    }

    fn call_dropper(
        &mut self,
        block: BlockId,
        value: RegisterId,
        location: InstructionLocation,
    ) {
        let typ = self.method.registers.value_type(value);
        let reg = self.method.registers.alloc(TypeRef::nil());

        if let Some(typ) = typ.type_id(self.db) {
            // If the type of the receiver is statically known to be a type, we
            // can just call the dropper directly.
            let method = typ.method(self.db, types::DROPPER_METHOD).unwrap();

            self.block_mut(block).call_instance(
                reg,
                value,
                method,
                Vec::new(),
                None,
                location,
            );
        } else if !typ.is_copy_type(self.db) {
            self.block_mut(block).call_dropper(reg, value, location);
        }
    }

    fn drop_stack_borrow(
        &mut self,
        before_id: BlockId,
        after_id: BlockId,
        value: RegisterId,
        instance: TypeInstance,
        location: InstructionLocation,
    ) {
        let reg = self.method.registers.alloc(TypeRef::nil());
        let method = instance
            .instance_of()
            .method(self.db, types::DECREMENT_METHOD)
            .unwrap();
        let args = Vec::new();

        self.block_mut(before_id)
            .call_instance(reg, value, method, args, None, location);
        self.block_mut(before_id).goto(after_id, location);
        self.method.body.add_edge(before_id, after_id);
    }

    fn block_mut(&mut self, id: BlockId) -> &mut Block {
        &mut self.method.body.blocks[id.0]
    }

    fn add_block(&mut self) -> BlockId {
        self.method.body.add_block()
    }
}

struct ExpandBorrow<'a, 'b, 'c> {
    db: &'a types::Database,
    method: &'b mut Method,
    shapes: &'c HashMap<TypeParameterId, Shape>,
    intern: &'c mut InternedTypeArguments,
}

impl<'a, 'b, 'c> ExpandBorrow<'a, 'b, 'c> {
    fn run(mut self) {
        let mut block_idx = 0;

        while block_idx < self.method.body.blocks.len() {
            let bid = BlockId(block_idx);

            if let Some((ins, remaining_ins)) = self.block_mut(bid).split_when(
                |ins| matches!(ins, Instruction::Borrow(_)),
                |ins| match ins {
                    Instruction::Borrow(i) => i,
                    _ => unreachable!(),
                },
            ) {
                let after = self.method.body.add_block();
                let succ = self.block_mut(bid).take_successors();

                self.insert(*ins, bid, after);

                for succ_id in succ {
                    self.method.body.remove_predecessor(succ_id, bid);
                    self.method.body.add_edge(after, succ_id);
                }

                self.block_mut(after).instructions = remaining_ins;
            }

            block_idx += 1;
        }
    }

    fn insert(&mut self, ins: Borrow, block_id: BlockId, after_id: BlockId) {
        let loc = ins.location;
        let reg = ins.register;
        let val = ins.value;
        let typ = self.method.registers.value_type(val);

        match typ.shape(self.db, self.intern, self.shapes) {
            Shape::Int(_, _)
            | Shape::Float(_)
            | Shape::Nil
            | Shape::Boolean
            | Shape::Pointer
            | Shape::Copy(_) => {
                // These values should be left as-is.
            }
            Shape::Mut | Shape::Ref | Shape::Owned => {
                self.block_mut(block_id).increment(val, loc);
            }
            Shape::Atomic | Shape::String => {
                self.block_mut(block_id).increment_atomic(val, loc);
            }
            Shape::Inline(t) | Shape::InlineRef(t) | Shape::InlineMut(t) => {
                self.borrow_inline_type(block_id, val, t, loc);
            }
        }

        self.block_mut(block_id).move_register(reg, val, loc);
        self.block_mut(block_id).goto(after_id, loc);
        self.method.body.add_edge(block_id, after_id);
    }

    fn block_mut(&mut self, id: BlockId) -> &mut Block {
        &mut self.method.body.blocks[id.0]
    }

    fn borrow_inline_type(
        &mut self,
        block: BlockId,
        value: RegisterId,
        instance: TypeInstance,
        location: InstructionLocation,
    ) {
        let reg = self.method.registers.alloc(TypeRef::nil());
        let method = instance
            .instance_of()
            .method(self.db, types::INCREMENT_METHOD)
            .unwrap();
        let args = Vec::new();

        self.block_mut(block)
            .call_instance(reg, value, method, args, None, location);
    }
}
