use std::collections::{BTreeMap, HashMap, VecDeque};

use pyo3::prelude::*;
use pyo3::types::PyDict;
use solverforge_core::score::Score;

use crate::error::py_err;
use crate::score::DynamicScore;
use crate::state::PyDynamicSolution;
use crate::value::DynamicValue;

type NodeId = usize;
type OwnerId = usize;
type DenseNodeId = usize;
type DenseEdge = (DenseNodeId, DenseNodeId);

pub type ListPrecedenceStateCache = BTreeMap<usize, DynamicListPrecedenceState>;

pub enum ListPrecedenceDeltaMode {
    Insert,
    Retract,
}

struct ListPrecedencePlan<'py> {
    score_family: String,
    variable_name: String,
    duration: Option<Bound<'py, PyAny>>,
    duration_field: Option<String>,
    successors: Option<Bound<'py, PyAny>>,
    successors_field: Option<String>,
    expected_owner: Option<Bound<'py, PyAny>>,
    expected_owner_field: Option<String>,
}

impl ListPrecedencePlan<'_> {
    fn uses_callbacks(&self) -> bool {
        self.duration.is_some() || self.successors.is_some() || self.expected_owner.is_some()
    }
}

#[derive(Default)]
struct RouteSnapshot {
    elements: Vec<DenseNodeId>,
    edges: Vec<DenseEdge>,
    invalid_count: usize,
    violation_count: usize,
}

#[derive(Default)]
struct RouteChange {
    added_edges: Vec<DenseEdge>,
    removed_edges: Vec<DenseEdge>,
}

impl RouteChange {
    fn is_empty(&self) -> bool {
        self.added_edges.is_empty() && self.removed_edges.is_empty()
    }

    fn seeds(&self) -> impl Iterator<Item = DenseNodeId> + '_ {
        self.added_edges
            .iter()
            .chain(self.removed_edges.iter())
            .map(|&(_, to)| to)
    }
}

pub fn is_list_precedence_plan(plan: &Bound<'_, PyDict>) -> PyResult<bool> {
    Ok(plan
        .get_item("constraint_type")?
        .map(|value| value.extract::<String>())
        .transpose()?
        .as_deref()
        == Some("list_precedence_makespan"))
}

pub fn evaluate_plan(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    descriptor_index: usize,
    plan: &Bound<'_, PyDict>,
) -> PyResult<DynamicScore> {
    let access = parse_plan(plan)?;
    let state = build_state(py, solution, descriptor_index, &access, None)?;
    Ok(state.score)
}

pub fn initialize_cache_entry(
    py: Python<'_>,
    cache: &mut ListPrecedenceStateCache,
    plan_index: usize,
    solution: &PyDynamicSolution,
    descriptor_index: usize,
    plan: &Bound<'_, PyDict>,
) -> PyResult<()> {
    let access = parse_plan(plan)?;
    let state = build_state(py, solution, descriptor_index, &access, None)?;
    cache.insert(plan_index, state);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn evaluate_delta(
    py: Python<'_>,
    cache: &mut ListPrecedenceStateCache,
    plan_index: usize,
    solution: &PyDynamicSolution,
    descriptor_index: usize,
    entity_index: usize,
    plan: &Bound<'_, PyDict>,
    mode: ListPrecedenceDeltaMode,
    retracted_entities: &std::collections::BTreeSet<(usize, usize)>,
) -> PyResult<DynamicScore> {
    let access = parse_plan(plan)?;
    if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(plan_index) {
        let state = build_state(
            py,
            solution,
            descriptor_index,
            &access,
            Some(retracted_entities),
        )?;
        entry.insert(state);
    }
    let Some(state) = cache.get_mut(&plan_index) else {
        return Ok(DynamicScore::zero());
    };
    if entity_index >= state.owner_edges.len() {
        return Ok(DynamicScore::zero());
    }
    let variable_index = list_variable_index(solution, descriptor_index, &access.variable_name)?;
    let before = state.score;
    let change = match mode {
        ListPrecedenceDeltaMode::Insert => state.add_owner_route(
            py,
            solution,
            descriptor_index,
            entity_index,
            &access,
            variable_index,
        )?,
        ListPrecedenceDeltaMode::Retract => state.remove_owner_route(entity_index),
    };
    let after = state.refresh_score_after_route_change(&change);
    Ok(after - before)
}

fn parse_plan<'py>(plan: &Bound<'py, PyDict>) -> PyResult<ListPrecedencePlan<'py>> {
    Ok(ListPrecedencePlan {
        score_family: required_plan_string(plan, "score_family")?,
        variable_name: required_plan_string(plan, "variable_name")?,
        duration: optional_plan_callback(plan, "precedence_duration")?,
        duration_field: optional_plan_string(plan, "precedence_duration_field")?,
        successors: optional_plan_callback(plan, "precedence_successors")?,
        successors_field: optional_plan_string(plan, "precedence_successors_field")?,
        expected_owner: optional_plan_callback(plan, "element_owner")?,
        expected_owner_field: optional_plan_string(plan, "element_owner_field")?,
    })
}

