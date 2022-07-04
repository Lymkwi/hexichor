//! Authentication module

use argon2::{
    self,
    Config
};
use rand::{
    distributions::Alphanumeric,
    Rng
};
use serde::{
    Deserialize,
    Serialize
};

use std::{
    collections::HashMap,
    path::{
        Path,
        PathBuf    
    }
};

pub struct Engine {
    users: HashMap<String, String>,
    allowed: Vec<String>
}

#[derive(Default, Deserialize, Serialize)]
struct FileFormat {
    credentials: HashMap<String, String>
}

impl Engine {
    pub fn new(path: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let bpath: PathBuf = PathBuf::from(path);
        // If the file does not exist..
        if !bpath.exists() {
            // Try and create the file
            let file = std::fs::File::create(path)?;
            serde_yaml::to_writer(file, &FileFormat::default())?;
        }
        // Read from the file
        let data_bytes: Vec<u8> = std::fs::read(path)?;
        let data_string: String = String::from_utf8(data_bytes)?;
        let users_file: FileFormat = serde_yaml::from_str(&data_string)?;
        //users.insert("a_user".into(), argon2::hash_encoded(password.as_bytes(), salt, &config).unwrap());
        Ok(Self {
            users: users_file.credentials,
            allowed: Vec::new()
        })
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
