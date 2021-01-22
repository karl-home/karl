mod error;
mod builder;

use rand::Rng;
use serde::Serialize;

pub type HeaderType = u32;
pub const HT_RAW_BYTES: HeaderType = 0;
pub const HT_PING_REQUEST: HeaderType = 1;
pub const HT_PING_RESULT: HeaderType = 2;
pub const HT_COMPUTE_REQUEST: HeaderType = 3;
pub const HT_COMPUTE_RESULT: HeaderType = 4;
pub const HT_HOST_REQUEST: HeaderType = 5;
pub const HT_HOST_RESULT: HeaderType = 6;
pub const HT_REGISTER_REQUEST: HeaderType = 7;
pub const HT_REGISTER_RESULT: HeaderType = 8;
pub const HT_NOTIFY_START: HeaderType = 9;
pub const HT_NOTIFY_END: HeaderType = 10;
pub const HT_HOST_HEARTBEAT: HeaderType = 11;
pub const HT_HOST_REGISTER_REQUEST: HeaderType = 12;

pub use error::Error;
pub use builder::{import_path, ComputeRequestBuilder};

/// Length of auth token.
const TOKEN_LEN: usize = 32;
/// Auth token character set.
const TOKEN_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
/// 32-character unique alphanumeric identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Hash)]
pub struct Token(pub String);
/// Client token, alias for Token.
pub type ClientToken = Token;
/// Request token, alias for Token.
pub type RequestToken = Token;

impl Token {
    /// Randomly generate a new token.
    pub fn gen() -> Self {
        let mut rng = rand::thread_rng();
        let token: String = (0..TOKEN_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..TOKEN_CHARSET.len());
                TOKEN_CHARSET[idx] as char
            })
            .collect();
        Token(token)
    }

    /// Validate the internal string is a valid token.
    ///
    /// Returns true if and only if the token is valid.
    pub fn validate(&self) -> bool {
        if self.0.len() != TOKEN_LEN {
            return false;
        }
        for ch in self.0.bytes() {
            if !TOKEN_CHARSET.contains(&ch) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod test {
    use super::Token;

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
        assert!(Token::gen().validate());
        assert!(Token::gen().validate());
        assert!(Token::gen().validate());
    }

    #[test]
    fn tokens_must_be_alphanumeric() {
        assert!(Token("abcdefghijklmnopqrstuvwxyzABCDEF".to_string()).validate());
        assert!(Token("GHIJKLMNOPQRSTUVWXYZ0123456789ab".to_string()).validate());
        assert!(!Token("%()*@#&%)!(@#*!)*@^#&%(*@^#*(^(*".to_string()).validate());
        assert!(!Token("GHIJKLMNOPQRSTUVWXYZ!!0123456789".to_string()).validate());
    }

    #[test]
    fn tokens_must_be_length_32() {
        assert!(Token("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()).validate());
        assert!(!Token("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()).validate());
        assert!(!Token("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()).validate());
        assert!(!Token("a".to_string()).validate());
        assert!(!Token("".to_string()).validate());
    }
}