fn build_state(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    descriptor_index: usize,
    access: &ListPrecedencePlan<'_>,
    retracted_entities: Option<&std::collections::BTreeSet<(usize, usize)>>,
) -> PyResult<DynamicListPrecedenceState> {
    let variable_index = list_variable_index(solution, descriptor_index, &access.variable_name)?;
    let nodes = list_elements_for_variable(solution, descriptor_index, &access.variable_name)?;
    let node_to_index = nodes
        .iter()
        .copied()
        .enumerate()
        .map(|(index, node)| (node, index))
        .collect::<BTreeMap<_, _>>();
    let callback_solution = if access.uses_callbacks() {
        Some(solution.to_python_callback_view(py)?)
    } else {
        None
    };
    let durations = nodes
        .iter()
        .map(|node| precedence_duration(py, solution, access, callback_solution.as_ref(), *node))
        .collect::<PyResult<Vec<_>>>()?;
    let owner_count = solution
        .state
        .entities
        .get(descriptor_index)
        .map_or(0, Vec::len);
    let mut state = DynamicListPrecedenceState::new(
        access.score_family.clone(),
        nodes.to_vec(),
        node_to_index,
        owner_count,
        durations,
    );

    for node in state.nodes.clone() {
        let successors =
            precedence_successors(py, solution, access, callback_solution.as_ref(), node)?;
        for successor in successors {
            match (state.index_for_node(node), state.index_for_node(successor)) {
                (Some(from), Some(to)) => {
                    state.add_edge((from, to));
                }
                _ => state.invalid_fixed_edges += 1,
            }
        }
    }

    for owner in 0..owner_count {
        if retracted_entities.is_some_and(|entities| entities.contains(&(descriptor_index, owner)))
        {
            continue;
        }
        let change = state.add_owner_route(
            py,
            solution,
            descriptor_index,
            owner,
            access,
            variable_index,
        )?;
        debug_assert!(change.removed_edges.is_empty());
    }
    state.refresh_score_full();
    Ok(state)
}

pub struct DynamicListPrecedenceState {
    score_family: String,
    nodes: Vec<NodeId>,
    node_to_index: BTreeMap<NodeId, DenseNodeId>,
    durations: Vec<i64>,
    edge_counts: Vec<Vec<(DenseNodeId, usize)>>,
    successors: Vec<Vec<DenseNodeId>>,
    predecessors: Vec<Vec<DenseNodeId>>,
    assigned_counts: Vec<usize>,
    owner_elements: Vec<Vec<DenseNodeId>>,
    owner_edges: Vec<Vec<DenseEdge>>,
    owner_invalid_counts: Vec<usize>,
    owner_violation_counts: Vec<usize>,
    invalid_fixed_edges: usize,
    owner_invalid_total: usize,
    owner_violation_total: usize,
    assignment_penalty: usize,
    cycle_penalty: usize,
    cycle_added_edges: Vec<DenseEdge>,
    earliest: Vec<i64>,
    finishes: Vec<i64>,
    score: DynamicScore,
    hard_penalty: usize,
    makespan: i64,
}

