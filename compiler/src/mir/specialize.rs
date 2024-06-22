use crate::mir::{
    Block, BlockId, CallDynamic, CallInstance, CastType, Class as MirClass,
    Drop, Instruction, LocationId, Method, Mir, Reference, RegisterId, SELF_ID,
};
use crate::state::State;
use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::swap;
use types::specialize::{ordered_shapes_from_map, TypeSpecializer};
use types::{
    Block as _, ClassId, ClassInstance, Database, MethodId, Shape,
    TypeArguments, TypeParameterId, TypeRef, CALL_METHOD, DROPPER_METHOD,
};

fn argument_shape(
    db: &Database,
    shapes: &HashMap<TypeParameterId, Shape>,
    arguments: &TypeArguments,
    parameter: TypeParameterId,
) -> Shape {
    arguments.get_recursive(db, parameter).unwrap().shape(db, shapes)
}

fn specialize_constants(db: &mut Database, mir: &mut Mir) {
    let mut classes = Vec::new();
    let shapes = HashMap::new();

    for &id in mir.constants.keys() {
        let old_typ = id.value_type(db);
        let new_typ =
            TypeSpecializer::new(db, &shapes, &mut classes).specialize(old_typ);

        id.set_value_type(db, new_typ);
    }

    for class in classes {
        mir.classes.insert(class, MirClass::new(class));

        let mod_id = class.module(db);

        mir.modules.get_mut(&mod_id).unwrap().classes.push(class);
    }
}

struct Job {
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
        method: MethodId,
        shapes: HashMap<TypeParameterId, Shape>,
    ) -> bool {
        if self.done.insert(method) {
            self.jobs.push_back(Job { method, shapes });
            true
        } else {
            false
        }
    }

    fn pop(&mut self) -> Option<Job> {
        self.jobs.pop_front()
    }
}

/// A type that tracks classes along with their methods that are called using
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
///     class async Main {
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
    mapping: HashMap<ClassId, HashSet<DynamicCall>>,
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

impl DynamicCalls {
    fn new() -> DynamicCalls {
        DynamicCalls { mapping: HashMap::new() }
    }

    fn add(
        &mut self,
        class: ClassId,
        method: MethodId,
        key: Vec<Shape>,
        shapes: Vec<(TypeParameterId, Shape)>,
    ) {
        self.mapping.entry(class).or_default().insert(DynamicCall {
            method,
            key,
            shapes,
        });
    }

    fn get(&self, class: ClassId) -> Option<&HashSet<DynamicCall>> {
        self.mapping.get(&class)
    }
}

/// A compiler pass that specializes generic types.
pub(crate) struct Specialize<'a, 'b> {
    method: MethodId,
    state: &'a mut State,
    work: &'b mut Work,
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
    /// These classes are tracked so we can generate their droppers, and
    /// specialize any implemented trait methods that are called through dynamic
    /// dispatch.
    classes: Vec<ClassId>,
}

impl<'a, 'b> Specialize<'a, 'b> {
    pub(crate) fn run_all(state: &'a mut State, mir: &'a mut Mir) {
        // As part of specialization we create specializations for generics, and
        // discover all the classes that are in use. This ensures that once
        // we're done, anything that we didn't encounter is removed, ensuring
        // future passes don't operate on unused classes and methods.
        for class in mir.classes.values_mut() {
            class.methods.clear();
        }

        for module in mir.modules.values_mut() {
            module.classes.clear();
            module.methods.clear();
        }

        let mut work = Work::new();
        let mut dcalls = DynamicCalls::new();
        let main_class = state.db.main_class().unwrap();
        let main_method = state.db.main_method().unwrap();
        let main_mod = main_class.module(&state.db);

        work.push(main_method, HashMap::new());

        // The main() method isn't called explicitly, so we have to manually
        // record it in the main class.
        mir.classes.get_mut(&main_class).unwrap().methods.push(main_method);
        mir.modules.get_mut(&main_mod).unwrap().methods.push(main_method);

        while let Some(job) = work.pop() {
            Specialize {
                state,
                method: job.method,
                shapes: job.shapes,
                work: &mut work,
                regular_methods: Vec::new(),
                specialized_methods: Vec::new(),
                classes: Vec::new(),
            }
            .run(mir, &mut dcalls);
        }

        // Constants may contain arrays, so we need to make sure those use the
        // correct classes.
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
        specialize_constants(&mut state.db, mir);

        // Specialization may create many new methods, and in the process makes
        // the original generic methods redundant and unused. In fact, compiling
        // them as-is could result in incorrect code being generated. As such,
        // we end up removing all methods we haven't processed (i.e they're
        // unused).
        mir.methods.retain(|id, _| work.done.contains(id));

        // The specialization source is also set for regular classes that we
        // encounter. Thus, this filters out any classes that we don't encounter
        // anywhere; generic or not.
        mir.classes
            .retain(|id, _| id.specialization_source(&state.db).is_some());

        // We don't need the type arguments after this point.
        mir.type_arguments = Vec::new();
    }

