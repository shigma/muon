//! Observer implementations for string types.

mod c_str;
mod c_string;
#[cfg(any(unix, windows))]
mod os_str;
#[cfg(any(unix, windows))]
mod os_string;
mod path;
mod path_buf;
mod str;
mod string;

#[cfg(any(unix, windows))]
pub use os_str::OsStrObserver;
#[cfg(any(unix, windows))]
pub use os_string::OsStringObserver;
pub use path::PathObserver;
pub use path_buf::PathBufObserver;
pub use str::StrObserver;
pub use string::StringObserver;

pub(crate) fn str_truncate_len(s: &str) -> usize {
    if cfg!(feature = "utf8") {
        s.chars().count()
    } else if cfg!(feature = "utf16") {
        s.encode_utf16().count()
    } else {
        s.len()
    }
}

pub(crate) fn char_truncate_len(ch: char) -> usize {
    if cfg!(feature = "utf8") {
        1
    } else if cfg!(feature = "utf16") {
        ch.len_utf16()
    } else {
        ch.len_utf8()
    }
}
