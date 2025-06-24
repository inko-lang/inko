use crate::mir::{
    Block, BlockId, Borrow, CallDynamic, CallInstance, CastType, Constant,
    Drop, Instruction, InstructionLocation, Method, Mir, RegisterId,
    Type as MirType,
};
use crate::state::State;
use indexmap::{IndexMap, IndexSet};
use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::swap;
use types::format::format_type;
use types::specialize::{Closures, TypeSpecializer};
use types::{
    Block as _, Database, InternedTypeArguments, MethodId, Shape, TraitId,
    TypeArguments, TypeEnum, TypeId, TypeInstance, TypeRef, CALL_METHOD,
    DECREMENT_METHOD, DROPPER_METHOD, INCREMENT_METHOD,
};

fn specialize_constants(
    db: &mut Database,
    mir: &mut Mir,
    closures: &mut Closures,
    interned: &mut InternedTypeArguments,
) {
    let mut types = Vec::new();
    let targs = TypeArguments::new();

    // Constants never need access to the self type, so we just use a dummy
    // value here.
    let stype = TypeEnum::TypeInstance(TypeInstance::new(TypeId::nil()));
    let mut stack: Vec<_> =
        mir.constants.iter_mut().map(|(&i, c)| (Some(i), c)).collect();

    while let Some((id, cons)) = stack.pop() {
        // Only Array constants need to be (recursively) specialized.
        let Constant::Array(vals, old) = cons else { continue };
        let new = TypeSpecializer::new(
            db, closures, interned, &targs, &targs, &mut types, stype,
        )
        .specialize(*old);

        *old = new;
        stack.extend(vals.iter_mut().map(|v| (None, v)));

        if let Some(id) = id {
            id.set_value_type(db, new);
        }
    }

    for typ in types {
        mir.types.insert(typ, MirType::new(typ));

        let mod_id = typ.module(db);

        mir.modules.get_mut(&mod_id).unwrap().types.push(typ);
    }
}

struct Job {
    self_type: TypeEnum,
    method: MethodId,
    type_arguments: TypeArguments,
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

    fn is_new(&self, method: MethodId) -> bool {
        !self.done.contains(&method)
    }

    fn add(&mut self, method: MethodId) -> bool {
        self.done.insert(method)
    }

    fn push(
        &mut self,
        method: MethodId,
        self_type: TypeEnum,
        type_arguments: TypeArguments,
    ) -> bool {
        if self.done.insert(method) {
            self.jobs.push_back(Job { self_type, method, type_arguments });
            true
        } else {
            false
        }
    }

    fn pop(&mut self) -> Option<Job> {
        self.jobs.pop_front()
    }
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct DynamicCall {
    method: MethodId,
    type_arguments: Option<usize>,
}

impl DynamicCall {
    fn new(method: MethodId, type_arguments: Option<usize>) -> Self {
        Self { method, type_arguments }
    }
}

/// Info needed to specialize the potential targets of dynamic dispatch calls.
struct Dynamic {
    casts: HashMap<TraitId, IndexSet<TypeInstance>>,
    calls: HashMap<TraitId, IndexSet<DynamicCall>>,
}

impl Dynamic {
    fn new() -> Self {
        Self { casts: HashMap::new(), calls: HashMap::new() }
    }

    fn add_cast(&mut self, trait_id: TraitId, instance: TypeInstance) -> bool {
        self.casts.entry(trait_id).or_default().insert(instance)
    }

    fn types_cast_to(
        &self,
        trait_id: TraitId,
    ) -> impl Iterator<Item = TypeInstance> + '_ {
        self.casts.get(&trait_id).into_iter().flat_map(|x| x.iter()).cloned()
    }

    fn add_call(
        &mut self,
        trait_id: TraitId,
        method: MethodId,
        arguments: Option<usize>,
    ) -> bool {
        self.calls
            .entry(trait_id)
            .or_default()
            .insert(DynamicCall::new(method, arguments))
    }