    fn run(&mut self, mir: &mut Mir, dynamic_calls: &mut DynamicCalls) {
        self.update_self_type(mir);
        self.process_instructions(mir, dynamic_calls);
        self.process_specialized_types(mir, dynamic_calls);
        self.expand_instructions(mir);
        self.add_methods(mir);
    }

    fn update_self_type(&mut self, mir: &mut Mir) {
        let method = mir.methods.get_mut(&self.method).unwrap();

        if method.id.is_static(&self.state.db)
            || method.id.is_extern(&self.state.db)
        {
            return;
        }

        method.registers.get_mut(RegisterId(SELF_ID)).value_type =
            method.id.receiver(&self.state.db);
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
                &self.shapes,
                &mut self.classes,
            )
            .specialize(reg.value_type);
        }

        for block in &mut method.body.blocks {
            for instruction in &mut block.instructions {
                // When specializing a method, we _don't_ store them in any
                // class types. Different specializations of the same method use
                // the same name, so if they are stored on the same class they'd
                // overwrite each other. Since we don't need to look up any
                // methods by their names at and beyond this point, we just not
                // store them in the class types to begin with.
                match instruction {
                    Instruction::MoveRegister(ins) => {
                        // For trait methods `self` is updated to point to the
                        // class instance receiver of a method. We need to make
                        // sure that new type is propagated to any registers
                        // `self` is assigned to.
                        method.registers.get_mut(ins.target).value_type =
                            method.registers.get(ins.source).value_type;
                    }
                    Instruction::Reference(ins) => {
                        let src = method.registers.value_type(ins.value);
                        let target = method.registers.value_type(ins.register);

                        method.registers.get_mut(ins.register).value_type =
                            if target.is_ref(&self.state.db) {
                                src.as_ref(&self.state.db)
                            } else if target.is_mut(&self.state.db) {
                                src.force_as_mut(&self.state.db)
                            } else {
                                src
                            };
                    }
                    Instruction::CallExtern(ins) => {
                        mir.extern_methods.insert(ins.method);
                    }
                    Instruction::CallStatic(ins) => {
                        let rec = ins.method.receiver(&self.state.db);
                        let cls = rec.class_id(&self.state.db).unwrap();
                        let targs = ins
                            .type_arguments
                            .and_then(|i| mir.type_arguments.get(i));

                        ins.method = self.call_static(cls, ins.method, targs);
                    }
                    Instruction::CallInstance(ins) => {
                        let rec = method.registers.value_type(ins.receiver);
                        let cls = rec.class_id(&self.state.db).unwrap();
                        let targs = ins
                            .type_arguments
                            .and_then(|i| mir.type_arguments.get(i));

                        ins.method = self.call_static(cls, ins.method, targs);
                    }
                    Instruction::Send(ins) => {
                        let rec = method.registers.value_type(ins.receiver);
                        let cls = rec.class_id(&self.state.db).unwrap();
                        let targs = ins
                            .type_arguments
                            .and_then(|i| mir.type_arguments.get(i));

                        ins.method = self.call_static(cls, ins.method, targs);
                    }
                    Instruction::CallDynamic(call) => match method
                        .registers
                        .value_type(call.receiver)
                        .as_class_instance(&self.state.db)
                    {
                        // As part of specialization, we may encounter a dynamic
                        // call that's now acting on a class instance. We need
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
                        let cls = method
                            .registers
                            .value_type(ins.register)
                            .class_id(&self.state.db)
                            .unwrap();

                        ins.class = cls;
                        self.schedule_regular_dropper(cls);
                    }
                    Instruction::Free(ins) => {
                        let cls = method
                            .registers
                            .value_type(ins.register)
                            .class_id(&self.state.db)
                            .unwrap();

                        ins.class = cls;
                    }
                    Instruction::Spawn(ins) => {
                        let cls = method
                            .registers
                            .value_type(ins.register)
                            .class_id(&self.state.db)
                            .unwrap();

                        ins.class = cls;
                        self.schedule_regular_dropper(cls);
                    }
                    Instruction::SetField(ins) => {
                        ins.class = method
                            .registers
                            .value_type(ins.receiver)
                            .class_id(&self.state.db)
                            .unwrap();
                    }
                    Instruction::GetField(ins) => {
                        ins.class = method
                            .registers
                            .value_type(ins.receiver)
                            .class_id(&self.state.db)
                            .unwrap();
                    }
                    Instruction::FieldPointer(ins) => {
                        ins.class = method
                            .registers
                            .value_type(ins.receiver)
                            .class_id(&self.state.db)
                            .unwrap();
                    }
                    Instruction::MethodPointer(ins) => {
                        let rec = ins.method.receiver(&self.state.db);
                        let cls = rec.class_id(&self.state.db).unwrap();

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
                    _ => {}
                }
            }
        }
    }

    fn expand_instructions(&mut self, mir: &mut Mir) {
        let method = mir.methods.get_mut(&self.method).unwrap();

        ExpandDrop { db: &self.state.db, method, shapes: &self.shapes }.run();

        ExpandReference { db: &self.state.db, method, shapes: &self.shapes }
            .run();
    }

    fn process_specialized_types(
        &mut self,
        mir: &mut Mir,
        dynamic_calls: &mut DynamicCalls,
    ) {
        while let Some(class) = self.classes.pop() {
            mir.classes.entry(class).or_insert_with(|| MirClass::new(class));

            let mod_id = class.module(&self.state.db);
            let module = mir.modules.get_mut(&mod_id).unwrap();
            let kind = class.kind(&self.state.db);

            module.classes.push(class);

            if kind.is_extern() {
                // We don't generate droppers for extern classes, nor can they
                // be used for receivers as method calls.
                continue;
            }

            // New classes are only added for types to specialize, so the source
            // is always set at this point.
            let orig = class.specialization_source(&self.state.db).unwrap();

            self.generate_dropper(orig, class);

            if orig == class {
                // For regular classes the rest of the work doesn't apply.
                continue;
            }

            if kind.is_closure() {
                self.generate_closure_methods(orig, class);
            }

            if let Some(calls) = dynamic_calls.get(orig) {
                let mut class_shapes = HashMap::new();

                for (param, &shape) in class
                    .type_parameters(&self.state.db)
                    .into_iter()
                    .zip(class.shapes(&self.state.db))
                {
                    class_shapes.insert(param, shape);
                }

                for call in calls {
                    let mut shapes = class_shapes.clone();

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

                    self.add_implementation_shapes(call.method, &mut shapes);
                    self.add_method_bound_shapes(call.method, &mut shapes);
                    self.specialize_method(class, call.method, &shapes);
                }
            }
        }
    }

    fn add_methods(&mut self, mir: &mut Mir) {
        for &method in &self.regular_methods {
            self.track_method(method, mir);
        }

        for &(old, new) in &self.specialized_methods {
            let mut method = mir.methods[&old].clone();

            method.id = new;
            mir.methods.insert(new, method);
            self.track_method(new, mir);
        }
    }

    fn track_method(&self, method: MethodId, mir: &mut Mir) {
        let class =
            method.receiver(&self.state.db).class_id(&self.state.db).unwrap();

        mir.modules
            .get_mut(&class.module(&self.state.db))
            .unwrap()
            .methods
            .push(method);

        // Static methods aren't tracked in any classes, so we can skip the
        // rest.
        if method.is_instance(&self.state.db) {
            mir.classes.get_mut(&class).unwrap().methods.push(method);
        }
    }

    fn call_static(
        &mut self,
        class: ClassId,
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
        if shapes.is_empty() && class.is_generic(&self.state.db) {
            for (par, &shape) in class
                .type_parameters(&self.state.db)
                .into_iter()
                .zip(class.shapes(&self.state.db))
            {
                shapes.insert(par, shape);
            }
        }

        self.specialize_method(class, method, &shapes)
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

        let base_shapes = type_arguments
            .map(|args| self.type_argument_shapes(method, args))
            .unwrap_or_default();

        let mut method_params = HashSet::new();
        let mut method_shapes = Vec::new();

        for par in method.type_parameters(&self.state.db) {
            method_params.insert(par);
            method_shapes.push(*base_shapes.get(&par).unwrap());
        }

        // These are the shapes of the trait or any parent traits. We filter out
        // method parameter shapes because we derive those from the method key.
        let extra_shapes = base_shapes
            .iter()
            .filter(|(k, _)| !method_params.contains(*k))
            .map(|(&k, &v)| (k, v))
            .collect::<Vec<_>>();

        for class in trait_id.implemented_by(&self.state.db).clone() {
            let method_impl = class
                .method(&self.state.db, method.name(&self.state.db))
                .unwrap();

            dynamic_calls.add(
                class,
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

            if class.is_generic(&self.state.db) {
                let params = class.type_parameters(&self.state.db);

                for (key, class) in
                    class.specializations(&self.state.db).clone()
                {
                    // A dynamic call won't include shapes/type arguments for
                    // type parameters of the specialized class, so we have to
                    // inject those here.
                    //
                    // We don't need to clone `shapes` here because the type
                    // parameters are the same for every specialization of the
                    // base class, so we don't accidentally end up using the
                    // wrong shape on a future iteration of the surrounding
                    // loop.
                    for (&param, shape) in params.iter().zip(key) {
                        shapes.insert(param, shape);
                    }

                    // We have to repeat these two calls for every specialized
                    // class, because the shapes referred to through bounds or
                    // type arguments may differ per specialization.
                    self.add_implementation_shapes(method_impl, &mut shapes);
                    self.add_method_bound_shapes(method_impl, &mut shapes);
                    self.specialize_method(class, method_impl, &shapes);
                }
            } else {
                self.add_implementation_shapes(method_impl, &mut shapes);
                self.add_method_bound_shapes(method_impl, &mut shapes);
                self.specialize_method(class, method_impl, &shapes);
            }
        }

        // Most dynamically dispatched methods are likely not generic, meaning
        // that the list of shapes for the method parameters is empty. We want
        // to reuse previously specialized method types for the same receivers,
        // so we need to generate a key based on all type arguments present.
        // This ensures that multiple call sites of `next` on `Iter[Int]`
        // produce the same method ID, while call sites of `next` on
        // `Iter[String]` produce a different method ID.
        let spec_key = ordered_shapes_from_map(&base_shapes);

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
        receiver: ClassInstance,
        type_arguments: Option<&TypeArguments>,
    ) -> Instruction {
        let class = receiver.instance_of();
        let method_impl = class
            .specialization_source(&self.state.db)
            .unwrap_or(class)
            .method(&self.state.db, call.method.name(&self.state.db))
            .unwrap();

        let mut shapes = type_arguments
            .map(|args| self.type_argument_shapes(call.method, args))
            .unwrap_or_default();

        for (param, &shape) in class
            .type_parameters(&self.state.db)
            .into_iter()
            .zip(class.shapes(&self.state.db))
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

        let new = self.specialize_method(class, method_impl, &shapes);

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
        class: ClassId,
        method: MethodId,
        shapes: &HashMap<TypeParameterId, Shape>,
    ) -> MethodId {
        // Regular methods on regular types don't need to be specialized.
        if !class.is_generic(&self.state.db)
            && !class.is_closure(&self.state.db)
            && !method.is_generic(&self.state.db)
        {
            if self.work.push(method, shapes.clone()) {
                self.update_method_type(method, shapes);
                self.regular_methods.push(method);
            }

            return method;
        }

        let key: Vec<Shape> = class
            .type_parameters(&self.state.db)
            .into_iter()
            .chain(method.type_parameters(&self.state.db))
            .map(|p| *shapes.get(&p).unwrap())
            .collect();

        if let Some(new) = method.specialization(&self.state.db, &key) {
            return new;
        }

        let ins = ClassInstance::new(class);
        let new_rec = method.receiver_for_class_instance(&self.state.db, ins);
        let new = self.specialize_method_type(new_rec, method, key, shapes);

        self.work.push(new, shapes.clone());
        self.specialized_methods.push((method, new));
        new
    }

    fn schedule_regular_dropper(&mut self, class: ClassId) {
        if class.is_generic(&self.state.db) {
            return;
        }

        if let Some(dropper) = class.method(&self.state.db, DROPPER_METHOD) {
            if self.work.push(dropper, HashMap::new()) {
                self.regular_methods.push(dropper);
            }
        }
    }

    fn generate_dropper(&mut self, original: ClassId, class: ClassId) {
        let name = DROPPER_METHOD;
        let method = original.method(&self.state.db, name).unwrap();

        if original == class {
            if self.work.push(method, HashMap::new()) {
                self.regular_methods.push(method);
            }

            return;
        }

        let shapes = if class.is_closure(&self.state.db) {
            self.shapes.clone()
        } else {
            class
                .type_parameters(&self.state.db)
                .into_iter()
                .zip(class.shapes(&self.state.db).clone())
                .collect()
        };

        let new = self.specialize_method(class, method, &shapes);

        class.add_method(&mut self.state.db, name.to_string(), new);
    }

    fn generate_closure_methods(&mut self, original: ClassId, class: ClassId) {
        // Closures may capture generic types from the surrounding method, so we
        // have to expose the surrounding method's shapes to the closure.
        let shapes = self.shapes.clone();
        let method = original.method(&self.state.db, CALL_METHOD).unwrap();

        self.specialize_method(class, method, &shapes);
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
        key: Vec<Shape>,
        shapes: &HashMap<TypeParameterId, Shape>,
    ) -> MethodId {
        // For static methods we include the class' type parameter shapes such
        // that we can generate unique names using just the shapes for
        // non-generic static methods. If we didn't do this, then two different
        // instances of e.g. `Result.Ok` would produce the same symbol name
        let shape_params = if method.is_static(&self.state.db) {
            let class = receiver.class_id(&self.state.db).unwrap();
            let mut params = class.type_parameters(&self.state.db);

            params.append(&mut method.type_parameters(&self.state.db));
            params
        } else {
            method.type_parameters(&self.state.db)
        };

        let method_shapes = shape_params
            .into_iter()
            .map(|p| *shapes.get(&p).unwrap())
            .collect();

        let new = method.clone_for_specialization(&mut self.state.db);
        let old_ret = method.return_type(&self.state.db);

        for arg in method.arguments(&self.state.db) {
            let arg_type = TypeSpecializer::new(
                &mut self.state.db,
                shapes,
                &mut self.classes,
            )
            .specialize(arg.value_type);

            let raw_var_type = arg.variable.value_type(&self.state.db);
            let var_loc = *arg.variable.location(&self.state.db);
            let var_type = TypeSpecializer::new(
                &mut self.state.db,
                shapes,
                &mut self.classes,
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

        let new_ret =
            TypeSpecializer::new(&mut self.state.db, shapes, &mut self.classes)
                .specialize(old_ret);

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
                shapes,
                &mut self.classes,
            )
            .specialize(arg.value_type);

            let raw_var_type = arg.variable.value_type(&self.state.db);
            let var_type = TypeSpecializer::new(
                &mut self.state.db,
                shapes,
                &mut self.classes,
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
        let new_ret =
            TypeSpecializer::new(&mut self.state.db, shapes, &mut self.classes)
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
            let shape =
                argument_shape(&self.state.db, &self.shapes, arguments, par);

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

            shapes.insert(par, arg.shape(&self.state.db, &self.shapes));
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
        &self,
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
                let args = tins.type_arguments(&self.state.db);

                for &par in args.keys() {
                    shapes.insert(
                        par,
                        argument_shape(&self.state.db, shapes, args, par),
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
                    argument_shape(&self.state.db, shapes, args, par),
                );
            }
        }
    }
}

/// A type that expands the raw Drop instruction into dedicated instructions,
/// based on the types/shapes the Drop instruction operates on.
struct ExpandDrop<'a, 'b, 'c> {
    db: &'a Database,
    method: &'b mut Method,
    shapes: &'c HashMap<TypeParameterId, Shape>,
}