impl DynamicListPrecedenceState {
    fn new(
        score_family: String,
        nodes: Vec<NodeId>,
        node_to_index: BTreeMap<NodeId, DenseNodeId>,
        owner_count: usize,
        durations: Vec<i64>,
    ) -> Self {
        let node_count = nodes.len();
        Self {
            score_family,
            nodes,
            node_to_index,
            durations,
            edge_counts: vec![Vec::new(); node_count],
            successors: vec![Vec::new(); node_count],
            predecessors: vec![Vec::new(); node_count],
            assigned_counts: vec![0; node_count],
            owner_elements: vec![Vec::new(); owner_count],
            owner_edges: vec![Vec::new(); owner_count],
            owner_invalid_counts: vec![0; owner_count],
            owner_violation_counts: vec![0; owner_count],
            invalid_fixed_edges: 0,
            owner_invalid_total: 0,
            owner_violation_total: 0,
            assignment_penalty: node_count,
            cycle_penalty: 0,
            cycle_added_edges: Vec::new(),
            earliest: vec![0; node_count],
            finishes: vec![0; node_count],
            score: DynamicScore::zero(),
            hard_penalty: 0,
            makespan: 0,
        }
    }

    fn index_for_node(&self, node: NodeId) -> Option<DenseNodeId> {
        self.node_to_index.get(&node).copied()
    }

    fn add_edge(&mut self, edge: DenseEdge) -> bool {
        if let Some((_, count)) = self.edge_counts[edge.0]
            .iter_mut()
            .find(|(to, _)| *to == edge.1)
        {
            *count += 1;
            false
        } else {
            self.edge_counts[edge.0].push((edge.1, 1));
            self.successors[edge.0].push(edge.1);
            self.predecessors[edge.1].push(edge.0);
            true
        }
    }

    fn remove_edge(&mut self, edge: DenseEdge) -> bool {
        let Some(pos) = self.edge_counts[edge.0]
            .iter()
            .position(|(to, _)| *to == edge.1)
        else {
            return false;
        };
        let count = &mut self.edge_counts[edge.0][pos].1;
        *count -= 1;
        if *count == 0 {
            self.edge_counts[edge.0].swap_remove(pos);
            remove_node(&mut self.successors[edge.0], edge.1);
            remove_node(&mut self.predecessors[edge.1], edge.0);
            true
        } else {
            false
        }
    }

    fn adjust_assignment(&mut self, node: DenseNodeId, new_count: usize) {
        let old_count = self.assigned_counts[node];
        if old_count == new_count {
            return;
        }
        let old_penalty = assignment_penalty(old_count);
        let new_penalty = assignment_penalty(new_count);
        if new_penalty >= old_penalty {
            self.assignment_penalty += new_penalty - old_penalty;
        } else {
            self.assignment_penalty -= old_penalty - new_penalty;
        }
        self.assigned_counts[node] = new_count;
    }

    fn add_owner_route(
        &mut self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
        descriptor_index: usize,
        owner: OwnerId,
        access: &ListPrecedencePlan<'_>,
        variable_index: usize,
    ) -> PyResult<RouteChange> {
        let snapshot = self.owner_route_snapshot(
            py,
            solution,
            descriptor_index,
            owner,
            access,
            variable_index,
        )?;
        Ok(self.replace_owner_route(owner, snapshot))
    }

    fn remove_owner_route(&mut self, owner: OwnerId) -> RouteChange {
        self.replace_owner_route(owner, RouteSnapshot::default())
    }

