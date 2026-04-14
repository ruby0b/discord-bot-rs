use eyre::{OptionExt as _, Result};

pub trait OptionExt<T> {
    /// Convert None to an error
    fn some(self) -> Result<T>;
    fn inspect_none(self, f: impl FnOnce()) -> Self;
}

impl<T> OptionExt<T> for Option<T> {
    fn some(self) -> Result<T> {
        self.ok_or_eyre("Expected Some but got None")
    }

    fn inspect_none(self, f: impl FnOnce()) -> Self {
        match self {
            Some(x) => Some(x),
            None => {
                f();
                None
            }
        }
    }
}
