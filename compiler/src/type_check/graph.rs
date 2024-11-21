//! Helpers for performing graph-like operations on types, such as checking if a
//! class is recursive.
use types::{ClassId, ClassInstance, Database, TypeRef};

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

/// A type used for checking if a stack class is a recursive class.
pub(crate) struct RecursiveClassChecker<'a> {
    db: &'a Database,
    states: Vec<Visit>,
    work: Vec<ClassId>,
}

impl<'a> RecursiveClassChecker<'a> {
    pub(crate) fn new(db: &'a Database) -> RecursiveClassChecker<'a> {
        RecursiveClassChecker {
            db,
            states: vec![Visit::Unvisited; db.number_of_classes()],
            work: Vec::new(),
        }
    }

    pub(crate) fn is_recursive(&mut self, class: ClassId) -> bool {
        self.add(class);

        while let Some(&class) = self.work.last() {
            if let Visit::Visiting = self.state(class) {
                self.set_state(class, Visit::Visited);
                self.work.pop();
                continue;
            }

            self.set_state(class, Visit::Visiting);

            for field in class.fields(self.db) {
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

    fn edge(&self, typ: TypeRef) -> Option<ClassInstance> {
        // Pointers _are_ stack allocated, but they introduce indirection that
        // breaks recursion so we don't need to process them.
        if typ.is_pointer(self.db) {
            return None;
        }

        typ.as_class_instance(self.db)
            .filter(|v| v.instance_of().is_stack_allocated(self.db))
    }

    fn set_state(&mut self, id: ClassId, state: Visit) {
        self.states[id.0 as usize] = state;
    }

    fn state(&self, id: ClassId) -> Visit {
        self.states[id.0 as usize]
    }

    fn add(&mut self, id: ClassId) {
        self.set_state(id, Visit::Scheduled);
        self.work.push(id);
    }
}
