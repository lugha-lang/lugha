//! The platforms lughac builds for.

/// A platform an executable is built for: `--target`'s values. All are
/// Linux with the same scalar sizes (`split_leaves::byte_size`); they differ
/// in instructions, in the runtime's stack switch and in how C passes structs
/// (by CPU), and in the C library the runtime is linked with (glibc or musl).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    /// x86-64 Linux with glibc, `x86_64-unknown-linux-gnu`.
    #[default]
    X86_64Linux,
    /// ARM64 Linux with glibc, `aarch64-unknown-linux-gnu`.
    Aarch64Linux,
    /// x86-64 Linux with musl, `x86_64-unknown-linux-musl`: static executables.
    X86_64LinuxMusl,
    /// ARM64 Linux with musl, `aarch64-unknown-linux-musl`: static executables.
    Aarch64LinuxMusl,
    /// macOS on Apple Silicon, `aarch64-apple-darwin`: Mach-O executables,
    /// built on Linux and run on a Mac.
    Aarch64MacOs,
}

/// A target's processor family: what codegen's instructions, registers and
/// C calling rules follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    /// x86-64, System V calling rules.
    X86_64,
    /// ARM64 (ARMv8-A), AAPCS64 calling rules.
    Aarch64,
}

impl Target {
    /// The target's LLVM triple.
    pub fn triple(self) -> &'static str {
        match self {
            Target::X86_64Linux => "x86_64-unknown-linux-gnu",
            Target::Aarch64Linux => "aarch64-unknown-linux-gnu",
            Target::X86_64LinuxMusl => "x86_64-unknown-linux-musl",
            Target::Aarch64LinuxMusl => "aarch64-unknown-linux-musl",
            Target::Aarch64MacOs => "aarch64-apple-darwin",
        }
    }

    /// The target's processor family.
    pub fn arch(self) -> Arch {
        match self {
            Target::X86_64Linux | Target::X86_64LinuxMusl => Arch::X86_64,
            Target::Aarch64Linux | Target::Aarch64LinuxMusl | Target::Aarch64MacOs => Arch::Aarch64,
        }
    }

    /// The machine lughac itself was built for, which its own builds and
    /// runs target unless `--target` names another.
    pub fn native() -> Target {
        let arm = cfg!(target_arch = "aarch64");
        match (arm, cfg!(target_env = "musl")) {
            (false, false) => Target::X86_64Linux,
            (true, false) => Target::Aarch64Linux,
            (false, true) => Target::X86_64LinuxMusl,
            (true, true) => Target::Aarch64LinuxMusl,
        }
    }

    /// Whether this is the machine lughac runs on, built for with `cc`;
    /// any other target is foreign, built for with zig.
    pub fn is_native(self) -> bool {
        self == Target::native()
    }
}

#[cfg(test)]
mod tests {
    use super::{Arch, Target};

    #[test]
    fn each_target_has_its_triple_and_family() {
        let all = [
            (
                Target::X86_64Linux,
                "x86_64-unknown-linux-gnu",
                Arch::X86_64,
            ),
            (
                Target::Aarch64Linux,
                "aarch64-unknown-linux-gnu",
                Arch::Aarch64,
            ),
            (
                Target::X86_64LinuxMusl,
                "x86_64-unknown-linux-musl",
                Arch::X86_64,
            ),
            (
                Target::Aarch64LinuxMusl,
                "aarch64-unknown-linux-musl",
                Arch::Aarch64,
            ),
            (Target::Aarch64MacOs, "aarch64-apple-darwin", Arch::Aarch64),
        ];
        for (target, triple, arch) in all {
            assert_eq!((target.triple(), target.arch()), (triple, arch));
        }
    }

    #[test]
    fn only_the_build_machine_is_native() {
        let native = [
            Target::X86_64Linux,
            Target::Aarch64Linux,
            Target::X86_64LinuxMusl,
            Target::Aarch64LinuxMusl,
            Target::Aarch64MacOs,
        ]
        .into_iter()
        .filter(|t| t.is_native())
        .collect::<Vec<_>>();
        assert_eq!(native, vec![Target::native()]);
        // These tests run on the machine they were built for.
        let arch = if cfg!(target_arch = "aarch64") {
            Arch::Aarch64
        } else {
            Arch::X86_64
        };
        assert_eq!(Target::native().arch(), arch);
    }
}
