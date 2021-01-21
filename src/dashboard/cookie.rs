//! Dashboard state and auth.
use std::time::{Duration, Instant};
use std::collections::HashMap;
use rand::Rng;

/// Length of cookie.
const LEN: usize = 32;
/// Cookie character set.
const CHARSET: &[u8] = b"0123456789)(*&^%$#@!~\
    ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
/// How long a cookie is valid for from its last use, in seconds.
const EXPIRY: u64 = 300;

/// Active client session state - generated cookies and expiration times.
pub struct SessionState {
    /// Generated cookies and expiration times.
    cookies: HashMap<String, Instant>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            cookies: HashMap::new(),
        }
    }

    /// Generates a new cookie with the default expiration time.
    ///
    /// Returns the cookie.
    pub fn gen_cookie(&mut self) -> String {
        let mut rng = rand::thread_rng();
        let cookie: String = (0..LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        let expiry = Instant::now() + Duration::new(EXPIRY, 0);
        self.cookies.insert(cookie.clone(), expiry);
        cookie
    }

    /// Removes a cookie, if it exists.
    #[allow(dead_code)]
    pub fn remove_cookie(&mut self, cookie: &str) {
        self.cookies.remove(cookie);
    }

    /// Checks the cookie against the generated cookies.
    ///
    /// If the cookie exists and is valid, refreshes the cookie expiry time
    /// and returns true. If the cookie is invalid (aka expired), removes the
    /// cookie and returns false. If the cookie does not exist, returns false.
    pub fn use_cookie(&mut self, cookie: &str) -> bool {
        if let Some(expiry) = self.cookies.get_mut(cookie) {
            let now = Instant::now();
            if now < *expiry {
                *expiry = now + Duration::new(EXPIRY, 0);
                return true;
            }
        }
        self.cookies.remove(cookie);
        false
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::thread;

    #[test]
    fn test_gen_cookie() {
        let mut cookies = SessionState::new();

        // Multiple cookies are generated
        let c1 = cookies.gen_cookie();
        let c2 = cookies.gen_cookie();
        let c3 = cookies.gen_cookie();
        assert!(c1 == c1);
        assert!(c1 != c2);
        assert!(c1 != c3);
        assert!(c2 != c3);

        // The cookies are of the right length
        assert_eq!(c1.len(), LEN);
        assert_eq!(c2.len(), LEN);
        assert_eq!(c3.len(), LEN);
    }

    #[test]
    fn test_use_valid_cookie() {
        let mut c = SessionState::new();
        let cookie = c.gen_cookie();
        let old_expiry = c.cookies.get(&cookie).unwrap().clone();
        thread::sleep(Duration::from_secs(2));
        assert!(c.use_cookie(&cookie), "used valid cookie");
        let new_expiry = c.cookies.get(&cookie).unwrap();
        assert!(new_expiry > &old_expiry, "expiration time updated");
    }

    #[test]
    fn test_use_invalid_cookie() {
        let mut c = SessionState::new();
        let mut rng = rand::thread_rng();
        let cookie: String = (0..LEN)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        assert!(!c.use_cookie(&cookie), "bogus cookie");
    }

    #[test]
    fn test_use_expired_cookie() {
        let mut c = SessionState::new();
        let cookie = c.gen_cookie();
        assert!(c.use_cookie(&cookie), "cookie has not expired");
        *c.cookies.get_mut(&cookie).unwrap() = Instant::now();
        thread::sleep(Duration::from_secs(2));
        assert!(!c.use_cookie(&cookie), "cookie has expired");
        assert!(!c.cookies.contains_key(&cookie), "cookie was removed")
    }

    #[test]
    fn test_remove_cookie() {
        let mut c = SessionState::new();
        let cookie = c.gen_cookie();
        assert!(c.use_cookie(&cookie), "used valid cookie once");
        assert!(c.use_cookie(&cookie), "used valid cookie twice");
        c.remove_cookie(&cookie);
        assert!(!c.use_cookie(&cookie), "cookie was removed");
    }
}
