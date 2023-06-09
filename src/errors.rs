use thiserror::Error;

#[derive(Error, Debug)]
pub enum ERR {
    #[error("Failed to spawn gpg - is it installed on your system?\nSee the docs at https://docs.env-store.com/gpg/ for more info")]
    GpgError,
}
