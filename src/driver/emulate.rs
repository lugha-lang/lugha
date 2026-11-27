//! Starting an executable built for a
//! target on this machine: directly when the target's CPU is this
//! machine's (a static musl executable included), else under qemu's
//! user-mode emulator, `qemu-<arch> <exe> <args…>`, found on `PATH` and
//! run from an argument list, never a shell. Under qemu the kernel's
//! process lockdown doesn't apply: emulated runs are for testing.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::codegen::{Arch, Target};

/// How to start an executable built for one target on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Runner {
    /// The emulator, or `None` to start the executable itself.
    program: Option<OsString>,
    /// Environment variables the emulator gets: where its C library is.
    envs: Vec<(OsString, OsString)>,
}

impl Runner {
    /// A command starting `exe`; the caller adds the program's arguments.
    pub(super) fn command(&self, exe: &Path) -> Command {
        let Some(program) = &self.program else {
            return Command::new(exe);
        };
        let mut command = Command::new(program);
        command.arg(exe);
        command.envs(self.envs.iter().map(|(k, v)| (k, v)));
        command
    }
}

/// How to start an executable built for `target` here.
///
/// # Errors
///
/// A message, for an internal error (exit 2): qemu isn't on `PATH`, or a
/// glibc executable has no C library for qemu to load.
pub(super) fn runner(target: Target) -> Result<Runner, String> {
    let machine = Machine {
        arch: Target::native().arch(),
        prefix_set: std::env::var_os("QEMU_LD_PREFIX").is_some(),
        has: &|name| on_path(name),
        is_dir: &|path| path.is_dir(),
    };
    choose(target, &machine)
}

/// What `choose` needs to know about this machine, so tests can fake it.
struct Machine<'a> {
    /// This machine's CPU.
    arch: Arch,
    /// Whether `QEMU_LD_PREFIX` is set, which then always wins.
    prefix_set: bool,
    /// Whether a program of this name is on `PATH`.
    has: &'a dyn Fn(&str) -> bool,
    /// Whether this folder exists.
    is_dir: &'a dyn Fn(&Path) -> bool,
}

/// [`runner`] on `machine`.
fn choose(target: Target, machine: &Machine) -> Result<Runner, String> {
    if target == Target::Aarch64MacOs {
        return Err(format!(
            "a program built for {} runs only on a Mac: build it with `lughac build`",
            target.triple()
        ));
    }
    if target.is_windows() {
        return Err(format!(
            "a program built for {} runs only on Windows: build it with `lughac build`",
            target.triple()
        ));
    }
    // Before the CPU check: an x86-64 Android build would otherwise start
    // here, on a Linux without Android's C library.
    if target.is_android() {
        return Err(format!(
            "a program built for {} runs only on Android: build it with `lughac build`, then run it with `adb shell`",
            target.triple()
        ));
    }
    let arch = target.arch();
    if arch == machine.arch {
        return Ok(Runner::default());
    }
    let (qemu, gnu) = match arch {
        Arch::X86_64 => ("qemu-x86_64", "x86_64-linux-gnu"),
        Arch::Aarch64 => ("qemu-aarch64", "aarch64-linux-gnu"),
    };
    let triple = target.triple();
    if !(machine.has)(qemu) {
        return Err(format!(
            "running a program built for {triple} needs {qemu} (Ubuntu: `sudo apt install qemu-user`)"
        ));
    }
    let mut envs = Vec::new();
    // A static musl executable needs nothing; a glibc one needs that CPU's
    // C library, which Debian and Ubuntu's cross packages put here.
    if !triple.ends_with("-musl") && !machine.prefix_set {
        let folder = PathBuf::from("/usr").join(gnu);
        if !(machine.is_dir)(&folder) {
            let cpu = match arch {
                Arch::X86_64 => ("an x86-64", "libc6-amd64-cross"),
                Arch::Aarch64 => ("an ARM64", "libc6-arm64-cross"),
            };
            let musl = triple.replace("-gnu", "-musl");
            return Err(format!(
                "running a program built for {triple} needs {} C library for qemu: \
                 install {}, set QEMU_LD_PREFIX, or build for {musl}",
                cpu.0, cpu.1
            ));
        }
        envs.push((OsString::from("QEMU_LD_PREFIX"), folder.into_os_string()));
    }
    Ok(Runner {
        program: Some(OsString::from(qemu)),
        envs,
    })
}

