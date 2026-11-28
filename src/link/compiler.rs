//! The C compiler lughac runs: `cc` for the
//! machine it runs on, `zig cc -target …` for any other (`zig`), or the
//! words of `$LUGHA_CC` for every target — a program and its first
//! arguments, split on spaces and run from an argument list, never through
//! a shell, as `$PKG_CONFIG` is. One per build, so every compile, the link
//! and the object cache's key agree.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use super::{LinkError, ndk, zig};

/// The oldest macOS a Mac's own build runs on, as `cc` takes it.
pub const MAC_MINIMUM: &str = "-mmacosx-version-min=14.4";

/// A C compiler: a program and the arguments that go before lughac's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiler {
    program: OsString,
    args: Vec<OsString>,
    /// Set when the words came from `$LUGHA_CC`, for messages.
    from_env: bool,
    /// Arguments only the link takes, after `args`: zig's unwinder.
    link_args: Vec<OsString>,
    /// Environment variables every run gets: where zig keeps its cache.
    envs: Vec<(OsString, OsString)>,
    /// What `zig version` said, for the object cache's key; empty for `cc`.
    version: String,
}

impl Default for Compiler {
    /// `cc`, with no arguments.
    fn default() -> Compiler {
        Compiler {
            program: OsString::from("cc"),
            args: Vec::new(),
            from_env: false,
            link_args: Vec::new(),
            envs: Vec::new(),
            version: String::new(),
        }
    }
}

impl Compiler {
    /// The compiler `$LUGHA_CC` names, or `cc` when it is unset or blank.
    ///
    /// # Errors
    ///
    /// [`LinkError::BadCompiler`] when `$LUGHA_CC` isn't UTF-8, since its
    /// words couldn't be split reliably.
    pub fn from_env() -> Result<Compiler, LinkError> {
        choose(std::env::var_os("LUGHA_CC").as_deref(), "cc")
    }

    /// The compiler for the triple `target`: `$LUGHA_CC`'s when it is set;
    /// else `cc` when `native` (the machine lughac runs on); else the NDK's
    /// `clang` for Android; else zig, once `zig version` says it is new
    /// enough.
    ///
    /// # Errors
    ///
    /// As [`Compiler::from_env`], [`ndk::find`]'s for Android, and
    /// [`zig::check`]'s for another foreign target.
    pub fn for_target(target: &str, native: bool) -> Result<Compiler, LinkError> {
        let mut chosen = Compiler::from_env()?;
        if chosen.from_env {
            return Ok(chosen);
        }
        if native {
            // A Mac's own build runs on any Mac zig's would: 14.4 brought
            // `os_sync_wait_on_address`, which the runtime waits with.
            if target.contains("-apple-") {
                chosen.args.push(OsString::from(MAC_MINIMUM));
            }
            return Ok(chosen);
        }
        if target.ends_with("-android") {
            let (clang, version) = ndk::find(target, &|name| std::env::var_os(name))?;
            return Ok(Compiler::ndk(target, clang, version));
        }
        let version = zig::check(target)?;
        let cache = crate::modules::cache::root(|n| std::env::var_os(n)).map(|r| r.join("zig"));
        Ok(Compiler::zig(target, version, cache))
    }

    /// `zig cc` for `target`, whose `zig version` said `version`, keeping
    /// its cache in `cache` when there is one.
    fn zig(target: &str, version: String, cache: Option<PathBuf>) -> Compiler {
        // The driver offers only targets the table has.
        let triple = zig::zig_triple(target).expect("every --target has a zig spelling");
        // The runtime is built for the baseline CPU, as `cc` builds it.
        let args = ["cc", "-target", triple, "-mcpu=baseline"];
        // zig's cache goes in lughac's, so a read-only HOME still works.
        let envs = cache.map_or_else(Vec::new, |dir| {
            ["ZIG_GLOBAL_CACHE_DIR", "ZIG_LOCAL_CACHE_DIR"]
                .map(|name| (OsString::from(name), dir.clone().into_os_string()))
                .to_vec()
        });
        Compiler {
            program: OsString::from("zig"),
            args: args.map(OsString::from).to_vec(),
            from_env: false,
            link_args: zig::link_args(target),
            envs,
            version,
        }
    }

