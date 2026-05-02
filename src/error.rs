pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

pub fn message(text: impl Into<String>) -> Error {
    std::io::Error::other(text.into()).into()
}