/// Whether an executable file `name` is in one of `PATH`'s folders.
fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|folder| {
        std::fs::metadata(folder.join(name))
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

#[cfg(test)]
mod tests {
    use super::{Machine, Runner, choose};
    use crate::codegen::{Arch, Target};
    use std::path::Path;

    fn x86(has_qemu: bool, prefix_set: bool, cross_libc: bool) -> Result<Runner, String> {
        let has = move |name: &str| has_qemu && name == "qemu-aarch64";
        let is_dir = move |path: &Path| cross_libc && path == Path::new("/usr/aarch64-linux-gnu");
        let machine = Machine {
            arch: Arch::X86_64,
            prefix_set,
            has: &has,
            is_dir: &is_dir,
        };
        choose(Target::Aarch64Linux, &machine)
    }

    fn args(runner: &Runner) -> Vec<String> {
        let command = runner.command(Path::new("/t/prog"));
        let mut words = vec![command.get_program().to_string_lossy().into_owned()];
        words.extend(command.get_args().map(|a| a.to_string_lossy().into_owned()));
        words
    }

    fn envs(runner: &Runner) -> Vec<(String, String)> {
        let command = runner.command(Path::new("/t/prog"));
        command
            .get_envs()
            .map(|(k, v)| {
                let v = v.map_or(String::new(), |v| v.to_string_lossy().into_owned());
                (k.to_string_lossy().into_owned(), v)
            })
            .collect()
    }

    fn on(arch: Arch, target: Target, has_qemu: bool) -> Result<Runner, String> {
        let has = move |_: &str| has_qemu;
        let is_dir = |_: &Path| true;
        let machine = Machine {
            arch,
            prefix_set: false,
            has: &has,
            is_dir: &is_dir,
        };
        choose(target, &machine)
    }

    #[test]
    fn this_machine_s_cpu_runs_directly_whatever_its_c_library() {
        for target in [Target::X86_64Linux, Target::X86_64LinuxMusl] {
            let runner = on(Arch::X86_64, target, false).unwrap();
            assert_eq!(args(&runner), ["/t/prog"], "{target:?}");
            assert!(envs(&runner).is_empty());
        }
        let arm = on(Arch::Aarch64, Target::Aarch64LinuxMusl, false).unwrap();
        assert_eq!(args(&arm), ["/t/prog"]);
    }

    #[test]
    fn an_android_program_is_never_run_here_even_on_its_cpu() {
        for target in [Target::X86_64Android, Target::Aarch64Android] {
            for arch in [Arch::X86_64, Arch::Aarch64] {
                let refused = on(arch, target, true).unwrap_err();
                assert!(refused.contains("runs only on Android"), "{refused}");
                assert!(refused.contains("adb shell"), "{refused}");
            }
        }
    }

    #[test]
    fn a_windows_program_is_never_run_here() {
        for target in [Target::X86_64Windows, Target::Aarch64Windows] {
            for arch in [Arch::X86_64, Arch::Aarch64] {
                let refused = on(arch, target, true).unwrap_err();
                assert!(refused.contains("runs only on Windows"), "{refused}");
            }
        }
    }

    #[test]
    fn another_cpu_runs_under_its_qemu() {
        let musl = on(Arch::X86_64, Target::Aarch64LinuxMusl, true).unwrap();
        assert_eq!(args(&musl), ["qemu-aarch64", "/t/prog"]);
        assert!(
            envs(&musl).is_empty(),
            "a static executable needs no C library"
        );
        let x86 = on(Arch::Aarch64, Target::X86_64LinuxMusl, true).unwrap();
        assert_eq!(args(&x86), ["qemu-x86_64", "/t/prog"]);
    }

    #[test]
    fn glibc_under_qemu_gets_the_cross_c_library_unless_one_is_set() {
        let found = x86(true, false, true).unwrap();
        assert_eq!(args(&found), ["qemu-aarch64", "/t/prog"]);
        assert_eq!(
            envs(&found),
            [(
                "QEMU_LD_PREFIX".to_string(),
                "/usr/aarch64-linux-gnu".to_string()
            )]
        );
        let set = x86(true, true, false).unwrap();
        assert!(envs(&set).is_empty(), "a set QEMU_LD_PREFIX is left alone");
        let missing = x86(true, false, false).unwrap_err();
        assert!(
            missing.contains("needs an ARM64 C library for qemu"),
            "{missing}"
        );
        assert!(missing.contains("libc6-arm64-cross"), "{missing}");
        assert!(missing.contains("aarch64-unknown-linux-musl"), "{missing}");
    }

    #[test]
    fn a_missing_qemu_is_named() {
        let err = x86(false, false, true).unwrap_err();
        assert_eq!(
            err,
            "running a program built for aarch64-unknown-linux-gnu needs qemu-aarch64 \
             (Ubuntu: `sudo apt install qemu-user`)"
        );
    }
}