    fn owner_route_snapshot(
        &self,
        py: Python<'_>,
        solution: &PyDynamicSolution,
        descriptor_index: usize,
        owner: OwnerId,
        access: &ListPrecedencePlan<'_>,
        variable_index: usize,
    ) -> PyResult<RouteSnapshot> {
        let mut snapshot = RouteSnapshot::default();
        let values = solution
            .state
            .entities
            .get(descriptor_index)
            .and_then(|rows| rows.get(owner))
            .and_then(|row| row.list_at(variable_index))
            .map(<[usize]>::to_vec)
            .unwrap_or_default();
        let callback_solution = if access.expected_owner.is_some() {
            Some(solution.to_python_callback_view(py)?)
        } else {
            None
        };
        let mut previous = None;
        for value in values {
            let Some(node) = self.index_for_node(value) else {
                snapshot.invalid_count += 1;
                previous = None;
                continue;
            };
            snapshot.elements.push(node);
            if expected_owner_mismatch(
                py,
                solution,
                access,
                callback_solution.as_ref(),
                owner,
                value,
            )? {
                snapshot.violation_count += 1;
            }
            if let Some(from) = previous {
                snapshot.edges.push((from, node));
            }
            previous = Some(node);
        }
        Ok(snapshot)
    }

    fn replace_owner_route(&mut self, owner: OwnerId, snapshot: RouteSnapshot) -> RouteChange {
        let mut change = RouteChange::default();
        let RouteSnapshot {
            elements,
            edges,
            invalid_count,
            violation_count,
        } = snapshot;

        let old_elements = std::mem::take(&mut self.owner_elements[owner]);
        self.diff_owner_assignments(&old_elements, &elements);
        self.owner_elements[owner] = elements;

        let old_edges = std::mem::take(&mut self.owner_edges[owner]);
        self.diff_owner_edges(&old_edges, &edges, &mut change);
        self.owner_edges[owner] = edges;

        self.owner_invalid_total -= self.owner_invalid_counts[owner];
        self.owner_invalid_total += invalid_count;
        self.owner_invalid_counts[owner] = invalid_count;

        self.owner_violation_total -= self.owner_violation_counts[owner];
        self.owner_violation_total += violation_count;
        self.owner_violation_counts[owner] = violation_count;

        change
    }

    fn diff_owner_assignments(
        &mut self,
        old_elements: &[DenseNodeId],
        new_elements: &[DenseNodeId],
    ) {
        let mut counts = HashMap::<DenseNodeId, (usize, usize)>::new();
        for &node in old_elements {
            counts.entry(node).or_default().0 += 1;
        }
        for &node in new_elements {
            counts.entry(node).or_default().1 += 1;
        }

        for (node, (old_count, new_count)) in counts {
            if old_count == new_count {
                continue;
            }
            let current = self.assigned_counts[node];
            let updated = current.saturating_sub(old_count).saturating_add(new_count);
            self.adjust_assignment(node, updated);
        }
    }

    fn diff_owner_edges(
        &mut self,
        old_edges: &[DenseEdge],
        new_edges: &[DenseEdge],
        change: &mut RouteChange,
    ) {
        let mut counts = HashMap::<DenseEdge, (usize, usize)>::new();
        for &edge in old_edges {
            counts.entry(edge).or_default().0 += 1;
        }
        for &edge in new_edges {
            counts.entry(edge).or_default().1 += 1;
        }

        for (edge, (old_count, new_count)) in counts {
            if old_count > new_count {
                for _ in 0..(old_count - new_count) {
                    if self.remove_edge(edge) {
                        change.removed_edges.push(edge);
                    }
                }
            } else if new_count > old_count {
                for _ in 0..(new_count - old_count) {
                    if self.add_edge(edge) {
                        change.added_edges.push(edge);
                    }
                }
            }
        }
    }

    fn refresh_score_full(&mut self) -> DynamicScore {
        self.rebuild_graph_summary();
        self.refresh_score_from_cached_graph()
    }

    fn refresh_score_after_route_change(&mut self, change: &RouteChange) -> DynamicScore {
        self.refresh_graph_after_route_change(change);
        self.refresh_score_from_cached_graph()
    }