    /// The NDK's `clang` at `clang` for the Android triple `target`, whose
    /// `--version` said `version`.
    fn ndk(target: &str, clang: PathBuf, version: String) -> Compiler {
        Compiler {
            program: clang.into_os_string(),
            args: ndk::args(target),
            from_env: false,
            link_args: ndk::link_args(),
            envs: Vec::new(),
            version,
        }
    }

    /// The compiler `text` names: its first space-separated word is the
    /// program, the rest its first arguments; blank text is `cc`. Quotes,
    /// `$`, `;` and the like are ordinary characters, as no shell runs it.
    pub fn from_words(text: &str) -> Compiler {
        let mut words = text
            .split(' ')
            .filter(|w| !w.is_empty())
            .map(OsString::from);
        match words.next() {
            None => Compiler::default(),
            Some(program) => Compiler {
                program,
                args: words.collect(),
                from_env: true,
                ..Compiler::default()
            },
        }
    }

    /// A command running the compiler, its first arguments already added.
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command.envs(self.envs.iter().map(|(k, v)| (k, v)));
        command
    }

    /// A command running the compiler as the linker: [`Compiler::command`]
    /// with the arguments only a link takes.
    pub fn link_command(&self) -> Command {
        let mut command = self.command();
        command.args(&self.link_args);
        command
    }

    /// The words and zig's version, for the object cache's key: zig's
    /// `cc --version` names its clang, which two zig releases can share.
    pub fn key_words(&self) -> String {
        format!("{}\n{}", self.words(), self.version)
    }

    /// The words, for the object cache's key: two commands printing the
    /// same `--version`, such as `zig cc` for two targets, still differ.
    pub fn words(&self) -> String {
        let mut words = vec![self.program.to_string_lossy().into_owned()];
        words.extend(self.args.iter().map(|a| a.to_string_lossy().into_owned()));
        words.join(" ")
    }

    /// The compiler as messages name it: `` `cc` ``, or
    /// `` `zig cc` (from $LUGHA_CC) ``.
    pub fn name(&self) -> String {
        match self.from_env {
            true => format!("`{}` (from $LUGHA_CC)", self.words()),
            false => format!("`{}`", self.words()),
        }
    }
}

/// The compiler a `$LUGHA_CC` of `value` names, or `fallback` when it is
/// unset or blank.
fn choose(value: Option<&std::ffi::OsStr>, fallback: &str) -> Result<Compiler, LinkError> {
    let fallback = || Compiler {
        program: OsString::from(fallback),
        ..Compiler::default()
    };
    match value.map(|v| v.to_str()) {
        None => Ok(fallback()),
        Some(Some(text)) if text.trim_matches(' ').is_empty() => Ok(fallback()),
        Some(Some(text)) => Ok(Compiler::from_words(text)),
        Some(None) => Err(LinkError::BadCompiler),
    }
}

#[cfg(test)]
mod tests {
    use super::{Compiler, choose};
    use std::ffi::OsStr;

    #[test]
    fn the_fallback_is_used_unless_lugha_cc_names_one() {
        assert_eq!(choose(None, "cc").unwrap(), Compiler::default());
        assert_eq!(
            choose(Some(OsStr::new("  ")), "cc").unwrap(),
            Compiler::default()
        );
        let zig = choose(Some(OsStr::new("zig cc -target aarch64-linux-gnu")), "cc").unwrap();
        assert_eq!(zig.words(), "zig cc -target aarch64-linux-gnu");
        assert!(
            zig.link_args.is_empty(),
            "$LUGHA_CC's words are all it gets"
        );
    }