    fn calls_for(
        &self,
        trait_id: TraitId,
    ) -> impl Iterator<Item = DynamicCall> + '_ {
        self.calls.get(&trait_id).into_iter().flat_map(|x| x.iter()).cloned()
    }
}

/// A compiler pass that specializes generic types.
pub(crate) struct Specialize<'a, 'b> {
    method: MethodId,
    state: &'a mut State,
    work: &'b mut Work,
    interned: &'b mut InternedTypeArguments,
    closures: &'b mut Closures,

    /// The type of the `self` and `Self`.
    self_type: TypeEnum,

    /// The type arguments the method has access to.
    type_arguments: TypeArguments,

    /// Method specializations created while processing the body of the method.
    ///
    /// The tuple stores the following:
    ///
    /// 1. The ID of the original/old method the specialization is based on.
    /// 1. The ID of the newly specialized method.
    ///
    /// For regular methods both values are the same.
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
        let mut intern = InternedTypeArguments::new();
        let mut dynamic = Dynamic::new();
        let mut closures = Closures::new();
        let main_type = state.db.main_type().unwrap();
        let main_method = state.db.main_method().unwrap();
        let main_mod = main_type.module(&state.db);

        work.push(
            main_method,
            TypeEnum::TypeInstance(TypeInstance::new(main_type)),
            TypeArguments::new(),
        );

        // Main.main is the entry point and thus has to be added manually.
        mir.types.get_mut(&main_type).unwrap().methods.push(main_method);
        mir.modules.get_mut(&main_mod).unwrap().methods.push(main_method);