    fn refresh_score_from_cached_graph(&mut self) -> DynamicScore {
        let hard_penalty = self.invalid_fixed_edges
            + self.owner_invalid_total
            + self.owner_violation_total
            + self.assignment_penalty
            + self.cycle_penalty;
        self.hard_penalty = hard_penalty;
        self.score =
            precedence_score_for_family(self.score_family.as_str(), hard_penalty, self.makespan);
        self.score
    }

    fn refresh_graph_after_route_change(&mut self, change: &RouteChange) {
        if change.is_empty() {
            return;
        }

        if self.cycle_penalty > 0 {
            if self.recover_cached_cycle(change) {
                return;
            }
            self.rebuild_graph_summary();
            return;
        }

        if self.added_edges_introduce_cycle(&change.added_edges) {
            if change.removed_edges.is_empty() {
                self.mark_cyclic_from_route_change(change);
            } else {
                self.mark_cyclic_without_cache();
            }
            return;
        }

        let mut queued = vec![false; self.nodes.len()];
        let mut queue = VecDeque::new();
        for node in change.seeds() {
            if node < self.nodes.len() && !queued[node] {
                queued[node] = true;
                queue.push_back(node);
            }
        }

        while let Some(node) = queue.pop_front() {
            queued[node] = false;

            let new_earliest = self.predecessors[node]
                .iter()
                .map(|&predecessor| {
                    self.earliest[predecessor].saturating_add(self.durations[predecessor])
                })
                .max()
                .unwrap_or(0);
            if new_earliest == self.earliest[node] {
                continue;
            }

            self.replace_earliest(node, new_earliest);
            for &successor in &self.successors[node] {
                if !queued[successor] {
                    queued[successor] = true;
                    queue.push_back(successor);
                }
            }
        }
        self.makespan = self.max_finish();
    }

    fn rebuild_graph_summary(&mut self) {
        let mut indegree: Vec<usize> = self.predecessors.iter().map(Vec::len).collect();
        let mut earliest = vec![0i64; self.nodes.len()];
        let mut finishes = vec![0i64; self.nodes.len()];
        let mut ready = VecDeque::new();
        for (node, &degree) in indegree.iter().enumerate() {
            if degree == 0 {
                ready.push_back(node);
            }
        }

        let mut processed = 0usize;
        let mut makespan = 0i64;
        while let Some(node) = ready.pop_front() {
            processed += 1;
            let finish = earliest[node].saturating_add(self.durations[node]);
            finishes[node] = finish;
            makespan = makespan.max(finish);
            for &successor in &self.successors[node] {
                earliest[successor] = earliest[successor].max(finish);
                indegree[successor] -= 1;
                if indegree[successor] == 0 {
                    ready.push_back(successor);
                }
            }
        }

        if processed < self.nodes.len() {
            self.mark_cyclic_without_cache();
        } else {
            self.earliest = earliest;
            self.finishes = finishes;
            self.cycle_penalty = 0;
            self.cycle_added_edges.clear();
            self.makespan = makespan;
        }
    }

    fn mark_cyclic_from_route_change(&mut self, change: &RouteChange) {
        self.cycle_added_edges = change.added_edges.clone();
        self.cycle_penalty = self.nodes.len();
        self.makespan = 0;
    }

    fn mark_cyclic_without_cache(&mut self) {
        self.earliest.fill(0);
        self.finishes.fill(0);
        self.cycle_added_edges.clear();
        self.cycle_penalty = self.nodes.len();
        self.makespan = 0;
    }

    fn recover_cached_cycle(&mut self, change: &RouteChange) -> bool {
        if self.cycle_added_edges.is_empty() || !change.added_edges.is_empty() {
            return false;
        }
        if self
            .cycle_added_edges
            .iter()
            .any(|&edge| self.contains_edge(edge))
        {
            return false;
        }
        self.cycle_penalty = 0;
        self.cycle_added_edges.clear();
        self.makespan = self.max_finish();
        true
    }

