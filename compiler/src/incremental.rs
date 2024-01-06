use std::collections::{HashMap, HashSet};
use types::module_name::ModuleName;

struct Node {
    /// The indexes of the modules that directly depend on this module.
    depending: HashSet<usize>,

    /// If the module's code has changed and its cache(s) should be invalidated.
    changed: bool,
}

impl Node {
    fn new() -> Node {
        Node { depending: HashSet::new(), changed: false }
    }
}

pub(crate) struct DependencyGraph {
    /// All the modules/nodes in this dependency graph.
    nodes: Vec<Node>,

    /// A mapping of module names to their indexes in the `modules` array.
    mapping: HashMap<ModuleName, usize>,
}

impl DependencyGraph {
    pub(crate) fn new() -> DependencyGraph {
        DependencyGraph { nodes: Vec::new(), mapping: HashMap::new() }
    }

    pub(crate) fn add_module(&mut self, name: ModuleName) -> usize {
        if let Some(&id) = self.mapping.get(&name) {
            return id;
        }

        let id = self.nodes.len();

        self.nodes.push(Node::new());
        self.mapping.insert(name, id);
        id
    }

    pub(crate) fn module_id(&self, name: &ModuleName) -> Option<usize> {
        self.mapping.get(name).cloned()
    }

    pub(crate) fn add_depending(&mut self, module: usize, depending: usize) {
        self.nodes[module].depending.insert(depending);
    }

    pub(crate) fn mark_as_changed(&mut self, module: usize) -> bool {
        if self.nodes[module].changed {
            false
        } else {
            self.nodes[module].changed = true;
            true
        }
    }

    pub(crate) fn depending(&self, module: usize) -> Vec<usize> {
        self.nodes[module].depending.iter().cloned().collect()
    }

    pub(crate) fn module_changed(&self, name: &ModuleName) -> bool {
        self.mapping.get(name).map_or(true, |&i| self.nodes[i].changed)
    }
}
