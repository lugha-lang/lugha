//! The debug form of interface values: `lugha_debug_<I>` compares the
//! value's identity slot with each concrete type the program has a table
//! of for `I`, and shows the box's value in that type's debug form. Its body is
//! built at the end of lowering, when every table is known, so dispatch
//! tables themselves never change.

use inkwell::IntPredicate;

use super::lower::{Lowerer, POSITIONED};

impl<'ctx> Lowerer<'ctx> {
    /// The bodies of every interface show helper declared, after
    /// `finish_upcasts`. Building one may declare another (an interface
    /// value inside a box), so this repeats until none is left.
    pub(super) fn finish_shows(&mut self) {
        let mut built = 0;
        while built < self.shows.len() {
            let (interface, function) = self.shows[built].clone();
            built += 1;
            self.function = Some(function);
            let entry = self.context.append_basic_block(function, "entry");
            self.builder.position_at_end(entry);
            let pair = function
                .get_nth_param(0)
                .expect("one parameter")
                .into_struct_value();
            let (table, boxed) = self.parts(pair);
            let identity = self.slot(table, 0);
            for concrete in self.upcasts.concretes(&interface) {
                let same = self
                    .builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        identity,
                        self.type_id(&concrete),
                        "is.type",
                    )
                    .expect(POSITIONED);
                let (this, next) = (self.append("show.type"), self.append("show.next"));
                self.builder
                    .build_conditional_branch(same, this, next)
                    .expect(POSITIONED);
                self.builder.position_at_end(this);
                let value = self.load_element(boxed, &concrete);
                let text = self.part(value, &concrete);
                self.builder.build_return(Some(&text)).expect(POSITIONED);
                self.builder.position_at_end(next);
            }
            // A value always has one of the program's tables.
            self.builder.build_unreachable().expect(POSITIONED);
        }
        self.function = None;
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::test_util::ir;

    #[test]
    fn values_show_through_their_blocks_and_impls() {
        let ir = ir("struct Point { x: i64, s: string }\n\
            struct M { c: i64 }\nimpl Display for M { fun show(self): string = \"m\"; }\n\
            fun main(console: Console) { console.println(Point { x: 1, s: \"a\" }.debug()); console.println(M { c: 1 }); }");
        for needle in [
            "@lugha_debug_5Point(",
            "@\"lugha_fn_$prelude/Debug@struct.debug<Point>\"",
            "@lugha_rt_str_quote",
            "@\"lugha_fn_M.show@Display\"",
        ] {
            assert!(ir.contains(needle), "{needle}: {ir}");
        }
    }

    #[test]
    fn interface_values_show_by_identity() {
        let ir = ir("interface Named { fun name(self): string; }\n\
            struct Dog { age: i64 }\nimpl Named for Dog { fun name(self): string = \"d\"; }\n\
            fun dump<T>(console: Console, x: T) { console.println(x.debug()); }\n\
            fun main(console: Console) { let n: Named = Dog { age: 1 }; dump(console, n); }");
        let f = &ir[ir.find("define internal ptr @lugha_debug_5Named").unwrap()..];
        assert!(
            f.contains("@lugha_type_Dog") && f.contains("@lugha_debug_3Dog"),
            "{f}"
        );
    }

    #[test]
    fn plain_values_print_as_before() {
        let ir = ir(
            "fun main(console: Console) { console.println(1); console.println(\"s\"); let t = \"{2.5}\"; }",
        );
        assert!(!ir.contains("lugha_debug_"), "{ir}");
    }
}
