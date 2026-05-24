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
#[cfg(feature = "url")]
mod url;

#[cfg(any(unix, windows))]
pub use os_str::OsStrObserver;
#[cfg(any(unix, windows))]
pub use os_string::OsStringObserver;
pub use path::PathObserver;
pub use path_buf::PathBufObserver;
pub use str::StrObserver;
pub use string::StringObserver;
#[cfg(feature = "url")]
pub use url::UrlObserver;

pub(crate) trait TruncateLen {
    fn truncate_len(&self) -> usize;
}

impl TruncateLen for str {
    fn truncate_len(&self) -> usize {
        if cfg!(feature = "utf8") {
            self.chars().count()
        } else if cfg!(feature = "utf16") {
            self.encode_utf16().count()
        } else {
            self.len()
        }
    }
}

impl TruncateLen for char {
    fn truncate_len(&self) -> usize {
        if cfg!(feature = "utf8") {
            1
        } else if cfg!(feature = "utf16") {
            self.len_utf16()
        } else {
            self.len_utf8()
        }
    }
}
