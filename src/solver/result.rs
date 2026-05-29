use crate::score::DynamicScore;

#[derive(Debug, Clone)]
pub struct SolveSummary {
    pub score: Option<DynamicScore>,
}