    fn replace_earliest(&mut self, node: DenseNodeId, new_earliest: i64) {
        let old_finish = self.finishes[node];
        self.earliest[node] = new_earliest;
        let new_finish = new_earliest.saturating_add(self.durations[node]);
        self.finishes[node] = new_finish;
        if new_finish >= self.makespan {
            self.makespan = new_finish;
        } else if old_finish == self.makespan {
            self.makespan = self.max_finish();
        }
    }

    fn max_finish(&self) -> i64 {
        self.finishes.iter().copied().max().unwrap_or(0)
    }

    fn added_edges_introduce_cycle(&self, added_edges: &[DenseEdge]) -> bool {
        if added_edges.is_empty() {
            return false;
        }

        let mut visited = vec![0usize; self.nodes.len()];
        let mut stack = Vec::new();
        for (visit_id, &(from, to)) in added_edges.iter().enumerate() {
            if self.reaches_with_scratch(to, from, visit_id + 1, &mut visited, &mut stack) {
                return true;
            }
        }
        false
    }

    fn reaches_with_scratch(
        &self,
        start: DenseNodeId,
        target: DenseNodeId,
        visit_id: usize,
        visited: &mut [usize],
        stack: &mut Vec<DenseNodeId>,
    ) -> bool {
        if start == target {
            return true;
        }
        stack.clear();
        stack.push(start);
        while let Some(node) = stack.pop() {
            if visited[node] == visit_id {
                continue;
            }
            visited[node] = visit_id;
            for &successor in &self.successors[node] {
                if successor == target {
                    return true;
                }
                if visited[successor] != visit_id {
                    stack.push(successor);
                }
            }
        }
        false
    }

    fn contains_edge(&self, edge: DenseEdge) -> bool {
        self.edge_counts[edge.0]
            .iter()
            .any(|&(to, count)| to == edge.1 && count > 0)
    }
}

fn list_variable_index(
    solution: &PyDynamicSolution,
    entity_index: usize,
    variable_name: &str,
) -> PyResult<usize> {
    solution
        .schema()
        .entities
        .get(entity_index)
        .and_then(|entity| {
            entity.variables.iter().position(|variable| {
                variable.kind == "planning_list_variable" && variable.name.as_str() == variable_name
            })
        })
        .ok_or_else(|| {
            py_err(format!(
                "missing planning list variable `{variable_name}` for entity index `{entity_index}`"
            ))
        })
}

fn list_elements_for_variable<'a>(
    solution: &'a PyDynamicSolution,
    entity_index: usize,
    variable_name: &str,
) -> PyResult<&'a [usize]> {
    let variable_index = list_variable_index(solution, entity_index, variable_name)?;
    solution
        .state
        .list_elements_at(entity_index, variable_index)
        .ok_or_else(|| {
            py_err(format!(
                "planning list variable `{variable_name}` has no element collection"
            ))
        })
}

fn assignment_penalty(count: usize) -> usize {
    match count {
        0 => 1,
        1 => 0,
        extra => extra - 1,
    }
}

fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn precedence_duration(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    access: &ListPrecedencePlan<'_>,
    callback_solution: Option<&Py<PyAny>>,
    node: usize,
) -> PyResult<i64> {
    if let Some(field_name) = access.duration_field.as_deref() {
        if let Some(value) = list_metadata_usize(solution, field_name, node) {
            return Ok(usize_to_i64(value));
        }
    }
    let Some(callback) = access.duration.as_ref() else {
        return Err(py_err("list precedence duration metadata is missing"));
    };
    let Some(callback_solution) = callback_solution else {
        return Err(py_err("list precedence callback solution is missing"));
    };
    callback
        .call1((callback_solution.bind(py), node))?
        .extract::<usize>()
        .map(usize_to_i64)
}

fn precedence_successors(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    access: &ListPrecedencePlan<'_>,
    callback_solution: Option<&Py<PyAny>>,
    node: usize,
) -> PyResult<Vec<usize>> {
    if let Some(field_name) = access.successors_field.as_deref() {
        if let Some(values) = list_metadata_usize_list(solution, field_name, node) {
            return Ok(values);
        }
    }
    let Some(callback) = access.successors.as_ref() else {
        return Ok(Vec::new());
    };
    let Some(callback_solution) = callback_solution else {
        return Err(py_err("list precedence callback solution is missing"));
    };
    let result = callback.call1((callback_solution.bind(py), node))?;
    if result.is_none() {
        Ok(Vec::new())
    } else {
        result.extract::<Vec<usize>>()
    }
}

