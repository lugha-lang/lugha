//! The platforms lughac builds for.

/// A platform an executable is built for: `--target`'s values. All have
/// the same scalar sizes (`split_leaves::byte_size`); they differ in
/// instructions, in the runtime's stack switch and in how C passes structs
/// (by CPU and system), and in the C library the runtime is linked with.
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
    /// Windows on x86-64, `x86_64-pc-windows-gnu`: PE executables linked
    /// against Windows' own C runtime, built on Linux and run on Windows.
    X86_64Windows,
    /// Windows on ARM64, `aarch64-pc-windows-gnu`: the same, for ARM64 PCs.
    Aarch64Windows,
    /// Android on ARM64, `aarch64-linux-android`: PIE executables linked
    /// against Android's own C library (bionic) by the NDK, for Android 10
    /// and newer, built on Linux and run from a shell on the device.
    Aarch64Android,
    /// Android on x86-64, `x86_64-linux-android`: the same, for the
    /// emulator and x86-64 devices.
    X86_64Android,
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
            Target::X86_64Windows => "x86_64-pc-windows-gnu",
            Target::Aarch64Windows => "aarch64-pc-windows-gnu",
            Target::Aarch64Android => "aarch64-linux-android",
            Target::X86_64Android => "x86_64-linux-android",
        }
    }

    /// The triple LLVM generates code for: [`Target::triple`], but for
    /// Android, whose carries the oldest Android supported (10, API 29),
    /// so thread-locals are the system's own rather than emulated.
    pub fn llvm_triple(self) -> &'static str {
        match self {
            Target::Aarch64Android => "aarch64-unknown-linux-android29",
            Target::X86_64Android => "x86_64-unknown-linux-android29",
            other => other.triple(),
        }
    }

    /// The target's processor family.
    pub fn arch(self) -> Arch {
        match self {
            Target::X86_64Linux
            | Target::X86_64LinuxMusl
            | Target::X86_64Windows
            | Target::X86_64Android => Arch::X86_64,
            Target::Aarch64Linux
            | Target::Aarch64LinuxMusl
            | Target::Aarch64MacOs
            | Target::Aarch64Windows
            | Target::Aarch64Android => Arch::Aarch64,
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

    /// Whether this is a Windows target: `.exe` names, SEH unwinding, and
    /// Windows' own rules for passing structs to C.
    pub fn is_windows(self) -> bool {
        matches!(self, Target::X86_64Windows | Target::Aarch64Windows)
    }

    /// Whether this is an Android target: built with the NDK, run only on
    /// Android.
    pub fn is_android(self) -> bool {
        matches!(self, Target::Aarch64Android | Target::X86_64Android)
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
            (Target::X86_64Windows, "x86_64-pc-windows-gnu", Arch::X86_64),
            (
                Target::Aarch64Windows,
                "aarch64-pc-windows-gnu",
                Arch::Aarch64,
            ),
            (
                Target::Aarch64Android,
                "aarch64-linux-android",
                Arch::Aarch64,
            ),
            (Target::X86_64Android, "x86_64-linux-android", Arch::X86_64),
        ];
        for (target, triple, arch) in all {
            assert_eq!((target.triple(), target.arch()), (triple, arch));
        }
    }

    #[test]
    fn android_is_its_own_system_and_llvm_gets_its_api_level() {
        for (target, llvm) in [
            (Target::Aarch64Android, "aarch64-unknown-linux-android29"),
            (Target::X86_64Android, "x86_64-unknown-linux-android29"),
        ] {
            assert!(target.is_android() && !target.is_windows() && !target.is_native());
            assert_eq!(target.llvm_triple(), llvm);
        }
        assert!(!Target::Aarch64LinuxMusl.is_android());
        assert_eq!(Target::X86_64Windows.llvm_triple(), "x86_64-pc-windows-gnu");
    }

    #[test]
    fn only_the_build_machine_is_native() {
        let native = [
            Target::X86_64Linux,
            Target::Aarch64Linux,
            Target::X86_64LinuxMusl,
            Target::Aarch64LinuxMusl,
            Target::Aarch64MacOs,
            Target::X86_64Windows,
            Target::Aarch64Windows,
            Target::Aarch64Android,
            Target::X86_64Android,
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
