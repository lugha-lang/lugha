//! User-defined order through the `lughac` binary: `<` on a struct with
//! `impl Ord`, and on a tuple holding one, calls that block's `cmp`.
//! The prelude's `Ordering` is needed, so this runs the whole pipeline.

use std::path::PathBuf;
use std::process::Command;

const SRC: &str = "struct S { x: i64 } with Eq
impl Ord for S {
    fun cmp(self, o: S): Ordering = if self.x < o.x { Ordering.Less } else { Ordering.Equal };
}
fun main(console: Console) {
    let a = S { x: 1 };
    let b = a < S { x: 2 };
    let c = (a, 1) >= (a, 2);
    console.println(\"{b} {c}\");
}
";

/// A temporary directory holding `order.la`, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("lugha-ordering-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir is writable");
        std::fs::write(dir.join("order.la"), SRC).expect("temp dir is writable");
        Dir(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir must not mask the real failure.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn ordering_a_user_type_calls_its_cmp() {
    let dir = Dir::new();
    let out = Command::new(env!("CARGO_BIN_EXE_lughac"))
        .args(["build", "--emit=ir", "order.la"])
        .current_dir(&dir.0)
        .output()
        .expect("lughac runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ir = String::from_utf8_lossy(&out.stdout);
    let main = &ir[ir.find("define void @lugha_fn_main").expect("main")..];
    let cmp = "@\"lugha_fn_S.cmp@Ord\"(";
    assert!(main.contains(cmp), "{main}");
    // The tuple is ordered through the prelude's block, which compares
    // each element through `$order`: for `S`, its own `cmp`.
    assert!(main.contains("Ord@tuple.cmp<(S, i64)>"), "{main}");
    let order = &ir[ir
        .find("define internal i64 @\"lugha_fn_$prelude/$order<S>\"")
        .expect("the element comparison")..];
    assert!(order.contains(cmp), "{order}");
}
