//! Which `Ref` boxes may be part of a cycle: those whose contents can
//! reach another `Ref`, or a function or interface value, which may hide
//! one. Codegen makes such a box with the runtime's cell count, so its
//! task's cycle collector sees it; every other box is made as before.

use super::enums::variants;
use super::lower::Lowerer;
use crate::check::{Enums, Structs, Type};

/// True if a value of type `contents`, held in a `Ref`, can lead back to a
/// `Ref`: through fields, payloads, elements, `Option`s and tuples, or a
/// function or interface value, whose type doesn't show what it holds.
pub(super) fn cycle_prone(contents: &Type, structs: &Structs, enums: &Enums) -> bool {
    reaches_ref(contents, structs, enums, &mut Vec::new())
}

fn reaches_ref(ty: &Type, structs: &Structs, enums: &Enums, path: &mut Vec<String>) -> bool {
    match ty {
        Type::Ref(_) | Type::Fun(..) | Type::Own(_) | Type::Interface(..) => true,
        Type::Array(element) => reaches_ref(element, structs, enums, path),
        Type::Struct(..) | Type::Enum(..) | Type::Option(_) => {
            let name = ty.to_string();
            // A type leading back to itself adds nothing new on the way.
            if path.contains(&name) {
                return false;
            }
            path.push(name.clone());
            let parts: Vec<Type> = match ty {
                Type::Struct(..) => structs
                    .get(&name)
                    .map(|fields| fields.iter().map(|(_, t)| t.clone()).collect())
                    .unwrap_or_default(),
                _ => variants(ty, enums)
                    .into_iter()
                    .flat_map(|(_, p)| p)
                    .collect(),
            };
            let reaches = parts.iter().any(|p| reaches_ref(p, structs, enums, path));
            path.pop();
            reaches
        }
        // Numbers, strings, `()`, handles, and what crosses between tasks
        // (channel ends, tasks, actors), which can never hold a `Ref`.
        _ => false,
    }
}

/// True if the program, by its expressions' `types`, may make a cell: a
/// `Ref` of a cycle-prone type, or of a type parameter, which an instance
/// may fill with one. Such a program counts down to its scheduling points,
/// where collection runs, even without tasks.
pub(super) fn makes_cells(types: &[Type], structs: &Structs, enums: &Enums) -> bool {
    types.iter().any(|ty| match ty {
        Type::Ref(inner) => matches!(**inner, Type::Param(_)) || cycle_prone(inner, structs, enums),
        _ => false,
    })
}

impl Lowerer<'_> {
    /// True if a `Ref<contents>` box is made as a cell.
    pub(super) fn cycle_prone(&self, contents: &Type) -> bool {
        cycle_prone(contents, &self.structs, &self.enums)
    }
}

#[cfg(test)]
mod tests {
    /// Whether `Ref.new(…)` in `body` makes a cell, from the IR.
    fn makes_cell(decls: &str, body: &str) -> bool {
        let ir = crate::codegen::test_util::ir(&format!("{decls}\nfun main() {{ {body} }}"));
        ir.contains("call ptr @lugha_rt_rc_cell_alloc(")
    }

    #[test]
    fn only_boxes_that_can_reach_a_ref_are_cells() {
        assert!(!makes_cell("", "let r = Ref.new(1);"));
        assert!(!makes_cell("", "let r = Ref.new([\"a\"]);"));
        assert!(!makes_cell("", "let r = Ref.new(Map { \"a\": 1 });"));
        assert!(makes_cell("", "let r = Ref.new(Some(Ref.new(1)));"));
        assert!(makes_cell(
            "struct Node { name: string, next: Option<Ref<Node>> }",
            "let r = Ref.new(Node { name: \"a\", next: None });"
        ));
        assert!(makes_cell("", "let r = Ref.new(fun(): i64 = 1);"));
        assert!(makes_cell(
            "interface Shape { fun area(self): f64; }",
            "let r: Ref<Option<Shape>> = Ref.new(None);"
        ));
        // A struct of plain data, however deep, isn't.
        assert!(!makes_cell(
            "struct P { x: i64, tags: string[] }\nstruct Q { p: P, ps: P[] }",
            "let r = Ref.new(Q { p: P { x: 1, tags: [] }, ps: [] });"
        ));
    }

    #[test]
    fn only_programs_that_may_make_cells_count_down() {
        let ir = crate::codegen::test_util::ir;
        let plain = ir("fun main() { let r = Ref.new(1); let mut i = 0; while i < 3 { i += 1; } }");
        assert!(!plain.contains("lugha_rt_task_preempt"), "{plain}");
        let cells = ir("fun main() { let r = Ref.new(Some(Ref.new(1))); }");
        assert!(
            cells.contains("call void @lugha_rt_task_preempt()"),
            "{cells}"
        );
        // A box of a type parameter may be filled with a cell.
        let generic =
            ir("fun boxed<T>(x: T): Ref<T> = Ref.new(x);\nfun main() { let r = boxed(1); }");
        assert!(
            generic.contains("call void @lugha_rt_task_preempt()"),
            "{generic}"
        );
    }

    #[test]
    fn a_plain_box_keeps_the_inline_count() {
        let ir = crate::codegen::test_util::ir("fun main() { let r = Ref.new(1); r.set(2); }");
        assert!(!ir.contains("lugha_rt_rc_cell_alloc"), "{ir}");
    }
}
