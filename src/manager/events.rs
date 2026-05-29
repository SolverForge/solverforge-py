#[derive(Debug, Clone)]
pub struct PySolverEvent {
    pub job_id: usize,
    pub lifecycle_state: String,
}
