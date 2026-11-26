//! Passing `extern struct`s to and from C on Windows x64, as Microsoft's
//! x64 calling convention says and clang lowers it (MinGW's too):
//!
//! - a struct of exactly 1, 2, 4 or 8 bytes is one integer of that width,
//!   in a general register or its stack slot, even when its fields are
//!   floats;
//! - any other argument is a plain pointer to a copy the caller makes (no
//!   `byval`), so C may change it without the Lugha value changing;
//! - any other result comes back through `sret`, a hidden pointer the
//!   caller passes first.
//!
//! Scalars and pointers need nothing: LLVM passes them by Windows' rules.

use crate::check::{Enums, Structs, Type};

use super::c_abi::Piece;
use super::c_calls::{Arg, CKind, CSig, Ret};
use super::lower::Lowerer;
use super::structs::layout;

/// How a struct travels on Windows x64.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WinPassing {
    /// One integer of this many bits: the struct's whole size.
    Int(u32),
    /// By a pointer to a copy (argument), or `sret` (result).
    Indirect,
}

/// How the extern struct `ty` is passed on Windows x64.
pub(super) fn classify_win(ty: &Type, structs: &Structs, enums: &Enums) -> WinPassing {
    let (size, _) = layout(ty, structs, enums);
    assert!(size > 0, "the checker refuses empty extern structs (E0437)");
    match size {
        1 | 2 | 4 | 8 => WinPassing::Int(u32::try_from(size * 8).expect("64 bits at most")),
        _ => WinPassing::Indirect,
    }
}

impl<'ctx> Lowerer<'ctx> {
    /// The lowered signature of a C function taking `kinds` and returning
    /// `ret`, by Windows x64's rules.
    pub(super) fn c_sig_win(&self, kinds: &[CKind], ret: &Type) -> CSig<'ctx> {
        let classified = |ty: &Type| classify_win(ty, &self.structs, &self.enums);
        let ret = match ret {
            Type::Unit => Ret::Void,
            Type::Struct(..) => match classified(ret) {
                WinPassing::Indirect => Ret::Memory(ret.clone()),
                WinPassing::Int(bits) => {
                    Ret::Split(ret.clone(), vec![self.piece_type(Piece::Int(bits))])
                }
            },
            other => Ret::One(self.c_result_type(other), other.clone()),
        };
        let ptr = self.context.ptr_type(Default::default()).into();
        let args = kinds
            .iter()
            .map(|kind| match kind {
                CKind::Pointer => Arg::One(ptr, None),
                CKind::Value(ty @ Type::Struct(..)) => match classified(ty) {
                    WinPassing::Indirect => Arg::Indirect(ty.clone()),
                    WinPassing::Int(bits) => {
                        Arg::Split(ty.clone(), vec![self.piece_type(Piece::Int(bits))])
                    }
                },
                CKind::Value(ty) => Arg::One(self.c_result_type(ty), Some(ty.clone())),
            })
            .collect();
        self.c_sig_of(args, ret)
    }
}

#[cfg(test)]
mod tests {
    use super::{WinPassing, classify_win};
    use crate::check::{Enums, Structs, Type};

    /// One struct per name, with these fields.
    fn structs(defs: &[(&str, &[Type])]) -> Structs {
        defs.iter()
            .map(|(name, fields)| {
                let named = fields
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (format!("f{i}"), t.clone()))
                    .collect();
                (name.to_string(), named)
            })
            .collect()
    }

    fn pass(name: &str, defs: &[(&str, &[Type])]) -> WinPassing {
        classify_win(
            &Type::Struct(name.into(), Vec::new()),
            &structs(defs),
            &Enums::new(),
        )
    }

    use Type::{F32, F64, I16, I64, U8};
    use WinPassing::{Indirect, Int};

    #[test]
    fn structs_of_one_two_four_or_eight_bytes_are_one_integer() {
        assert_eq!(pass("B", &[("B", &[U8])]), Int(8));
        assert_eq!(pass("S", &[("S", &[U8, U8])]), Int(16));
        assert_eq!(pass("C", &[("C", &[U8, U8, U8, U8])]), Int(32));
        assert_eq!(pass("H", &[("H", &[I16, I16, I16, I16])]), Int(64));
    }

    #[test]
    fn floats_in_a_small_struct_go_as_an_integer_too() {
        assert_eq!(pass("V2", &[("V2", &[F32, F32])]), Int(64));
        assert_eq!(pass("D", &[("D", &[F64])]), Int(64));
        assert_eq!(pass("F", &[("F", &[F32])]), Int(32));
    }

    #[test]
    fn every_other_size_goes_by_pointer() {
        assert_eq!(pass("T", &[("T", &[U8, U8, U8])]), Indirect);
        assert_eq!(pass("V3", &[("V3", &[F32, F32, F32])]), Indirect);
        assert_eq!(pass("P", &[("P", &[F64, F64])]), Indirect);
        assert_eq!(pass("B3", &[("B3", &[I64, I64, I64])]), Indirect);
    }

    const SHAPES: &str = "extern struct V2 { x: f32, y: f32 }
extern struct Big { a: i64, b: i64, c: i64 }
extern struct Rgba { r: u8, g: u8, b: u8, a: u8 }
extern fun len(v: V2): f32;
extern fun big(b: Big): i64;
extern fun make(): Big;
extern fun tint(c: Rgba): Rgba;
fun main() {
    debug(len(V2 { x: 1.0, y: 2.0 }));
    debug(big(make()));
    debug(tint(Rgba { r: 1, g: 2, b: 3, a: 4 }).a);
}
";

    #[test]
    fn signatures_follow_windows_rules() {
        use crate::codegen::Target;
        use crate::codegen::test_util::ir_for;
        let win = ir_for(SHAPES, Target::X86_64Windows);
        // Two floats in eight bytes travel as one integer, not in SSE.
        assert!(win.contains("declare float @len(i64)"), "{win}");
        // A big argument is a plain pointer to the caller's copy: no `byval`.
        assert!(win.contains("declare i64 @big(ptr)"), "{win}");
        assert!(win.contains("declare void @make(ptr sret("), "{win}");
        assert!(win.contains("declare i32 @tint(i32)"), "{win}");
        assert!(!win.contains("byval("), "{win}");
    }
}