        while let Some(job) = work.pop() {
            Specialize {
                state,
                closures: &mut closures,
                interned: &mut intern,
                self_type: job.self_type,
                method: job.method,
                type_arguments: job.type_arguments,
                work: &mut work,
                specialized_methods: Vec::new(),
                types: Vec::new(),
            }
            .run(mir, &mut dynamic);
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
        specialize_constants(&mut state.db, mir, &mut closures, &mut intern);

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

    fn run(&mut self, mir: &mut Mir, dynamic: &mut Dynamic) {
        self.process_instructions(mir, dynamic);
        self.process_specialized_types(mir);
        self.expand_instructions(mir);
        self.add_methods(mir);
    }

    fn process_instructions(&mut self, mir: &mut Mir, dynamic: &mut Dynamic) {
        let method = mir.methods.get_mut(&self.method).unwrap();

        // Rather than specializing the registers of instructions that may
        // produce generic types, we just specialize all of them. The type
        // specializer bails out if this isn't needed anyway, and this makes our
        // code not prone to accidentally forgetting to specialize a register
        // when adding or changing MIR instructions.
        for reg in method.registers.iter_mut() {
            reg.value_type = TypeSpecializer::new(
                &mut self.state.db,
                self.closures,
                self.interned,
                &self.type_arguments,
                &self.type_arguments,
                &mut self.types,
                self.self_type,
            )
            .specialize(reg.value_type);
        }

        for block in &mut method.body.blocks {
            for instruction in &mut block.instructions {
                match instruction {
                    Instruction::CallExtern(ins) => {
                        mir.extern_methods.insert(ins.method);
                    }
                    Instruction::CallStatic(ins) => {
                        let rec = ins.method.receiver(&self.state.db);
                        let typ = rec.type_id(&self.state.db).unwrap();
                        let targs = ins
                            .type_arguments
                            .and_then(|i| mir.type_arguments.get(i));

                        ins.method = self.call_static(typ, ins.method, targs);
                    }
                    Instruction::CallInstance(ins) => {
                        let rec = method.registers.value_type(ins.receiver);
                        let typ = rec.type_id(&self.state.db).unwrap();
                        let targs = ins
                            .type_arguments
                            .and_then(|i| mir.type_arguments.get(i));

                        ins.method = self.call_static(typ, ins.method, targs);
                    }
                    Instruction::Send(ins) => {
                        let rec = method.registers.value_type(ins.receiver);
                        let typ = rec.type_id(&self.state.db).unwrap();
                        let targs = ins
                            .type_arguments
                            .and_then(|i| mir.type_arguments.get(i));

                        ins.method = self.call_static(typ, ins.method, targs);
                    }
                    Instruction::CallDynamic(call) => match method
                        .registers
                        .value_type(call.receiver)
                        .as_type_instance(&self.state.db)
                    {
                        Some(ins) => {
                            let targs = call
                                .type_arguments
                                .and_then(|i| mir.type_arguments.get(i));

                            *instruction = self
                                .devirtualize_call_dynamic(call, ins, targs);
                        }
                        _ => {
                            let old = call.method;
                            let (new, args) = self.call_dynamic(
                                old,
                                call.type_arguments,
                                &mir.type_arguments,
                                dynamic,
                            );

                            call.method = new;
                            mir.dynamic_calls
                                .entry(old)
                                .or_default()
                                .insert((new, args));
                        }
                    },
                    Instruction::Allocate(ins) => {
                        let new = method
                            .registers
                            .value_type(ins.register)
                            .type_id(&self.state.db)
                            .unwrap();

                        ins.type_id = new;
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
                        let new = method
                            .registers
                            .value_type(ins.register)
                            .type_id(&self.state.db)
                            .unwrap();

                        ins.type_id = new;
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
                        let typ = rec.type_id(&self.state.db).unwrap();

                        ins.method = self.call_static(typ, ins.method, None);
                    }
                    Instruction::Cast(ins) => {
                        let from = method.registers.value_type(ins.source);
                        let to = method.registers.value_type(ins.register);

                        ins.from = CastType::from(&self.state.db, from);
                        ins.to = CastType::from(&self.state.db, to);

                        self.cast_type(from, to, &mir.type_arguments, dynamic);
                    }
                    Instruction::SizeOf(ins) => {
                        ins.argument = TypeSpecializer::new(
                            &mut self.state.db,
                            self.closures,
                            self.interned,
                            &self.type_arguments,
                            &self.type_arguments,
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

        ExpandDrop { db: &self.state.db, method }.run();
        ExpandBorrow { db: &self.state.db, method }.run();
    }

    fn process_specialized_types(&mut self, mir: &mut Mir) {
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

            if kind.is_closure() {
                self.generate_closure_methods(orig, typ);
            }
        }
    }

    fn add_methods(&mut self, mir: &mut Mir) {
        for &(old, new) in &self.specialized_methods {
            if old != new {
                let mut method = mir.methods.get(&old).unwrap().clone();

                method.id = new;
                mir.methods.insert(new, method);
            }

            self.track_method(new, mir);
        }
    }

    fn track_method(&self, method: MethodId, mir: &mut Mir) {
        let tid =
            method.receiver(&self.state.db).type_id(&self.state.db).unwrap();
        let mid = &tid.module(&self.state.db);

        mir.modules.get_mut(mid).unwrap().methods.push(method);

        // Static methods aren't tracked in any types, so we can skip the rest.
        if method.is_instance(&self.state.db) {
            mir.types.get_mut(&tid).unwrap().methods.push(method);
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
            TypeEnum::TypeInstance(TypeInstance::new(type_id))
        };

        if original == type_id {
            if self.work.push(method, stype, TypeArguments::new()) {
                self.specialized_methods.push((method, method));
            }

            return;
        }

        let arguments = if type_id.is_closure(&self.state.db) {
            self.type_arguments.clone()
        } else if type_id.is_generic(&self.state.db) {
            type_id.type_arguments(&self.state.db).cloned().unwrap()
        } else {
            TypeArguments::new()
        };

        let new =
            self.specialize_method(type_id, method, arguments, Some(stype));

        type_id.add_method(&mut self.state.db, name.to_string(), new);
    }

    fn generate_closure_methods(&mut self, original: TypeId, type_id: TypeId) {
        // Closures may capture generic types from the surrounding method, so we
        // have to expose the surrounding method's type arguments to the
        // closure.
        let targs = self.type_arguments.clone();
        let method = original.method(&self.state.db, CALL_METHOD).unwrap();

        // Within a closure's `call` method, explicit references to or captures
        // of `self` should refer to the type of `self` as used by the method in
        // which the closure is defined, instead of pointing to the closure's
        // type.
        self.specialize_method(type_id, method, targs, Some(self.self_type));
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
                let stype = TypeEnum::TypeInstance(TypeInstance::new(type_id));

                if self.work.push(method, stype, TypeArguments::new()) {
                    self.specialized_methods.push((method, method));
                }

                continue;
            }

            let targs = if type_id.is_generic(&self.state.db) {
                type_id.type_arguments(&self.state.db).cloned().unwrap()
            } else {
                TypeArguments::new()
            };
            let new = self.specialize_method(type_id, method, targs, None);
            let name = method.name(&self.state.db).clone();

            type_id.add_method(&mut self.state.db, name, new);
        }
    }

    fn call_static(
        &mut self,
        type_id: TypeId,
        method: MethodId,
        type_arguments: Option<&TypeArguments>,
    ) -> MethodId {
        let mut targs =
            type_arguments.cloned().unwrap_or_else(TypeArguments::new);

        if let Some(t) = type_id.type_arguments(&self.state.db) {
            t.copy_into(&mut targs);
        }

        self.specialize_method(type_id, method, targs, None)
    }

    fn devirtualize_call_dynamic(
        &mut self,
        call: &CallDynamic,
        receiver: TypeInstance,
        type_arguments: Option<&TypeArguments>,
    ) -> Instruction {
        let typ = receiver.instance_of();
        let Some(method) = receiver
            .source_type(&self.state.db)
            .method(&self.state.db, call.method.name(&self.state.db))
        else {
            panic!(
                "can't devirtualize call to {}.{} in {}.{}",
                receiver.instance_of().name(&self.state.db),
                call.method.name(&self.state.db),
                format_type(&self.state.db, self.self_type),
                self.method.name(&self.state.db),
            );
        };

        let mut targs = typ
            .type_arguments(&self.state.db)
            .cloned()
            .unwrap_or_else(TypeArguments::new);

        // If the method has type parameters, the type parameters of the trait's
        // method aren't the exact same ones as the method implementation (i.e.
        // their IDs differ), so we need to ensure the implementation's
        // parameters are also assigned.
        if let Some(call_targs) = type_arguments {
            for (impl_par, call_par) in method
                .type_parameters(&self.state.db)
                .into_iter()
                .zip(call.method.type_parameters(&self.state.db))
            {
                targs.assign(impl_par, call_targs.get(call_par).unwrap());
            }
        }

        // We have to add these extra type arguments here as the type checker
        // only does this when the receiver's type is statically known.
        self.add_implementation_arguments(method, &mut targs);

        let new = self.specialize_method(typ, method, targs, None);

        Instruction::CallInstance(Box::new(CallInstance {
            register: call.register,
            receiver: call.receiver,
            method: new,
            arguments: call.arguments.clone(),
            type_arguments: None,
            location: call.location,
        }))
    }

    fn call_dynamic(
        &mut self,
        method: MethodId,
        type_arguments: Option<usize>,
        all_type_arguments: &[TypeArguments],
        dynamic: &mut Dynamic,
    ) -> (MethodId, Vec<TypeRef>) {
        let mut targs = type_arguments
            .and_then(|i| all_type_arguments.get(i))
            .cloned()
            .unwrap_or_else(TypeArguments::new);

        self.specialize_type_arguments(&mut targs);

        let method_targs = method
            .type_parameters(&self.state.db)
            .into_iter()
            .map(|p| targs.get(p).unwrap())
            .collect();

        let rec = method.receiver(&self.state.db);
        let trait_ins = rec.as_trait_instance(&self.state.db).unwrap();
        let trait_id = trait_ins.instance_of();

        // Non-generic methods only need to be updated in-place.
        if !trait_id.is_generic(&self.state.db)
            && !method.is_generic(&self.state.db)
            && self.work.add(method)
        {
            let stype = TypeEnum::TraitInstance(trait_ins.as_self_type());

            self.specialize_regular_method_type(method, &targs, stype);
            self.specialize_dynamic_call(trait_id, method, &targs, dynamic);
            dynamic.add_call(trait_id, method, type_arguments);
            return (method, method_targs);
        }

        let key: Vec<_> = trait_id
            .type_parameters(&self.state.db)
            .into_iter()
            .chain(method.type_parameters(&self.state.db))
            .map(|p| targs.get(p).unwrap())
            .collect();

        if let Some(new) = method.specialization(&self.state.db, &key) {
            return (new, method_targs);
        }

        let new = self.specialize_method_type(rec, method, key, &targs);

        self.specialize_dynamic_call(trait_id, method, &targs, dynamic);
        dynamic.add_call(trait_id, method, type_arguments);
        (new, method_targs)
    }

    fn cast_type(
        &mut self,
        from: TypeRef,
        to: TypeRef,
        all_type_arguments: &[TypeArguments],
        dynamic: &mut Dynamic,
    ) {
        let Ok(TypeEnum::TypeInstance(from)) =
            from.as_type_enum(&self.state.db)
        else {
            return;
        };
        let Ok(TypeEnum::TraitInstance(to)) = to.as_type_enum(&self.state.db)
        else {
            return;
        };

        // We record the type cast such that we know for what types we need to
        // specialize methods that _may_ be called through dynamic dispatch.
        let orig_trait =
            to.instance_of().specialization_source(&self.state.db).unwrap();

        // We also need to record any parent traits, such that we can take into
        // account types that are cast to a trait and then cast to a parent
        // trait.
        let mut stack = vec![orig_trait];
        let from_typ = from.instance_of();
        let from_orig = from_typ.specialization_source(&self.state.db).unwrap();

        while let Some(tid) = stack.pop() {
            if !dynamic.add_cast(tid, from) {
                continue;
            }

            for ins in tid.required_traits(&self.state.db) {
                stack.push(ins.instance_of());
            }

            for call in dynamic.calls_for(tid) {
                let method = from_orig
                    .method(&self.state.db, call.method.name(&self.state.db))
                    .unwrap();
                let targs =
                    call.type_arguments.and_then(|i| all_type_arguments.get(i));
                let mut args = from_typ
                    .type_arguments(&self.state.db)
                    .cloned()
                    .unwrap_or_else(TypeArguments::new);

                if let Some(targs) = targs {
                    for (impl_par, dyn_par) in method
                        .type_parameters(&self.state.db)
                        .into_iter()
                        .zip(call.method.type_parameters(&self.state.db))
                    {
                        args.assign(impl_par, targs.get(dyn_par).unwrap());
                    }
                }

                self.add_implementation_arguments(method, &mut args);
                self.call_static(from_typ, method, Some(&args));
            }
        }
    }

    fn specialize_method(
        &mut self,
        type_id: TypeId,
        method: MethodId,
        mut type_arguments: TypeArguments,
        custom_self: Option<TypeEnum>,
    ) -> MethodId {
        // If there are any bounds we need to make sure they're assigned the
        // same arguments as the original parameters. We do this here such that
        // we can handle both static calls, devirtualized dynamic calls, and
        // regular dynamic calls.
        for (&par, &bound) in method.bounds(&self.state.db).iter() {
            type_arguments.assign(bound, type_arguments.get(par).unwrap());
        }

        self.specialize_type_arguments(&mut type_arguments);

        let ins = TypeInstance::new(type_id);
        let rec = method.receiver_for_type_instance(&self.state.db, ins);
        let stype = custom_self
            .unwrap_or_else(|| rec.as_type_enum(&self.state.db).unwrap());

        // Non-generic methods only need to be updated in-place.
        if !type_id.is_generic(&self.state.db)
            && !type_id.is_closure(&self.state.db)
            && !method.is_generic(&self.state.db)
        {
            if self.work.is_new(method) {
                self.specialize_regular_method_type(
                    method,
                    &type_arguments,
                    stype,
                );
                self.specialized_methods.push((method, method));
                self.work.push(method, stype, type_arguments);
            }

            return method;
        }

        let key: Vec<_> = type_id
            .type_parameters(&self.state.db)
            .into_iter()
            .chain(method.type_parameters(&self.state.db))
            .map(|p| type_arguments.get(p).unwrap())
            .collect();

        if !key.is_empty() {
            if let Some(new) = method.specialization(&self.state.db, &key) {
                return new;
            }
        }

        let new =
            self.specialize_method_type(rec, method, key, &type_arguments);

        self.work.push(new, stype, type_arguments);
        self.specialized_methods.push((method, new));
        new
    }

    fn specialize_dynamic_call(
        &mut self,
        trait_id: TraitId,
        method: MethodId,
        type_arguments: &TypeArguments,
        dynamic: &mut Dynamic,
    ) {
        for ins in dynamic.types_cast_to(trait_id) {
            let typ = ins.instance_of();

            // Methods are only stored in the original type, not the specialized
            // type. Only fully processed types are tracked, so the unwrap()
            // won't panic here.
            let typ_method = ins
                .source_type(&self.state.db)
                .method(&self.state.db, method.name(&self.state.db))
                .unwrap();
            let mut args = typ
                .type_arguments(&self.state.db)
                .cloned()
                .unwrap_or_else(TypeArguments::new);

            // We don't need the type arguments of the receiving trait here,
            // because the type arguments of the specialized type's
            // implementation will overrule them.
            for (typ_par, dyn_par) in typ_method
                .type_parameters(&self.state.db)
                .into_iter()
                .zip(method.type_parameters(&self.state.db))
            {
                args.assign(typ_par, type_arguments.get(dyn_par).unwrap());
            }

            self.add_implementation_arguments(method, &mut args);
            self.call_static(typ, typ_method, Some(&args));
        }
    }

    fn specialize_method_type(
        &mut self,
        receiver: TypeRef,
        method: MethodId,
        key: Vec<TypeRef>,
        type_arguments: &TypeArguments,
    ) -> MethodId {
        let new = method.clone_for_specialization(&mut self.state.db);
        let stype = receiver.as_type_enum(&self.state.db).unwrap();

        for arg in method.arguments(&self.state.db) {
            let arg_typ =
                self.specialize_with(arg.value_type, stype, type_arguments);
            let old_var_typ = arg.variable.value_type(&self.state.db);
            let var_typ =
                self.specialize_with(old_var_typ, stype, type_arguments);
            let loc = arg.variable.location(&self.state.db);
            let db = &mut self.state.db;

            new.new_argument(db, arg.name, var_typ, arg_typ, loc);
        }

        let old_ret = method.return_type(&self.state.db);
        let new_ret = self.specialize_with(old_ret, stype, type_arguments);

        // For static methods we need to include the type arguments of both the
        // receiver and the method, otherwise this may result in duplicate
        // symbol names (e.g. for the `Ok` static method of the `Result` type).
        //
        // For other methods we only need to include the arguments of the method
        // itself, as the receiver's arguments are already stored in the
        // receiver type.
        let method_args = if method.is_static(&self.state.db) {
            key.clone()
        } else {
            // The receiver might be a type or trait instance. Because the key
            // types are always in the order of `(receiver types, method
            // types)`, we can simply skip the first N that belong to the
            // receiver, without having to make any assertions about its type.
            let take = method.number_of_type_parameters(&self.state.db);
            let skip = key.len() - take;

            key[skip..].to_vec()
        };

        new.set_type_arguments(&mut self.state.db, method_args);
        new.set_return_type(&mut self.state.db, new_ret);
        new.set_receiver(&mut self.state.db, receiver);

        // The key might be empty for e.g. the "call" method of a closure,
        // because neither the type nor the method are generic. If we instead
        // stored the empty key, all such methods would converge onto the same
        // specialized method even if the closures are different (e.g. they
        // capture different types).
        if !key.is_empty() {
            method.add_specialization(&mut self.state.db, key, new);
        }

        new
    }

    fn specialize_regular_method_type(
        &mut self,
        method: MethodId,
        type_arguments: &TypeArguments,
        self_type: TypeEnum,
    ) {
        for (idx, arg) in
            method.arguments(&self.state.db).into_iter().enumerate()
        {
            let arg_typ =
                self.specialize_with(arg.value_type, self_type, type_arguments);
            let old_var_typ = arg.variable.value_type(&self.state.db);
            let new_var_typ =
                self.specialize_with(old_var_typ, self_type, type_arguments);

            method.update_argument_types(
                &mut self.state.db,
                idx,
                new_var_typ,
                arg_typ,
            );
        }

        let old_ret = method.return_type(&self.state.db);
        let new_ret = self.specialize_with(old_ret, self_type, type_arguments);

        method.set_return_type(&mut self.state.db, new_ret);
    }

    fn add_implementation_arguments(
        &mut self,
        method: MethodId,
        arguments: &mut TypeArguments,
    ) {
        let Some(ins) = method.implemented_trait_instance(&self.state.db)
        else {
            return;
        };

        ins.copy_type_arguments_into(&self.state.db, arguments);

        ins.instance_of()
            .inherited_type_arguments(&self.state.db)
            .copy_into(arguments);
    }

    // Specializes all the given type arguments.
    //
    /// Type arguments may contain types originating from the receiver _or_ the
    /// surrounding type. Thus when specializing the type arguments, we need to
    /// take _both_ sources into account.
    fn specialize_type_arguments(&mut self, arguments: &mut TypeArguments) {
        for idx in 0..(arguments.len()) {
            let par = arguments.get_parameter_at(idx).unwrap();
            let old = arguments.get(par).unwrap();
            let new = self.specialize_with(old, self.self_type, arguments);

            arguments.assign(par, new);
        }
    }

    fn specialize_with(
        &mut self,
        typ: TypeRef,
        self_type: TypeEnum,
        type_arguments: &TypeArguments,
    ) -> TypeRef {
        TypeSpecializer::new(
            &mut self.state.db,
            self.closures,
            self.interned,
            type_arguments,
            &self.type_arguments,
            &mut self.types,
            self_type,
        )
        .specialize(typ)
    }
}

/// A type that expands the raw Drop instruction into dedicated instructions,
/// based on the types the Drop instruction operates on.
struct ExpandDrop<'a> {
    db: &'a Database,
    method: &'a mut Method,
}

impl<'a> ExpandDrop<'a> {
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

        match typ.shape(self.db) {
            Shape::Copy => {
                self.ignore_value(block_id, after_id);
            }
            Shape::Borrow => {
                self.drop_reference(block_id, after_id, val, loc);
            }
            Shape::Atomic => {
                self.drop_atomic(block_id, after_id, val, loc);
            }
            Shape::Owned | Shape::Inline => {
                self.drop_owned(block_id, after_id, val, ins.dropper, loc);
            }
            Shape::InlineBorrow => {
                self.drop_stack_borrow(block_id, after_id, val, loc);
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
        location: InstructionLocation,
    ) {
        let instance = self
            .method
            .registers
            .value_type(value)
            .as_type_instance(self.db)
            .unwrap();
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

struct ExpandBorrow<'a> {
    db: &'a types::Database,
    method: &'a mut Method,
}

impl<'a> ExpandBorrow<'a> {
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

        match typ.shape(self.db) {
            Shape::Copy => {
                // These values should be left as-is.
            }
            Shape::Borrow | Shape::Owned => {
                self.block_mut(block_id).increment(val, loc);
            }
            Shape::Atomic => {
                self.block_mut(block_id).increment_atomic(val, loc);
            }
            Shape::Inline | Shape::InlineBorrow => {
                self.borrow_inline_type(block_id, val, loc);
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
        location: InstructionLocation,
    ) {
        let instance = self
            .method
            .registers
            .value_type(value)
            .as_type_instance(self.db)
            .unwrap();
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
