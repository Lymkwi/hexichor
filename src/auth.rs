//! Authentication module

use argon2::{
    self,
    Config
};
use rand::{
    distributions::Alphanumeric,
    Rng
};

use std::{
    collections::HashMap,
    path::Path
};

pub struct Engine {
    users: HashMap<String, String>,
    allowed: Vec<String>
}

impl Engine {
    pub fn new<A: AsRef<Path>>(_path: A) -> Self {
        // TODO: load shit from the file
        //let salt = b"et oui Jamy c'est ici que l'on recolte la moitie du sel rustaceen";
        //let password = "🅱assword";
        //let config = Config::default();
        let users = HashMap::new();
        //users.insert("a_user".into(), argon2::hash_encoded(password.as_bytes(), salt, &config).unwrap());
        Self {
            users,
            allowed: Vec::new()
        }
    }

    pub fn is_authorized(&self, cookie: &String) -> bool {
        self.allowed.contains(cookie)
    }

    pub fn verify(&mut self, user: &str, password: &[u8]) -> Option<String> {
        if let Some(hash) = self.users.get(user) {
            if argon2::verify_encoded(hash, password) == Ok(true) {
                // Generate and store the cookie
                // Yeah it's not true randomness, it's
                // two Mersenne Twisters in a trench coat
                // but can I honestly do better? no rn at least
                let cookie: String = rand::thread_rng()
                    .sample_iter(&Alphanumeric)
                    .take(74)
                    .map(char::from)
                    .collect();
                self.allowed.push(cookie.clone());
                Some(cookie)
            } else {
                None
            }
        } else {
            // Not exactly secure, but better than nothing
            std::thread::sleep(std::time::Duration::from_millis(1000));
            None
        }
    }
}