    fn strings<'a>(items: impl Iterator<Item = &'a std::ffi::OsStr>) -> Vec<String> {
        items.map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn a_macs_own_build_runs_on_any_mac_from_14_4() {
        // Unless `$LUGHA_CC` names a compiler, whose words are all it gets.
        if std::env::var_os("LUGHA_CC").is_some() {
            return;
        }
        let mac = Compiler::for_target("aarch64-apple-darwin", true).unwrap();
        assert_eq!(mac.words(), format!("cc {}", super::MAC_MINIMUM));
        let linux = Compiler::for_target("x86_64-unknown-linux-gnu", true).unwrap();
        assert_eq!(linux.words(), "cc");
    }

    #[test]
    fn an_android_target_compiles_with_the_ndks_clang() {
        let clang = std::path::PathBuf::from("/ndk/bin/clang");
        let ndk = Compiler::ndk("aarch64-linux-android", clang, "clang 21".into());
        let compile = ndk.command();
        assert_eq!(compile.get_program(), "/ndk/bin/clang");
        assert_eq!(
            strings(compile.get_args()),
            ["--target=aarch64-linux-android29"]
        );
        assert_eq!(
            strings(ndk.link_command().get_args()),
            [
                "--target=aarch64-linux-android29",
                "-Wl,-z,max-page-size=16384"
            ]
        );
        assert!(ndk.key_words().contains("clang 21"), "{}", ndk.key_words());
    }

    #[test]
    fn a_foreign_target_compiles_with_zig_for_its_triple() {
        let cache = std::path::PathBuf::from("/c/lugha/zig");
        let zig = Compiler::zig("x86_64-unknown-linux-musl", "0.17.1".into(), Some(cache));
        assert_eq!(
            zig.name(),
            "`zig cc -target x86_64-linux-musl -mcpu=baseline`"
        );
        let compile = zig.command();
        assert_eq!(compile.get_program(), "zig");
        assert_eq!(
            strings(compile.get_args()),
            ["cc", "-target", "x86_64-linux-musl", "-mcpu=baseline"]
        );
        let envs: Vec<_> = compile
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(|v| v.to_owned())))
            .collect();
        assert_eq!(envs.len(), 2);
        assert!(
            envs.iter()
                .all(|(_, v)| v.as_deref() == Some(std::ffi::OsStr::new("/c/lugha/zig")))
        );
        assert_eq!(
            strings(zig.link_command().get_args()),
            [
                "cc",
                "-target",
                "x86_64-linux-musl",
                "-mcpu=baseline",
                "-lunwind",
                "-static"
            ]
        );
        assert_eq!(
            zig.key_words(),
            "zig cc -target x86_64-linux-musl -mcpu=baseline\n0.17.1"
        );
        let arm = Compiler::zig("aarch64-unknown-linux-gnu", "0.17.1".into(), None);
        assert_eq!(arm.command().get_envs().count(), 0);
        assert_eq!(
            strings(arm.link_command().get_args()).last().unwrap(),
            "-lunwind"
        );
    }

    #[test]
    fn cc_links_with_nothing_added() {
        let cc = Compiler::default();
        assert_eq!(cc.link_command().get_args().count(), 0);
        assert_eq!(cc.command().get_envs().count(), 0);
    }

    // Built from bytes that aren't UTF-8, which only Unix's names allow.
    #[cfg(unix)]
    #[test]
    fn a_lugha_cc_that_isnt_utf8_is_refused() {
        use std::os::unix::ffi::OsStrExt;
        let bad = OsStr::from_bytes(b"cc\xff");
        assert!(matches!(
            choose(Some(bad), "cc"),
            Err(super::LinkError::BadCompiler)
        ));
    }

    fn parts(text: &str) -> (String, Vec<String>) {
        let c = Compiler::from_words(text);
        let program = c.program.to_string_lossy().into_owned();
        let args = c
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        (program, args)
    }

    #[test]
    fn blank_text_is_cc() {
        for text in ["", "   "] {
            assert_eq!(Compiler::from_words(text), Compiler::default(), "{text:?}");
        }
        assert_eq!(parts(""), ("cc".to_string(), vec![]));
        assert_eq!(Compiler::default().name(), "`cc`");
    }

    #[test]
    fn the_first_word_is_the_program_and_the_rest_come_first() {
        assert_eq!(parts("clang"), ("clang".to_string(), vec![]));
        assert_eq!(
            parts("zig  cc -target  x86_64-linux-musl"),
            (
                "zig".to_string(),
                vec!["cc".into(), "-target".into(), "x86_64-linux-musl".into()]
            )
        );
        let c = Compiler::from_words("zig cc");
        assert_eq!(c.name(), "`zig cc` (from $LUGHA_CC)");
        assert_eq!(c.words(), "zig cc");
        let command = c.command();
        assert_eq!(command.get_program(), "zig");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["cc"]);
    }

    #[test]
    fn shell_characters_are_ordinary() {
        assert_eq!(parts("a;b"), ("a;b".to_string(), vec![]));
        assert_eq!(
            parts("cc $(x) \"q\" ~/y"),
            (
                "cc".to_string(),
                vec!["$(x)".into(), "\"q\"".into(), "~/y".into()]
            )
        );
    }
}