fn expected_owner_mismatch(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    access: &ListPrecedencePlan<'_>,
    callback_solution: Option<&Py<PyAny>>,
    owner: usize,
    value: usize,
) -> PyResult<bool> {
    if let Some(field_name) = access.expected_owner_field.as_deref() {
        return Ok(list_metadata_usize(solution, field_name, value)
            .is_some_and(|expected| expected != owner));
    }
    let Some(callback) = access.expected_owner.as_ref() else {
        return Ok(false);
    };
    let Some(callback_solution) = callback_solution else {
        return Err(py_err("list precedence callback solution is missing"));
    };
    let expected = callback.call1((callback_solution.bind(py), value))?;
    if expected.is_none() {
        return Ok(false);
    }
    Ok(expected
        .extract::<usize>()
        .is_ok_and(|expected| expected != owner))
}

fn list_metadata_value<'a>(
    solution: &'a PyDynamicSolution,
    field_name: &str,
    element: usize,
) -> Option<&'a DynamicValue> {
    let DynamicValue::List(values) = solution.state.solution_fields.get(field_name)? else {
        return None;
    };
    values.get(element)
}

fn list_metadata_usize(
    solution: &PyDynamicSolution,
    field_name: &str,
    element: usize,
) -> Option<usize> {
    dynamic_value_i64(list_metadata_value(solution, field_name, element)?)
        .and_then(|value| usize::try_from(value).ok())
}

fn list_metadata_usize_list(
    solution: &PyDynamicSolution,
    field_name: &str,
    element: usize,
) -> Option<Vec<usize>> {
    let DynamicValue::List(values) = list_metadata_value(solution, field_name, element)? else {
        return None;
    };
    values
        .iter()
        .map(|value| dynamic_value_i64(value).and_then(|value| usize::try_from(value).ok()))
        .collect()
}

fn dynamic_value_i64(value: &DynamicValue) -> Option<i64> {
    match value {
        DynamicValue::Int(value) => Some(*value),
        _ => None,
    }
}

fn precedence_score_for_family(
    score_family: &str,
    hard_penalty: usize,
    makespan: i64,
) -> DynamicScore {
    let hard = -usize_to_i64(hard_penalty);
    let soft = makespan.saturating_neg();
    match score_family {
        "soft" => DynamicScore::soft(soft.saturating_add(hard)),
        "hard_soft_decimal" => DynamicScore::hard_soft_decimal(hard, soft),
        "hard_medium_soft" => DynamicScore::hard_medium_soft(hard, 0, soft),
        _ => DynamicScore::hard_soft(hard, soft),
    }
}

fn remove_node(nodes: &mut Vec<DenseNodeId>, node: DenseNodeId) {
    let Some(pos) = nodes.iter().position(|&candidate| candidate == node) else {
        return;
    };
    nodes.swap_remove(pos);
}

fn required_plan_string(plan: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    plan.get_item(key)?
        .ok_or_else(|| py_err(format!("constraint missing {key}")))?
        .extract::<String>()
}

fn optional_plan_callback<'py>(
    plan: &Bound<'py, PyDict>,
    key: &str,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let Some(value) = plan.get_item(key)? else {
        return Ok(None);
    };
    if value.is_none() {
        Ok(None)
    } else if value.is_callable() {
        Ok(Some(value))
    } else {
        Err(py_err(format!("constraint field `{key}` must be callable")))
    }
}

fn optional_plan_string(plan: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<String>> {
    let Some(value) = plan.get_item(key)? else {
        return Ok(None);
    };
    if value.is_none() {
        Ok(None)
    } else {
        value.extract::<String>().map(Some)
    }
}
