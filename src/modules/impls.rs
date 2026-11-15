//! The methods of `impl I for T { … }` for another module's type. The
//! parser names a block's methods apart from the type's own, `area@Shape`
//! (`ImplDecl::tag`); a block for another module's type
//! (`impl Shape for geo.Circle`) also needs this module's path,
//! `area@Shape@shapes`, so two modules' `impl Shape for …` never share a
//! name.

use crate::ast::{Item, Program};

/// Adds `module`, the path of `program` joined with `.`, to the names of
/// the methods of every `impl I for T` there whose type is another
/// module's.
pub(crate) fn name_impls(program: &mut Program, module: &str) {
    for item in &mut program.items {
        let Item::Impl(block) = item else { continue };
        if block.interface.is_none() || block.module.is_none() {
            continue;
        }
        let suffix = format!("@{}", module.replace('.', "/"));
        for method in &mut block.methods {
            method.name.name.push_str(&suffix);
        }
        block.tag = format!("{}{suffix}", block.tag).into();
    }
}

/// The name a program wrote for a method's `name`, which may be an
/// interface's `area@Shape`.
pub fn item_name(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renamed(src: &str, module: &str) -> Vec<(String, Vec<String>)> {
        let (tokens, _) = crate::lexer::lex(src).expect("lexes");
        let (mut program, _) = crate::parser::parse(&tokens).expect("parses");
        name_impls(&mut program, module);
        program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Impl(b) => Some((
                    b.tag.to_string(),
                    b.methods.iter().map(|m| m.name.name.clone()).collect(),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_interfaces_methods_take_its_name() {
        let src = "impl Circle { fun area(self): f64 = 1.0; }\n\
                   impl Shape for Circle { fun area(self): f64 = 2.0; }\n\
                   impl geo.From< geo.Point > for Circle { fun from(p: geo.Point): Circle = c; }\n\
                   impl From<(i64, fun(i64): bool)> for Circle { fun from(p: (i64, fun(i64): bool)): Circle = c; }\n";
        assert_eq!(
            renamed(src, ""),
            [
                (String::new(), vec!["area".to_string()]),
                ("Shape".to_string(), vec!["area@Shape".to_string()]),
                (
                    "geo/From<geo/Point>".to_string(),
                    vec!["from@geo/From<geo/Point>".to_string()]
                ),
                (
                    "From<(i64,fun(i64):bool)>".to_string(),
                    vec!["from@From<(i64,fun(i64):bool)>".to_string()]
                ),
            ]
        );
    }

    #[test]
    fn another_modules_type_adds_this_modules_path() {
        let src = "impl Shape for geo.Circle { fun area(self): f64 = 2.0; }\n";
        assert_eq!(
            renamed(src, "app.shapes"),
            [(
                "Shape@app/shapes".to_string(),
                vec!["area@Shape@app/shapes".to_string()]
            )]
        );
        assert_eq!(item_name("area@Shape@app/shapes"), "area");
        assert_eq!(item_name("area"), "area");
    }
}
