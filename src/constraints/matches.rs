#[derive(Debug, Clone)]
pub struct DynamicConstraintMatch {
    pub constraint_name: String,
    pub entity_indices: Vec<usize>,
}
