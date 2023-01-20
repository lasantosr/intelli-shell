macro_rules! cfg_android {
    ($($item:stmt);* ;) => {
        $(
            #[cfg(target_os = "android")]
            $item;
        )*
    }
}

macro_rules! cfg_macos {
    ($($item:stmt);* ;) => {
        $(
            #[cfg(target_os = "macos")]
            $item;
        )*
    }
}

macro_rules! cfg_unix {
    ($($item:stmt);* ;) => {
        $(
            #[cfg(any(
                target_os = "linux",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "dragonfly",
            ))]
            $item;
        )*
    }
}

macro_rules! cfg_windows {
    ($($item:stmt);* ;) => {
        $(
            #[cfg(target_os = "windows")]
            $item;
        )*
    }
}

pub(crate) use cfg_android;
pub(crate) use cfg_macos;
pub(crate) use cfg_unix;
pub(crate) use cfg_windows;
