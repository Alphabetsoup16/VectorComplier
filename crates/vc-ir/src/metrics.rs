//! Tree metrics on instruction bodies (for tooling / manifests — not semantic validation).

use crate::ast::Instr;

/// Counts instructions including nested `block` / `if_else` bodies (same bound as validation uses).
pub fn instr_tree_node_count(instrs: &[Instr]) -> usize {
    instrs.iter().fold(0usize, |acc, i| {
        acc + 1
            + match i {
                Instr::Block { body, .. } => instr_tree_node_count(body),
                Instr::IfElse {
                    then_body,
                    else_body,
                    ..
                } => instr_tree_node_count(then_body) + instr_tree_node_count(else_body),
                _ => 0,
            }
    })
}

/// Deepest nested `block` / `if_else` depth in `instrs` (0 if no structured control).
pub fn max_control_nesting_depth(instrs: &[Instr]) -> usize {
    instrs
        .iter()
        .map(|i| match i {
            Instr::Block { body, .. } => 1 + max_control_nesting_depth(body),
            Instr::IfElse {
                then_body,
                else_body,
                ..
            } => 1 + max_control_nesting_depth(then_body).max(max_control_nesting_depth(else_body)),
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}
