#[derive(Debug, Clone)]
pub struct PythonProgressEvent {
    pub event_sequence: u64,
    pub lifecycle_state: String,
}
