
#[derive(Serialize)]
pub struct StatusReply {
    finished: bool,
    results: HashMap<String, i32>
}

impl StatusReply {
    pub fn new(finished: bool, results: HashMap<String, i32>) -> Self {
        Self { finished, results }
    }
}
