use std::{
    fs::{File, OpenOptions},
    sync::Mutex,
};

use once_cell::sync::Lazy;

static _LOG_FILE: Lazy<Mutex<File>> = Lazy::new(|| {
    Mutex::new(
        OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open("debug.log")
            .unwrap(),
    )
});

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::debug::trace_log(format!($($arg)*))
    };
}

pub fn trace_log(_str: impl AsRef<str>) {
    #[cfg(feature = "debug")]
    {
        use std::io::Write;
        let mut file = _LOG_FILE.lock().unwrap();
        writeln!(file, "{}", _str.as_ref()).unwrap();
    }
}

#[allow(unused_imports)]
pub(crate) use trace;
