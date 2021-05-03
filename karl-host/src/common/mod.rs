mod error;
mod builder;

use rand::Rng;
use std::path::Path;

pub use error::Error;
pub use builder::TarBuilder;
pub use crate::controller::types::*;

/// String ID.
pub type StringID = String;

/// Length of auth token.
const TOKEN_LEN: usize = 32;
/// Auth token character set.
const TOKEN_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
/// 32-character unique alphanumeric identifier.
pub struct Token {}
/// Sensor token.
pub type SensorToken = String;
/// Host token.
pub type HostToken = String;
/// Process token.
pub type ProcessToken = String;

/// Sanitize a path.
///
/// Transforms into a relative path regardless of host filesystem and removes
/// dots and trailing slashes e.g. ./raw/../camera/ --> raw/camera
///
/// Actual path is /home/user/.karl_controller/data/raw/camera.
pub fn sanitize_path(path: &str) -> String {
    let mut new_path = Path::new("").to_path_buf();
    let components = Path::new(path)
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(path) => Some(Path::new(path)),
            _ => None,
        });
    for component in components {
        new_path = new_path.join(component);
    }
    new_path.into_os_string().into_string().unwrap()
}

impl Token {
    /// Randomly generate a new token.
    pub fn gen() -> String {
        let mut rng = rand::thread_rng();
        let token: String = (0..TOKEN_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..TOKEN_CHARSET.len());
                TOKEN_CHARSET[idx] as char
            })
            .collect();
        token
    }

    /// Validate the internal string is a valid token.
    ///
    /// Returns true if and only if the token is valid.
    pub fn validate(token: &str) -> bool {
        if token.len() != TOKEN_LEN {
            return false;
        }
        for ch in token.bytes() {
            if !TOKEN_CHARSET.contains(&ch) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generated_tokens_are_unique() {
        let t1 = Token::gen();
        let t2 = Token::gen();
        let t3 = Token::gen();
        assert!(t1 == t1);
        assert!(t1 != t2);
        assert!(t1 != t3);
        assert!(t2 != t3);
    }

    #[test]
    fn generated_tokens_are_valid() {
        assert!(Token::validate(&Token::gen()));
        assert!(Token::validate(&Token::gen()));
        assert!(Token::validate(&Token::gen()));
    }

    #[test]
    fn tokens_must_be_alphanumeric() {
        assert!(Token::validate("abcdefghijklmnopqrstuvwxyzABCDEF"));
        assert!(Token::validate("GHIJKLMNOPQRSTUVWXYZ0123456789ab"));
        assert!(!Token::validate("%()*@#&%)!(@#*!)*@^#&%(*@^#*(^(*"));
        assert!(!Token::validate("GHIJKLMNOPQRSTUVWXYZ!!0123456789"));
    }

    #[test]
    fn tokens_must_be_length_32() {
        assert!(Token::validate("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(!Token::validate("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(!Token::validate("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(!Token::validate("a"));
        assert!(!Token::validate(""));
    }

    #[test]
    fn test_sanitize_path() {
        assert_eq!(&sanitize_path("raw/cam"), "raw/cam");
        assert_eq!(&sanitize_path("/raw/cam"), "raw/cam");
        assert_eq!(&sanitize_path("./raw/cam"), "raw/cam");
        assert_eq!(&sanitize_path("../raw/cam"), "raw/cam");
        assert_eq!(&sanitize_path("raw/../cam"), "raw/cam");
        assert_eq!(&sanitize_path("./raw/../cam/"), "raw/cam");
    }
}
