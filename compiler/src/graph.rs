//! Graph data structures.

/// A graph that supports insertions and retrievals but not removals.
pub(crate) struct Graph<T> {
    pub(crate) nodes: Vec<Node<T>>,
}

impl<T> Graph<T> {
    pub(crate) fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub(crate) fn add(&mut self, value: T) -> NodeId {
        let id = self.nodes.len();

        self.nodes.push(Node::new(value));
        NodeId(id)
    }

    pub(crate) fn get(&self, node: NodeId) -> &Node<T> {
        &self.nodes[node.0]
    }

    pub(crate) fn get_mut(&mut self, node: NodeId) -> &mut Node<T> {
        &mut self.nodes[node.0]
    }

    pub(crate) fn add_edge(&mut self, from: NodeId, to: NodeId) {
        self.get_mut(from).outgoing.push(to);
        self.get_mut(to).incoming.push(from);
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub(crate) struct NodeId(pub(crate) usize);

pub(crate) struct Node<T> {
    pub(crate) value: T,
    pub(crate) incoming: Vec<NodeId>,
    pub(crate) outgoing: Vec<NodeId>,
}

impl<T> Node<T> {
    pub(crate) fn new(value: T) -> Self {
        Self { value, incoming: Vec::new(), outgoing: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        let mut graph = Graph::new();

        graph.add(1);
        graph.add(2);

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[0].value, 1);
        assert_eq!(graph.nodes[1].value, 2);
    }

    #[test]
    fn test_add_edge() {
        let mut graph = Graph::new();
        let n1 = graph.add(1);
        let n2 = graph.add(2);

        graph.add_edge(n1, n2);

        assert!(graph.nodes[0].outgoing.contains(&n2));
        assert!(graph.nodes[1].incoming.contains(&n1));
    }

    #[test]
    fn test_get() {
        let mut graph = Graph::new();
        let node = graph.add(1);

        graph.add(2);

        assert_eq!(graph.get(node).value, 1);
    }

    #[test]
    fn test_get_mut() {
        let mut graph = Graph::new();
        let node = graph.add(1);

        graph.add(2);

        assert_eq!(graph.get_mut(node).value, 1);
    }
}
