use std::fmt;

/// The supported CPU architectures.
#[derive(Eq, PartialEq, Debug)]
pub(crate) enum Architecture {
    Amd64,
    Arm64,
}

impl Architecture {
    pub(crate) fn from_str(input: &str) -> Option<Architecture> {
        match input {
            "amd64" => Some(Architecture::Amd64),
            "arm64" => Some(Architecture::Arm64),
            _ => None,
        }
    }

    pub(crate) fn native() -> Architecture {
        if cfg!(target_arch = "x86_64") {
            Architecture::Amd64
        } else if cfg!(target_arch = "aarch64") {
            Architecture::Arm64
        } else {
            panic!("The host architecture isn't supported");
        }
    }
}

/// The supported operating systems.
#[derive(Eq, PartialEq, Debug)]
pub(crate) enum OperatingSystem {
    Freebsd,
    Linux,
    Mac,
    Openbsd,
}

impl OperatingSystem {
    pub(crate) fn from_str(input: &str) -> Option<OperatingSystem> {
        match input {
            "freebsd" => Some(OperatingSystem::Freebsd),
            "linux" => Some(OperatingSystem::Linux),
            "mac" => Some(OperatingSystem::Mac),
            "openbsd" => Some(OperatingSystem::Openbsd),
            _ => None,
        }
    }

    pub(crate) fn native() -> OperatingSystem {
        if cfg!(target_os = "freebsd") {
            OperatingSystem::Freebsd
        } else if cfg!(target_os = "linux") {
            OperatingSystem::Linux
        } else if cfg!(target_os = "macos") {
            OperatingSystem::Mac
        } else if cfg!(target_os = "openbsd") {
            OperatingSystem::Openbsd
        } else {
            panic!("The host operating system isn't supported");
        }
    }
}

/// The ABI to target.
#[derive(Eq, PartialEq, Debug)]
pub(crate) enum Abi {
    Native,
    Gnu,
    Msvc,
}

impl Abi {
    pub(crate) fn from_str(input: &str) -> Option<Abi> {
        match input {
            "native" => Some(Abi::Native),
            "gnu" => Some(Abi::Gnu),
            "msvc" => Some(Abi::Msvc),
            _ => None,
        }
    }

    pub(crate) fn native() -> Abi {
        if cfg!(target_env = "gnu") {
            Abi::Gnu
        } else if cfg!(target_env = "msvc") {
            Abi::Msvc
        } else {
            Abi::Native
        }
    }
}

/// A type describing the compile target, such as the operating system and
/// architecture.
#[derive(Eq, PartialEq, Debug)]
pub struct Target {
    pub(crate) arch: Architecture,
    pub(crate) os: OperatingSystem,
    pub(crate) abi: Abi,
}

impl Target {
    /// Parses a target from a string.
    ///
    /// If the target is invalid, a None is returned.
    pub(crate) fn from_str(input: &str) -> Option<Target> {
        let mut iter = input.split('-');
        let arch = iter.next().and_then(Architecture::from_str)?;
        let os = iter.next().and_then(OperatingSystem::from_str)?;
        let abi = iter.next().and_then(Abi::from_str)?;

        Some(Target { arch, os, abi })
    }

    /// Returns the target for the current platform.
    pub fn native() -> Target {
        Target {
            arch: Architecture::native(),
            os: OperatingSystem::native(),
            abi: Abi::native(),
        }
    }

    /// Returns a String describing the target using the LLVM triple format.
    pub(crate) fn llvm_triple(&self) -> String {
        let arch = match self.arch {
            Architecture::Amd64 => "x86_64",
            Architecture::Arm64 => "aarch64",
        };

        let os = match self.os {
            OperatingSystem::Freebsd => "unknown-freebsd",
            OperatingSystem::Mac => "apple-darwin",
            OperatingSystem::Openbsd => "unknown-openbsd",
            OperatingSystem::Linux => "unknown-linux-gnu",
        };

        format!("{}-{}", arch, os)
    }

