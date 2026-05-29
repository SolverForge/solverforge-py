use solverforge_solver::CrossEntityDistanceMeter;

#[derive(Debug, Clone, Copy, Default)]
pub struct PyDistanceMeter;

impl<S> CrossEntityDistanceMeter<S> for PyDistanceMeter {
    fn distance(
        &self,
        _solution: &S,
        entity_a: usize,
        value_a: usize,
        entity_b: usize,
        value_b: usize,
    ) -> f64 {
        entity_a.abs_diff(entity_b) as f64 + value_a.abs_diff(value_b) as f64
    }
}
