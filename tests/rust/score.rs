use solverforge_core::score::Score;
use solverforge_py::score::DynamicScore;

#[test]
fn dynamic_score_orders_by_hard_medium_soft() {
    assert!(DynamicScore::of(0, 0, 0) > DynamicScore::of(-1, 100, 100));
    assert_eq!(DynamicScore::levels_count(), 3);
}
