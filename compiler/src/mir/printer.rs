//! Pretty-printing of MIR for debugging purposes.
use crate::mir::inline::method_weight;
use crate::mir::{BlockId, Method};
use std::fmt::Write;
use types::{Database, MethodId, TypeId};

fn method_name(db: &Database, id: MethodId) -> String {
    format!("{}#{}", id.name(db), id.0,)
}

/// Returns a String containing Dot/graphviz code for visualising the MIR of one
/// or more methods.
pub(crate) fn to_dot(db: &Database, methods: &[&Method]) -> String {
    let mut buffer = String::new();

    buffer.push_str("digraph MIR {\n");

    for (method_index, &method) in methods.iter().enumerate() {
        let _ = writeln!(buffer, "subgraph cluster_MIR_{} {{", method_index);

        buffer.push_str("graph[fontname=\"monospace\", fontsize=10];\n");
        buffer.push_str("node[fontname=\"monospace\", fontsize=10];\n");
        buffer.push_str("edge[fontname=\"monospace\", fontsize=10];\n");

        let rec_name = match method.id.receiver_id(db) {
            TypeId::Class(id) => id.name(db).clone(),
            TypeId::Trait(id) => id.name(db).clone(),
            TypeId::ClassInstance(ins) => ins.instance_of().name(db).clone(),
            TypeId::TraitInstance(ins) => ins.instance_of().name(db).clone(),
            _ => String::new(),
        };

        let name = if rec_name.is_empty() {
            format!("{}()", method_name(db, method.id),)
        } else {
            format!("{}.{}()", rec_name, method_name(db, method.id),)
        };

        let _ = writeln!(
            buffer,
            "label=\"{}\nroot = b{}, inline weight = {}\";",
            name,
            method.body.start_id.0,
            method_weight(db, method),
        );
        let reachable_blocks = method.body.reachable();

        // Render a hidden node that points to the entry block, ensuring the
        // entry block is always placed at the top of the graph.
        let _ = writeln!(
            buffer,
            "  root{}[style=invis,height=0,wight=0,margin=0]",
            method_index
        );
        let _ = writeln!(
            buffer,
            "  root{} -> b{}{} [style=invis,constraint=false]",
            method_index, method_index, method.body.start_id.0
        );

        for (index, block) in method.body.blocks.iter().enumerate() {
            let reachable = reachable_blocks.contains(&BlockId(index));
            let _ = write!(
                buffer,
                "  b{}{}[shape=\"none\" label=\
                <<table border=\"0\" cellborder=\"1\" cellspacing=\"0\">",
                method_index, index
            );

            let _ = write!(
                buffer,
                "<tr>\
                <td colspan=\"2\" bgcolor=\"{}\" align=\"center\">\
                b{}{}\
                </td>\
                </tr>",
                if reachable { "CornSilk" } else { "LightGray" },
                index,
                if reachable { "" } else { " (unreachable)" }
            );

            buffer.push_str(
                "<tr>\
                <td bgcolor=\"#f2f2f2\">Instruction</td>\
                <td bgcolor=\"#f2f2f2\">Line</td>\
                </tr>",
            );

            for ins in &block.instructions {
                let _ = write!(
                    buffer,
                    "<tr><td>{}</td><td>{}</td></tr>",
                    ins.format(db)
                        .replace('&', "&amp;")
                        .replace('>', "&gt;")
                        .replace('<', "&lt;"),
                    ins.location().line_start,
                );
            }

            buffer.push_str("</table>>];\n");
        }

        for index in 0..method.body.blocks.len() {
            for edge in method.body.successors(BlockId(index)) {
                let _ = writeln!(
                    buffer,
                    "  b{}{} -> b{}{}",
                    method_index, index, method_index, edge.0
                );
            }
        }

        buffer.push_str("}\n");
    }

    buffer.push_str("}\n");
    buffer
}
