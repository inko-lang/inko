//! Basic platform detection.
//!
//! This module provides basic functionality for detecting the underlying
//! platform.

/// Returns the name of the underlying operating system.
pub fn operating_system<'a>() -> &'a str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "freebsd") {
        "freebsd"
    } else if cfg!(target_os = "dragonfly") {
        "dragonfly"
    } else if cfg!(target_os = "bitrig") {
        "bitrig"
    } else if cfg!(target_os = "openbsd") {
        "openbsd"
    } else if cfg!(target_os = "netbsd") {
        "netbsd"
    } else if cfg!(unix) {
        "unix"
    } else {
        "unknown"
    }
}

#[cfg(test)]
mod tests {
    macro_rules! test_operating_system {
        ($platform: expr) => {
            #[cfg(target_os = $platform)]
            #[test]
            fn test_operating_system() {
                assert_eq!(super::operating_system(), $platform);
            }
        };
    }

    test_operating_system!("windows");
    test_operating_system!("macos");
    test_operating_system!("ios");
    test_operating_system!("linux");
    test_operating_system!("android");
    test_operating_system!("freebsd");
    test_operating_system!("dragonfly");
    test_operating_system!("bitrig");
    test_operating_system!("openbsd");
    test_operating_system!("netbsd");

    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "bitrig",
        target_os = "openbsd",
        target_os = "netbsd"
    )))]
    #[test]
    fn test_operating_system() {
        assert_eq!(super::operating_system(), "unknown");
    }
}
