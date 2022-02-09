use std::fmt;

#[derive(Debug, Clone)]
pub struct MiscError(String);

impl fmt::Display for MiscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MiscError {:?}", self.0)
    }
}

impl std::error::Error for MiscError {}

impl From<String> for MiscError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for MiscError {
    fn from(s: &str) -> Self {
        Self(String::from(s))
    }
}