impl<'a, 'b, 'c> ExpandDrop<'a, 'b, 'c> {
    fn run(mut self) {
        let mut block_idx = 0;

        // We use a `while` loop here as both the list of blocks and
        // instructions are modified during iteration, meaning we can't use a
        // fixed range to iterate over.
        while block_idx < self.method.body.blocks.len() {
            let block_id = BlockId(block_idx);

            if let Some(ins_idx) = self
                .block_mut(block_id)
                .instructions
                .iter()
                .position(|ins| matches!(ins, Instruction::Drop(_)))
            {
                let (ins, remaining_ins) = {
                    let block = self.block_mut(block_id);

                    if let Instruction::Drop(ins) =
                        block.instructions.remove(ins_idx)
                    {
                        let ret = (ins, block.instructions.split_off(ins_idx));

                        // This ensures we don't keep redundant memory around if
                        // the number of instructions was very large.
                        block.instructions.shrink_to_fit();
                        ret
                    } else {
                        unreachable!()
                    }
                };

                let mut succ = Vec::new();
                let after_id = self.add_block();

                swap(&mut succ, &mut self.block_mut(block_id).successors);
                self.insert(*ins, block_id, after_id);

                // The new end block must be properly connected to the block(s)
                // the original block was connected to.
                for succ_id in succ {
                    self.method.body.remove_predecessor(succ_id, block_id);
                    self.method.body.add_edge(after_id, succ_id);
                }

                self.block_mut(after_id).instructions = remaining_ins;
            }

            block_idx += 1;
        }
    }

