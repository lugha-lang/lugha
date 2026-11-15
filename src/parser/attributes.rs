//! Attributes: `@name` before an item or a method. `@test` before a `fun`
//! marks a test, and `@caller_location` before a `fun` or a method makes its
//! panics report its caller's place; any other name, or either before
//! anything else, is E0210, and the item is still parsed.

use super::{PResult, Parser};
use crate::ast::Item;
use crate::diagnostic::Diagnostic;
use crate::lexer::TokenKind;
use crate::span::Span;

/// A test.
const TEST: &str = "test";

/// A function whose panics report its caller's place.
const CALLER_LOCATION: &str = "caller_location";

impl Parser<'_> {
    /// `@name` (one or more), then the item they go before.
    pub(super) fn attributed_item(&mut self) -> PResult<Item> {
        let mut test: Option<Span> = None;
        let mut located: Option<Span> = None;
        while self.at(&TokenKind::At) {
            let (name, span) = self.attribute()?;
            match name.as_str() {
                TEST => {
                    test.get_or_insert(span);
                }
                CALLER_LOCATION => {
                    located.get_or_insert(span);
                }
                _ => self.unknown_attribute(&name, span),
            }
        }
        let mut item = self.item()?;
        if let Some(span) = test {
            match &mut item {
                Item::Fun(f) => f.test = Some(span),
                _ => {
                    let message = "`@test` goes before a function";
                    self.error(
                        Diagnostic::error("E0210", message, span)
                            .with_help("write `@test` on the line above `fun name() { … }`"),
                    );
                }
            }
        }
        if let Some(span) = located {
            match &mut item {
                Item::Fun(f) => f.caller_location = Some(span),
                _ => {
                    let message = "`@caller_location` goes before a function or a method";
                    self.error(Diagnostic::error("E0210", message, span).with_help(
                        "write `@caller_location` on the line above `fun name(…) { … }`",
                    ));
                }
            }
        }
        Ok(item)
    }

    /// `@caller_location` (the one attribute a method may have) before a
    /// method in an `impl` block; any other is E0210.
    pub(super) fn method_attributes(&mut self) -> PResult<Option<Span>> {
        let mut located = None;
        while self.at(&TokenKind::At) {
            let (name, span) = self.attribute()?;
            match name.as_str() {
                CALLER_LOCATION => {
                    located.get_or_insert(span);
                }
                _ => self.unknown_attribute(&name, span),
            }
        }
        Ok(located)
    }

    /// `@name`: the name and its span.
    fn attribute(&mut self) -> PResult<(String, Span)> {
        let at = self.bump();
        let name = self.ident("attribute name")?;
        Ok((name.name, Span::new(at.start, name.span.end)))
    }

    /// E0210 for an attribute `@name` that isn't one, or not here.
    fn unknown_attribute(&mut self, name: &str, span: Span) {
        let message = format!("unknown attribute `@{name}`");
        self.error(Diagnostic::error("E0210", message, span).with_help(
            "the attributes are `@test` before a function and `@caller_location` before a function or a method",
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::test_util::{errors, program};

    #[test]
    fn test_marks_a_function() {
        assert_eq!(
            program("@test\nfun adds() { }"),
            "(@test fun adds () void (block))"
        );
        assert_eq!(
            program("@test private fun adds() { }"),
            "(@test private fun adds () void (block))"
        );
        // `test` stays an ordinary name.
        assert_eq!(program("fun test() { }"), "(fun test () void (block))");
    }

    #[test]
    fn caller_location_marks_a_function_or_a_method() {
        use crate::ast::Item;
        use crate::parser::test_util::parse_src;
        let items = parse_src("@caller_location fun f() { }\nimpl P { @caller_location fun g(self) { } fun h(self) { } }")
            .unwrap_or_else(|d| panic!("{d:?}"))
            .items;
        let [Item::Fun(f), Item::Impl(i)] = items.as_slice() else {
            panic!("{items:?}")
        };
        assert!(f.caller_location.is_some());
        assert!(i.methods[0].caller_location.is_some() && i.methods[1].caller_location.is_none());
        assert_eq!(
            errors("@caller_location struct S { }"),
            [("E0210", "@caller_location")]
        );
        assert_eq!(
            errors("impl P { @test fun g(self) { } }"),
            [("E0210", "@test")]
        );
    }

    #[test]
    fn unknown_attributes_and_misplaced_tests_are_e0210() {
        assert_eq!(errors("@inline fun f() { }"), [("E0210", "@inline")]);
        assert_eq!(errors("@test struct S { }"), [("E0210", "@test")]);
        assert_eq!(
            errors("@test extern fun abs(x: i32): i32;"),
            [("E0210", "@test")]
        );
    }
}
