//! The owner check (`LUGHA_RC_CHECK=1`): every object not marked shared
//! remembers the task that made it, and another task touching or marking
//! it ends the program. Every concurrency program must pass it, printing
//! and exiting as it always does: a value reaching a second task unmarked
//! would be a use after free once tasks run on several threads.

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use support::runner::{
    Case, DEFAULT_TIMEOUT, Mode, compare, discover, each_case, format_malformed, format_report,
    run_case,
};

const CHECK: (&str, &str) = ("LUGHA_RC_CHECK", "1");

#[test]
fn concurrency_programs_pass_the_owner_check() {
    let workdir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lughac = Path::new(env!("CARGO_BIN_EXE_lughac"));
    let root = workdir.join("tests/programs/concurrency");
    let cases = match discover(&root) {
        Ok(cases) => cases,
        Err(malformed) => panic!("\n{}", format_malformed(&malformed)),
    };
    let cases: Vec<Case> = cases
        .into_iter()
        .filter(|case| matches!(case.mode, Mode::Run { .. }))
        .map(|mut case| {
            let (key, value) = CHECK;
            case.extras.envvars.push((key.into(), value.into()));
            case
        })
        .collect();
    let failures: Vec<_> = each_case(&cases, |case| {
        let outcome = run_case(lughac, workdir, case, DEFAULT_TIMEOUT);
        (case.name.clone(), compare(case, &outcome))
    })
    .into_iter()
    .filter(|(_, mismatches)| !mismatches.is_empty())
    .collect();
    if !failures.is_empty() {
        panic!("\n{}", format_report(&failures, cases.len()));
    }
}

/// A program in a temporary folder of its own, removed on drop.
struct Program(PathBuf);

impl Program {
    fn new(source: &str) -> Program {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("lugha-rcc-{}-{n}", std::process::id()));
        // A leftover from an earlier run would change what the test sees.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("main.la"), source).unwrap();
        Program(dir)
    }

    fn run(&self, opt: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lughac"))
            .args(["run", "main.la", opt])
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .env(CHECK.0, CHECK.1)
            .output()
            .expect("lughac runs")
    }

    /// Under the check, at -O0 and -O2: prints `stdout` and exits 0.
    fn passes(&self, stdout: &str) {
        for opt in ["-O0", "-O2"] {
            let out = self.run(opt);
            assert_eq!(
                (out.status.code(), text(&out.stdout).as_str()),
                (Some(0), stdout),
                "{opt}, stderr: {}",
                text(&out.stderr)
            );
        }
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_joined_result_is_marked_by_its_task() {
    Program::new(
        "fun word(n: i64): string = \"word {n}\";\n\
         fun main(console: Console) {\n\
             let r = scope s {\n\
                 let t = s.spawn(word(3));\n\
                 let a = t.join().unwrap();\n\
                 let b = t.join().unwrap();\n\
                 \"{a}, {b}\"\n\
             };\n\
             console.println(r.unwrap());\n\
         }\n",
    )
    .passes("word 3, word 3\n");
}

#[test]
fn an_actor_marks_its_answer() {
    Program::new(
        "actor Store {\n\
             items: string[],\n\
             fun add(mut self, s: string) { self.items.push(s); }\n\
             fun all(self): string[] = self.items;\n\
         }\n\
         fun main(console: Console, root: Scope) {\n\
             let store = root.spawn(Store { items: [] });\n\
             store.add(\"a\");\n\
             store.add(\"b\");\n\
             let all = store.all();\n\
             console.println(all.len);\n\
             store.add(\"c\");\n\
             console.println(store.all().len);\n\
         }\n",
    )
    .passes("2\n3\n");
}

#[test]
fn constants_are_read_by_every_task() {
    Program::new(
        "const NAMES: string[] = [\"ann\", \"bo\"];\n\
         const GREETING: string = \"hi {NAMES[1]}\";\n\
         fun names(): string[] {\n\
             let all = NAMES;\n\
             all\n\
         }\n\
         fun greet(): string = GREETING;\n\
         fun main(console: Console) {\n\
             let r = scope s {\n\
                 let a = s.spawn(names());\n\
                 let b = s.spawn(greet());\n\
                 \"{a.join().unwrap().len} {b.join().unwrap()}\"\n\
             };\n\
             console.println(r.unwrap());\n\
         }\n",
    )
    .passes("2 hi bo\n");
}
