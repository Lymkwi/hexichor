
#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    user: String,
    password: String
}

impl LoginRequest {
    pub fn get_user(&self) -> &str {
        &self.user
    }

    pub fn get_password(&self) -> &str {
        &self.password
    }
}
