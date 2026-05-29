use crate::score::DynamicScore;

#[derive(Debug, Clone, Copy)]
pub struct FullRecomputeDelta {
    pub previous: DynamicScore,
}
