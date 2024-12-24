//! Helpers for performing graph-like operations on types, such as checking if a
//! type is recursive.
use types::{Database, TypeId, TypeInstance, TypeRef};

#[derive(Copy, Clone)]
enum Visit {
    /// The node has yet to be visited.
    Unvisited,

    /// The node is in the queue but has yet to be visited.
    ///
    /// This state exists to ensure we don't schedule the same node multiple
    /// times.
    Scheduled,

    /// A node's edges are being visited.
    Visiting,

    /// The node and its edges have been visited.
    Visited,
}

/// A type used for checking if a stack type is a recursive type.
pub(crate) struct RecursiveTypeChecker<'a> {
    db: &'a Database,
    states: Vec<Visit>,
    work: Vec<TypeId>,
}

impl<'a> RecursiveTypeChecker<'a> {
    pub(crate) fn new(db: &'a Database) -> RecursiveTypeChecker<'a> {
        RecursiveTypeChecker {
            db,
            states: vec![Visit::Unvisited; db.number_of_types()],
            work: Vec::new(),
        }
    }

    pub(crate) fn is_recursive(&mut self, type_id: TypeId) -> bool {
        self.add(type_id);

        while let Some(&typ) = self.work.last() {
            if let Visit::Visiting = self.state(typ) {
                self.set_state(typ, Visit::Visited);
                self.work.pop();
                continue;
            }

            self.set_state(typ, Visit::Visiting);

            for field in typ.fields(self.db) {
                let typ = field.value_type(self.db);
                let Some(ins) = self.edge(typ) else { continue };

                match self.state(ins.instance_of()) {
                    Visit::Unvisited => self.add(ins.instance_of()),
                    Visit::Visiting => return true,
                    _ => continue,
                }

                if !ins.instance_of().is_generic(self.db) {
                    continue;
                }

                for (_, &typ) in ins.type_arguments(self.db).unwrap().iter() {
                    let Some(ins) = self.edge(typ) else { continue };

                    match self.state(ins.instance_of()) {
                        Visit::Unvisited => self.add(ins.instance_of()),
                        Visit::Visiting => return true,
                        _ => continue,
                    }
                }
            }
        }

        false
    }

    fn edge(&self, typ: TypeRef) -> Option<TypeInstance> {
        // Pointers _are_ stack allocated, but they introduce indirection that
        // breaks recursion so we don't need to process them.
        if typ.is_pointer(self.db) {
            return None;
        }

        typ.as_type_instance(self.db)
            .filter(|v| v.instance_of().is_stack_allocated(self.db))
    }

    fn set_state(&mut self, id: TypeId, state: Visit) {
        self.states[id.0 as usize] = state;
    }

    fn state(&self, id: TypeId) -> Visit {
        self.states[id.0 as usize]
    }

    fn add(&mut self, id: TypeId) {
        self.set_state(id, Visit::Scheduled);
        self.work.push(id);
    }
}