    fn insert(&mut self, ins: Drop, block_id: BlockId, after_id: BlockId) {
        let loc = ins.location;
        let val = ins.register;
        let typ = self.method.registers.value_type(val);

        match typ.shape(self.db, self.shapes) {
            Shape::Int | Shape::Float | Shape::Nil | Shape::Boolean => {
                self.ignore_value(block_id, after_id);
            }
            Shape::Mut | Shape::Ref => {
                self.drop_reference(block_id, after_id, val, loc);
            }
            Shape::Atomic | Shape::String => {
                self.drop_atomic(block_id, after_id, val, loc);
            }
            Shape::Owned if typ.is_permanent(self.db) => {
                self.ignore_value(block_id, after_id);
            }
            Shape::Owned => {
                self.drop_owned(block_id, after_id, val, ins.dropper, loc);
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
        location: LocationId,
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
        location: LocationId,
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
        location: LocationId,
    ) {
        if dropper {
            self.call_dropper(before_id, value, location);
        } else {
            let class = self
                .method
                .registers
                .value_type(value)
                .class_id(self.db)
                .unwrap();

            self.block_mut(before_id).check_refs(value, location);
            self.block_mut(before_id).free(value, class, location);
        }

        self.block_mut(before_id).goto(after_id, location);
        self.method.body.add_edge(before_id, after_id);
    }

    fn call_dropper(
        &mut self,
        block: BlockId,
        value: RegisterId,
        location: LocationId,
    ) {
        let typ = self.method.registers.value_type(value);
        let reg = self.method.registers.alloc(TypeRef::nil());

        if let Some(class) = typ.class_id(self.db) {
            // If the type of the receiver is statically known to be a class, we
            // can just call the dropper directly.
            let method = class.method(self.db, types::DROPPER_METHOD).unwrap();

            self.block_mut(block).call_instance(
                reg,
                value,
                method,
                Vec::new(),
                None,
                location,
            );
        } else if !typ.is_permanent(self.db) {
            self.block_mut(block).call_dropper(reg, value, location);
        }
    }

    fn block_mut(&mut self, id: BlockId) -> &mut Block {
        &mut self.method.body.blocks[id.0]
    }

    fn add_block(&mut self) -> BlockId {
        self.method.body.add_block()
    }
}

struct ExpandReference<'a, 'b, 'c> {
    db: &'a types::Database,
    method: &'b mut Method,
    shapes: &'c HashMap<TypeParameterId, Shape>,
}

impl<'a, 'b, 'c> ExpandReference<'a, 'b, 'c> {
    fn run(mut self) {
        let mut block_idx = 0;

        while block_idx < self.method.body.blocks.len() {
            let block_id = BlockId(block_idx);

            if let Some(ins_idx) = self
                .block_mut(block_id)
                .instructions
                .iter()
                .position(|ins| matches!(ins, Instruction::Reference(_)))
            {
                let (ins, remaining_ins) = {
                    let block = self.block_mut(block_id);

                    if let Instruction::Reference(ins) =
                        block.instructions.remove(ins_idx)
                    {
                        let ret = (ins, block.instructions.split_off(ins_idx));

                        // This ensures we don't keep redundant memory around if
                        // the number of instructions was very large.
                        block.instructions.shrink_to_fit();
                        ret
                    } else {
                        unreachable!()
                    }
                };

                let mut succ = Vec::new();
                let after_id = self.method.body.add_block();

                swap(&mut succ, &mut self.block_mut(block_id).successors);
                self.insert(*ins, block_id, after_id);

                for succ_id in succ {
                    self.method.body.remove_predecessor(succ_id, block_id);
                    self.method.body.add_edge(after_id, succ_id);
                }

                self.block_mut(after_id).instructions = remaining_ins;
            }

            block_idx += 1;
        }
    }

    fn insert(&mut self, ins: Reference, block_id: BlockId, after_id: BlockId) {
        let loc = ins.location;
        let reg = ins.register;
        let val = ins.value;
        let typ = self.method.registers.value_type(val);
        let is_extern = typ
            .class_id(self.db)
            .map_or(false, |i| i.kind(self.db).is_extern());

        match typ.shape(self.db, self.shapes) {
            Shape::Owned if is_extern || typ.is_permanent(self.db) => {
                // Extern and permanent values are to be left as-is.
            }
            Shape::Int | Shape::Float | Shape::Nil | Shape::Boolean => {
                // These are unboxed value types, or permanent types, both which
                // we should leave as-is.
            }
            Shape::Mut | Shape::Ref | Shape::Owned => {
                self.block_mut(block_id).increment(val, loc);
            }
            Shape::Atomic | Shape::String => {
                self.block_mut(block_id).increment_atomic(val, loc);
            }
        }

        self.block_mut(block_id).move_register(reg, val, loc);
        self.block_mut(block_id).goto(after_id, loc);
        self.method.body.add_edge(block_id, after_id);
    }

    fn block_mut(&mut self, id: BlockId) -> &mut Block {
        &mut self.method.body.blocks[id.0]
    }
}