    pub(crate) fn arch_name(&self) -> &'static str {
        match self.arch {
            Architecture::Amd64 => "amd64",
            Architecture::Arm64 => "arm64",
        }
    }

    pub(crate) fn os_name(&self) -> &'static str {
        match self.os {
            OperatingSystem::Freebsd => "freebsd",
            OperatingSystem::Mac => "mac",
            OperatingSystem::Openbsd => "openbsd",
            OperatingSystem::Linux => "linux",
        }
    }

    pub(crate) fn abi_name(&self) -> &'static str {
        match self.abi {
            Abi::Native => match self.os {
                OperatingSystem::Linux => "gnu",
                _ => "native",
            },
            Abi::Gnu => "gnu",
            Abi::Msvc => "msvc",
        }
    }

    pub(crate) fn is_native(&self) -> bool {
        self == &Target::native()
    }
}

impl fmt::Display for Target {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(
            fmt,
            "{}-{}-{}",
            self.arch_name(),
            self.os_name(),
            self.abi_name()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(arch: Architecture, os: OperatingSystem, abi: Abi) -> Target {
        Target { arch, os, abi }
    }

    #[test]
    fn test_operating_system_from_str() {
        assert_eq!(
            OperatingSystem::from_str("freebsd"),
            Some(OperatingSystem::Freebsd)
        );
        assert_eq!(
            OperatingSystem::from_str("openbsd"),
            Some(OperatingSystem::Openbsd)
        );
        assert_eq!(
            OperatingSystem::from_str("linux"),
            Some(OperatingSystem::Linux)
        );
        assert_eq!(
            OperatingSystem::from_str("mac"),
            Some(OperatingSystem::Mac)
        );
        assert_eq!(OperatingSystem::from_str("bla"), None);
    }

    #[test]
    fn test_architecture_from_str() {
        assert_eq!(Architecture::from_str("amd64"), Some(Architecture::Amd64));
        assert_eq!(Architecture::from_str("arm64"), Some(Architecture::Arm64));
        assert_eq!(Architecture::from_str("bla"), None);
    }

    #[test]
    fn test_target_from_str() {
        assert_eq!(
            Target::from_str("amd64-freebsd-native"),
            Some(target(
                Architecture::Amd64,
                OperatingSystem::Freebsd,
                Abi::Native
            ))
        );
        assert_eq!(
            Target::from_str("arm64-linux-gnu"),
            Some(
                target(Architecture::Arm64, OperatingSystem::Linux, Abi::Gnu,)
            )
        );

        assert_eq!(Target::from_str("bla-linux-native"), None);
        assert_eq!(Target::from_str("amd64-bla-native"), None);
        assert_eq!(Target::from_str("amd64-linux"), None);
    }

    #[test]
    fn test_target_host() {
        let target = Target::native();

        assert_eq!(target.arch, Architecture::native());
        assert_eq!(target.os, OperatingSystem::native());
    }

    #[test]
    fn test_target_llvm_triple() {
        assert_eq!(
            target(Architecture::Amd64, OperatingSystem::Linux, Abi::Native)
                .llvm_triple(),
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            target(Architecture::Amd64, OperatingSystem::Freebsd, Abi::Native)
                .llvm_triple(),
            "x86_64-unknown-freebsd"
        );
        assert_eq!(
            target(Architecture::Arm64, OperatingSystem::Mac, Abi::Native)
                .llvm_triple(),
            "aarch64-apple-darwin"
        );
    }

    #[test]
    fn test_target_to_string() {
        assert_eq!(
            target(Architecture::Amd64, OperatingSystem::Linux, Abi::Native)
                .to_string(),
            "amd64-linux-gnu"
        );
        assert_eq!(
            target(Architecture::Amd64, OperatingSystem::Freebsd, Abi::Native)
                .to_string(),
            "amd64-freebsd-native"
        );
        assert_eq!(
            target(Architecture::Arm64, OperatingSystem::Mac, Abi::Native)
                .to_string(),
            "arm64-mac-native"
        );
    }

    #[test]
    fn test_target_is_native() {
        assert!(Target::native().is_native());
    }
}
