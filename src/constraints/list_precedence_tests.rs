use super::*;

fn state() -> DynamicListPrecedenceState {
    let mut state = DynamicListPrecedenceState::new(
        "hard_soft".into(),
        (0..6).collect(),
        (0..6).map(|node| (node, node)).collect(),
        2,
        vec![2, 3, 2, 3, 2, 3],
    );
    for edge in [(0, 1), (2, 3), (4, 5)] {
        state.add_edge(edge);
    }
    state.refresh_score_full();
    state
}

fn snapshot(elements: &[usize]) -> RouteSnapshot {
    RouteSnapshot {
        elements: elements.to_vec(),
        edges: elements.windows(2).map(|pair| (pair[0], pair[1])).collect(),
        ..RouteSnapshot::default()
    }
}

#[test]
fn incremental_route_trials_match_full_graph_rebuilds() {
    let mut incremental = state();
    let mut full = state();
    let routes = [
        vec![],
        vec![0, 2, 4],
        vec![1, 3, 5],
        vec![1, 0],
        vec![0, 0],
        vec![5, 4, 3, 2, 1, 0],
        vec![0, 1, 2, 3, 4, 5],
    ];
    for left in &routes {
        for right in &routes {
            for (owner, elements) in [(0, left), (1, right), (1, &routes[0]), (0, &routes[0])] {
                let change = incremental.replace_owner_route(owner, snapshot(elements));
                incremental.refresh_score_after_route_change(&change);
                full.replace_owner_route(owner, snapshot(elements));
                full.refresh_score_full();
                assert_eq!(incremental.score, full.score);
                if incremental.cycle_penalty == 0 {
                    assert_eq!(incremental.makespan, full.makespan);
                    assert_eq!(incremental.max_finish_nodes, full.max_finish_nodes);
                }
            }
        }
    }
}

#[test]
fn maximum_count_handles_ties_and_last_achiever_removal() {
    let mut state = state();
    assert_eq!(state.makespan, 5);
    assert_eq!(state.max_finish_nodes, 3);
    state.replace_earliest(1, 0);
    assert_eq!((state.makespan, state.max_finish_nodes), (5, 2));
    state.replace_earliest(3, 0);
    assert_eq!((state.makespan, state.max_finish_nodes), (5, 1));
    state.replace_earliest(5, 0);
    assert_eq!((state.makespan, state.max_finish_nodes), (3, 3));
    state.replace_earliest(0, 1);
    assert_eq!((state.makespan, state.max_finish_nodes), (3, 4));
    state.replace_earliest(0, 7);
    assert_eq!((state.makespan, state.max_finish_nodes), (9, 1));
}
