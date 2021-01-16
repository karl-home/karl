//! Dashboard state and auth.
use std::time::{Duration, Instant};
use std::collections::HashMap;
use rand::Rng;

/// 64-character alphanumeric string used for authorization.
pub type AuthToken = String;
/// Length of auth token.
const TOKEN_LEN: usize = 64;
/// Auth token character set.
const TOKEN_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
    abcdefghijklmnopqrstuvwxyz \
    0123456789)(*&^%$#@!~";
/// How long an authorization token is valid from its last use, in seconds.
const TOKEN_EXPIRY: u64 = 300;

pub struct DashboardState {
    /// Generated tokens and expiration times.
    tokens: HashMap<String, Instant>,
}

impl DashboardState {
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
        }
    }

    /// Generates a new token with the default expiration time.
    ///
    /// Returns the token.
    pub fn gen_token(&mut self) -> AuthToken {
        let mut rng = rand::thread_rng();
        let token: AuthToken = (0..TOKEN_LEN)
            .map(|_| {
                let idx = rng.gen_range(0..TOKEN_CHARSET.len());
                TOKEN_CHARSET[idx] as char
            })
            .collect();
        let expiry = Instant::now() + Duration::new(TOKEN_EXPIRY, 0);
        self.tokens.insert(token.clone(), expiry);
        token
    }

    /// Removes a token, if it exists.
    pub fn remove_token(&mut self, token: &AuthToken) {
        self.tokens.remove(token);
    }

    /// Checks the token against the generated tokens.
    ///
    /// If the token exists and is valid, refreshes the token expiry time
    /// and returns true. If the token is invalid (aka expired), removes the
    /// token and returns false. If the token does not exist, returns false.
    pub fn use_token(&mut self, token: &AuthToken) -> bool {
        if let Some(expiry) = self.tokens.get_mut(token) {
            let now = Instant::now();
            if now < *expiry {
                *expiry = now + Duration::new(TOKEN_EXPIRY, 0);
                return true;
            }
        }
        self.tokens.remove(token);
        false
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_gen_token() {
        // Multiple tokens are generated
        // The tokens are of the right length
    }

    #[test]
    fn test_use_valid_token() {
        // Generate a token
        // Use it, and it returns true
        // expiration time has changed
    }

    #[test]
    fn test_use_missing_token() {
        // Don't generate any tokens
        // Use a random 64-char token
        // It fails
    }

    #[test]
    fn test_use_expired_token() {
        // Generate a token
        // Wait for it to expire
        // Use it
        // It fails (returns false)
        // It is removed
    }

    #[test]
    fn test_remove_token() {
        // Generate a token
        // Use it, it works
        // Remove the token
        // Use it, it fails
    }
}