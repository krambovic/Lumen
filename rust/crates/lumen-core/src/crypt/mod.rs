//! Расшифровка закрытых ссылок: Happ (`happ://crypt*`) и Incy (`incy://crypt1/`).

mod happ;
mod happ_keys;
mod incy;

pub use happ::{
    decrypt_happ_link, is_happ_crypt_link, is_happ_link, HappDecryptError, HappKeyUnavailableError,
};
pub use incy::{decrypt_incy_link, is_incy_crypt_link, is_incy_link, IncyDecryptError};
