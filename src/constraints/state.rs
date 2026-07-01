use std::collections::{BTreeMap, BTreeSet};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use solverforge_core::score::Score;
use solverforge_scoring::api::constraint_set::{
    ConstraintMetadata, ConstraintResult, ConstraintSet,
};
use solverforge_scoring::ConstraintAnalysis;

use super::evaluate;
use super::list_precedence::{self, ListPrecedenceDeltaMode, ListPrecedenceStateCache};
use crate::error::py_err;
use crate::score::{dynamic_score_from_native, DynamicScore};
use crate::state::PyDynamicSolution;

struct PythonRowSet {
    type_name: String,
    rows: Vec<Py<PyAny>>,
}

#[pyclass(name = "Run")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct PyRun {
    start: i64,
    end: i64,
    point_count: usize,
    item_count: usize,
}

#[pymethods]
impl PyRun {
    fn start(&self) -> i64 {
        self.start
    }

    fn end(&self) -> i64 {
        self.end
    }

    fn point_count(&self) -> usize {
        self.point_count
    }

    fn item_count(&self) -> usize {
        self.item_count
    }
}

#[pyclass(name = "Runs")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PyRuns {
    runs: Vec<PyRun>,
    point_count: usize,
    item_count: usize,
}

#[pymethods]
impl PyRuns {
    fn runs(&self) -> Vec<PyRun> {
        self.runs.clone()
    }

    fn point_count(&self) -> usize {
        self.point_count
    }

    fn item_count(&self) -> usize {
        self.item_count
    }

    fn len(&self) -> usize {
        self.runs.len()
    }

    fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

#[pyclass(name = "IndexedPresence")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PyIndexedPresence {
    points: BTreeMap<i64, usize>,
    item_count: usize,
}

impl PyIndexedPresence {
    fn insert(&mut self, point: i64) {
        *self.points.entry(point).or_insert(0) += 1;
        self.item_count += 1;
    }
}

#[pymethods]
impl PyIndexedPresence {
    fn contains(&self, index: i64) -> bool {
        self.points.contains_key(&index)
    }

    fn count(&self) -> usize {
        self.points.len()
    }

    fn item_count(&self) -> usize {
        self.item_count
    }

    fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    fn runs(&self) -> PyRuns {
        runs_from_counts(&self.points, self.item_count)
    }

    fn complement_runs(&self, start: i64, end: i64) -> PyRuns {
        if start >= end {
            return PyRuns::default();
        }
        let mut complement = BTreeMap::new();
        let mut index = start;
        while index < end {
            if !self.points.contains_key(&index) {
                complement.insert(index, 1);
            }
            let Some(next) = index.checked_add(1) else {
                break;
            };
            index = next;
        }
        let item_count = complement.values().sum();
        runs_from_counts(&complement, item_count)
    }

    fn count_in(&self, start: i64, end: i64) -> usize {
        if start >= end {
            return 0;
        }
        self.points.range(start..end).count()
    }

    fn any_in(&self, start: i64, end: i64) -> bool {
        self.count_in(start, end) > 0
    }
}

fn runs_from_counts(points: &BTreeMap<i64, usize>, item_count: usize) -> PyRuns {
    let point_count = points.len();
    let mut runs = Vec::new();
    let mut current_start = None;
    let mut previous = 0;
    let mut current_point_count = 0;
    let mut current_item_count = 0;

    for (&point, &count) in points {
        match current_start {
            None => {
                current_start = Some(point);
                previous = point;
                current_point_count = 1;
                current_item_count = count;
            }
            Some(_) if previous.checked_add(1) == Some(point) => {
                previous = point;
                current_point_count += 1;
                current_item_count += count;
            }
            Some(start) => {
                runs.push(PyRun {
                    start,
                    end: previous,
                    point_count: current_point_count,
                    item_count: current_item_count,
                });
                current_start = Some(point);
                previous = point;
                current_point_count = 1;
                current_item_count = count;
            }
        }
    }

    if let Some(start) = current_start {
        runs.push(PyRun {
            start,
            end: previous,
            point_count: current_point_count,
            item_count: current_item_count,
        });
    }

    PyRuns {
        runs,
        point_count,
        item_count,
    }
}

type JoinKeyCache = BTreeMap<(usize, u8), Vec<Py<PyAny>>>;
type JoinIndexCache = BTreeMap<(usize, u8), Vec<(Py<PyAny>, Vec<usize>)>>;
type BalanceCountCache = BTreeMap<usize, Vec<(Py<PyAny>, i64)>>;
type ListUnassignedCountCache = BTreeMap<usize, BTreeMap<usize, i64>>;
type RetractedEntitySet = BTreeSet<(usize, usize)>;

#[derive(Clone, Copy)]
enum DeltaMode {
    Insert,
    Retract,
}

#[derive(Clone, Copy)]
struct DeltaTarget {
    entity_index: usize,
    descriptor_index: usize,
    mode: DeltaMode,
}

struct ConstraintStateCaches<'a> {
    join_key: &'a mut JoinKeyCache,
    join_index: &'a mut JoinIndexCache,
    balance_count: &'a mut BalanceCountCache,
    list_unassigned_count: &'a mut ListUnassignedCountCache,
    list_precedence: &'a mut ListPrecedenceStateCache,
}

#[derive(Clone, Copy)]
struct WeightedPlan<'a, 'py> {
    impact: &'a str,
    weight: DynamicScore,
    weight_callback: Option<&'a Bound<'py, PyAny>>,
}

struct BinaryPlanArgs<'a, 'py> {
    row_sets: &'a [PythonRowSet],
    left_index: usize,
    right_index: usize,
    left_filters: Bound<'py, PyList>,
    right_filters: Bound<'py, PyList>,
    filters: &'a Bound<'py, PyList>,
    joiners: Option<&'a Bound<'py, PyList>>,
    weighted: WeightedPlan<'a, 'py>,
}

struct GroupedPlanArgs<'a, 'py> {
    row_sets: &'a [PythonRowSet],
    entity_index: usize,
    filters: &'a Bound<'py, PyList>,
    group_filters: &'a Bound<'py, PyList>,
    group_key: &'a Bound<'py, PyAny>,
    group_collector: Option<&'a Bound<'py, PyDict>>,
    activity: GroupActivity<'a>,
    weighted: WeightedPlan<'a, 'py>,
}

enum GroupAggregate {
    Count(usize),
    IndexedPresence(PyIndexedPresence),
}

struct DynamicGroup {
    key: Py<PyAny>,
    aggregate: GroupAggregate,
}

#[derive(Clone, Copy)]
enum GroupActivity<'a> {
    All,
    BeforeDelta {
        retracted_entities: &'a RetractedEntitySet,
    },
    AfterDelta {
        retracted_entities: &'a RetractedEntitySet,
        target: DeltaTarget,
    },
}

#[derive(Clone, Copy)]
struct JoinKeySide<'a> {
    plan_index: usize,
    side: u8,
    rows: &'a [Py<PyAny>],
    joiners: &'a [EqualJoiner],
}

pub struct PyDynamicConstraintSet {
    constraints: Py<PyAny>,
    cached_rows: Option<Vec<PythonRowSet>>,
    join_key_cache: JoinKeyCache,
    join_index_cache: JoinIndexCache,
    balance_count_cache: BalanceCountCache,
    list_unassigned_count_cache: ListUnassignedCountCache,
    list_precedence_cache: ListPrecedenceStateCache,
    retracted_entities: RetractedEntitySet,
}

unsafe impl Send for PyDynamicConstraintSet {}
unsafe impl Sync for PyDynamicConstraintSet {}

impl PyDynamicConstraintSet {
    pub fn new(constraints: Py<PyAny>) -> Self {
        Self {
            constraints,
            cached_rows: None,
            join_key_cache: JoinKeyCache::new(),
            join_index_cache: JoinIndexCache::new(),
            balance_count_cache: BalanceCountCache::new(),
            list_unassigned_count_cache: ListUnassignedCountCache::new(),
            list_precedence_cache: ListPrecedenceStateCache::new(),
            retracted_entities: RetractedEntitySet::new(),
        }
    }

    pub fn evaluate_python(
        &self,
        py: Python<'_>,
        py_solution: &Bound<'_, PyAny>,
    ) -> PyResult<DynamicScore> {
        evaluate::evaluate_constraints(py, py_solution, self.constraints.bind(py))
    }

    pub fn evaluate_solution(&self, solution: &PyDynamicSolution) -> PyResult<DynamicScore> {
        Python::attach(|py| evaluate_state_constraints(py, solution, self.constraints.bind(py)))
    }

    fn initialize_incremental_state(
        &mut self,
        solution: &PyDynamicSolution,
    ) -> PyResult<DynamicScore> {
        Python::attach(|py| {
            let row_sets = python_rows_by_type(py, solution)?;
            let score = evaluate_state_constraint_plans(
                py,
                solution,
                &row_sets,
                self.constraints.bind(py),
            )?;
            self.join_key_cache.clear();
            self.join_index_cache.clear();
            self.balance_count_cache.clear();
            self.list_unassigned_count_cache.clear();
            self.list_precedence_cache.clear();
            self.retracted_entities.clear();
            initialize_list_precedence_cache(
                py,
                solution,
                &row_sets,
                self.constraints.bind(py),
                &mut self.list_precedence_cache,
            )?;
            self.cached_rows = Some(row_sets);
            Ok(score)
        })
    }

    fn reset_incremental_state(&mut self) {
        self.cached_rows = None;
        self.join_key_cache.clear();
        self.join_index_cache.clear();
        self.balance_count_cache.clear();
        self.list_unassigned_count_cache.clear();
        self.list_precedence_cache.clear();
        self.retracted_entities.clear();
    }
}

impl ConstraintSet<PyDynamicSolution, DynamicScore> for PyDynamicConstraintSet {
    fn evaluate_all(&self, solution: &PyDynamicSolution) -> DynamicScore {
        Python::attach(|py| evaluate_state_constraints(py, solution, self.constraints.bind(py)))
            .unwrap_or_else(|err| panic!("python constraint callback failed: {err:?}"))
    }

    fn constraint_count(&self) -> usize {
        Python::attach(|py| {
            self.constraints
                .bind(py)
                .cast::<PyList>()
                .map(|items| items.len())
                .unwrap_or(0)
        })
    }

    fn constraint_metadata_entries(&self) -> Vec<ConstraintMetadata<'_>> {
        Vec::new()
    }

    fn evaluate_each<'a>(
        &'a self,
        _solution: &PyDynamicSolution,
    ) -> Vec<ConstraintResult<'a, DynamicScore>> {
        Vec::new()
    }

    fn evaluate_detailed<'a>(
        &'a self,
        _solution: &PyDynamicSolution,
    ) -> Vec<ConstraintAnalysis<'a, DynamicScore>> {
        Vec::new()
    }

    fn initialize_all(&mut self, solution: &PyDynamicSolution) -> DynamicScore {
        self.initialize_incremental_state(solution)
            .unwrap_or_else(|err| panic!("python constraint callback failed: {err:?}"))
    }

    fn on_insert_all(
        &mut self,
        solution: &PyDynamicSolution,
        entity_index: usize,
        descriptor_index: usize,
    ) -> DynamicScore {
        let delta = Python::attach(|py| {
            ensure_cached_rows(py, &mut self.cached_rows, solution)?;
            let row_sets = self
                .cached_rows
                .as_mut()
                .expect("cached rows must exist after ensure_cached_rows");
            sync_cached_entity_row(py, row_sets, solution, descriptor_index, entity_index)?;
            let mut caches = ConstraintStateCaches {
                join_key: &mut self.join_key_cache,
                join_index: &mut self.join_index_cache,
                balance_count: &mut self.balance_count_cache,
                list_unassigned_count: &mut self.list_unassigned_count_cache,
                list_precedence: &mut self.list_precedence_cache,
            };
            evaluate_impacted_state_constraints(
                py,
                solution,
                row_sets,
                &mut caches,
                &self.retracted_entities,
                self.constraints.bind(py),
                DeltaTarget {
                    entity_index,
                    descriptor_index,
                    mode: DeltaMode::Insert,
                },
            )
        })
        .unwrap_or_else(|err| panic!("python constraint callback failed: {err:?}"));
        self.retracted_entities
            .remove(&(descriptor_index, entity_index));
        delta
    }

    fn on_retract_all(
        &mut self,
        solution: &PyDynamicSolution,
        entity_index: usize,
        descriptor_index: usize,
    ) -> DynamicScore {
        let delta = Python::attach(|py| {
            ensure_cached_rows(py, &mut self.cached_rows, solution)?;
            let mut caches = ConstraintStateCaches {
                join_key: &mut self.join_key_cache,
                join_index: &mut self.join_index_cache,
                balance_count: &mut self.balance_count_cache,
                list_unassigned_count: &mut self.list_unassigned_count_cache,
                list_precedence: &mut self.list_precedence_cache,
            };
            evaluate_impacted_state_constraints(
                py,
                solution,
                self.cached_rows
                    .as_ref()
                    .expect("cached rows must exist after ensure_cached_rows"),
                &mut caches,
                &self.retracted_entities,
                self.constraints.bind(py),
                DeltaTarget {
                    entity_index,
                    descriptor_index,
                    mode: DeltaMode::Retract,
                },
            )
        })
        .unwrap_or_else(|err| panic!("python constraint callback failed: {err:?}"));
        self.retracted_entities
            .insert((descriptor_index, entity_index));
        delta
    }

    fn reset_all(&mut self) {
        self.reset_incremental_state();
    }
}

fn evaluate_state_constraints(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    constraints: &Bound<'_, PyAny>,
) -> PyResult<DynamicScore> {
    let row_sets = python_rows_by_type(py, solution)?;
    evaluate_state_constraint_plans(py, solution, &row_sets, constraints)
}

fn evaluate_state_constraint_plans(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    row_sets: &[PythonRowSet],
    constraints: &Bound<'_, PyAny>,
) -> PyResult<DynamicScore> {
    let constraints = constraints.cast::<PyList>()?;
    let mut total = DynamicScore::zero();
    for plan_any in constraints.iter() {
        let plan = plan_any.cast::<PyDict>()?;
        let entity_type = plan
            .get_item("entity_type")?
            .ok_or_else(|| py_err("constraint missing entity_type"))?
            .extract::<String>()?;
        let entity_index = row_sets
            .iter()
            .position(|row_set| row_set.type_name == entity_type)
            .ok_or_else(|| py_err(format!("unknown constraint entity type `{entity_type}`")))?;
        let filters_any = plan
            .get_item("filters")?
            .ok_or_else(|| py_err("constraint missing filters"))?;
        let filters = filters_any.cast::<PyList>()?;
        let joiners_any = plan.get_item("joiners")?;
        let joiners = match joiners_any.as_ref() {
            Some(value) => Some(value.cast::<PyList>()?),
            None => None,
        };
        let impact = plan
            .get_item("impact")?
            .ok_or_else(|| py_err("constraint missing impact"))?
            .extract::<String>()?;
        let weight_any = plan
            .get_item("weight")?
            .ok_or_else(|| py_err("constraint missing weight"))?;
        let weight = dynamic_score_from_native(&weight_any)?;
        let weight_callback = plan.get_item("weight_callback")?;
        let constraint_type = plan
            .get_item("constraint_type")?
            .map(|value| value.extract::<String>())
            .transpose()?;
        if constraint_type.as_deref() == Some("list_precedence_makespan") {
            total = total + list_precedence::evaluate_plan(py, solution, entity_index, plan)?;
            continue;
        }
        if constraint_type.as_deref() == Some("list_unassigned_element") {
            total = evaluate_list_unassigned_plan(
                py,
                total,
                solution,
                entity_index,
                filters,
                plan,
                WeightedPlan {
                    impact: impact.as_str(),
                    weight,
                    weight_callback: weight_callback.as_ref(),
                },
            )?;
            continue;
        }
        if let Some(balance_key) = plan.get_item("balance_key")? {
            total = evaluate_balance_plan(
                total,
                row_sets,
                entity_index,
                filters,
                balance_key,
                WeightedPlan {
                    impact: impact.as_str(),
                    weight,
                    weight_callback: weight_callback.as_ref(),
                },
            )?;
            continue;
        }
        if let Some(group_key) = plan.get_item("group_key")? {
            let group_filters = optional_list(py, plan, "group_filters")?;
            let group_collector = optional_dict(plan, "group_collector")?;
            total = evaluate_grouped_plan(
                py,
                total,
                GroupedPlanArgs {
                    row_sets,
                    entity_index,
                    filters,
                    group_filters: &group_filters,
                    group_key: &group_key,
                    group_collector: group_collector.as_ref(),
                    activity: GroupActivity::All,
                    weighted: WeightedPlan {
                        impact: impact.as_str(),
                        weight,
                        weight_callback: weight_callback.as_ref(),
                    },
                },
            )?;
            continue;
        }

        let arity = plan
            .get_item("arity")?
            .map(|value| value.extract::<usize>())
            .transpose()?
            .unwrap_or(1);
        match arity {
            1 => {
                for entity in row_sets
                    .get(entity_index)
                    .into_iter()
                    .flat_map(|row_set| row_set.rows.iter())
                {
                    let entity = entity.bind(py);
                    if passes_unary_filters(filters, entity)? {
                        let score = match_score_unary(weight, weight_callback.as_ref(), entity)?;
                        total = apply_impact(total, impact.as_str(), score);
                    }
                }
            }
            2 => {
                let right_entity_type = plan
                    .get_item("right_entity_type")?
                    .ok_or_else(|| py_err("binary constraint missing `right_entity_type`"))?
                    .extract::<String>()?;
                let right_entity_index = row_sets
                    .iter()
                    .position(|row_set| row_set.type_name == right_entity_type)
                    .ok_or_else(|| {
                        py_err(format!(
                            "unknown binary constraint entity type `{right_entity_type}`"
                        ))
                    })?;
                total = evaluate_binary_plan(
                    py,
                    total,
                    BinaryPlanArgs {
                        row_sets,
                        left_index: entity_index,
                        right_index: right_entity_index,
                        left_filters: optional_list(py, plan, "left_filters")?,
                        right_filters: optional_list(py, plan, "right_filters")?,
                        filters,
                        joiners,
                        weighted: WeightedPlan {
                            impact: impact.as_str(),
                            weight,
                            weight_callback: weight_callback.as_ref(),
                        },
                    },
                )?;
            }
            other => {
                return Err(py_err(format!(
                    "unsupported dynamic constraint arity `{other}`"
                )));
            }
        }
    }
    Ok(total)
}

fn initialize_list_precedence_cache(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    row_sets: &[PythonRowSet],
    constraints: &Bound<'_, PyAny>,
    cache: &mut ListPrecedenceStateCache,
) -> PyResult<()> {
    let constraints = constraints.cast::<PyList>()?;
    for (plan_index, plan_any) in constraints.iter().enumerate() {
        let plan = plan_any.cast::<PyDict>()?;
        if !list_precedence::is_list_precedence_plan(plan)? {
            continue;
        }
        let entity_type = plan
            .get_item("entity_type")?
            .ok_or_else(|| py_err("constraint missing entity_type"))?
            .extract::<String>()?;
        let entity_index = row_sets
            .iter()
            .position(|row_set| row_set.type_name == entity_type)
            .ok_or_else(|| py_err(format!("unknown constraint entity type `{entity_type}`")))?;
        list_precedence::initialize_cache_entry(
            py,
            cache,
            plan_index,
            solution,
            entity_index,
            plan,
        )?;
    }
    Ok(())
}

fn evaluate_impacted_state_constraints(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    row_sets: &[PythonRowSet],
    caches: &mut ConstraintStateCaches<'_>,
    retracted_entities: &RetractedEntitySet,
    constraints: &Bound<'_, PyAny>,
    target: DeltaTarget,
) -> PyResult<DynamicScore> {
    let constraints = constraints.cast::<PyList>()?;
    let mut total = DynamicScore::zero();
    for (plan_index, plan_any) in constraints.iter().enumerate() {
        let plan = plan_any.cast::<PyDict>()?;
        let entity_type = plan
            .get_item("entity_type")?
            .ok_or_else(|| py_err("constraint missing entity_type"))?
            .extract::<String>()?;
        let entity_index = row_sets
            .iter()
            .position(|row_set| row_set.type_name == entity_type)
            .ok_or_else(|| py_err(format!("unknown constraint entity type `{entity_type}`")))?;
        let filters_any = plan
            .get_item("filters")?
            .ok_or_else(|| py_err("constraint missing filters"))?;
        let filters = filters_any.cast::<PyList>()?;
        let joiners_any = plan.get_item("joiners")?;
        let joiners = match joiners_any.as_ref() {
            Some(value) => Some(value.cast::<PyList>()?),
            None => None,
        };
        let impact = plan
            .get_item("impact")?
            .ok_or_else(|| py_err("constraint missing impact"))?
            .extract::<String>()?;
        let weight_any = plan
            .get_item("weight")?
            .ok_or_else(|| py_err("constraint missing weight"))?;
        let weight = dynamic_score_from_native(&weight_any)?;
        let weight_callback = plan.get_item("weight_callback")?;
        let constraint_type = plan
            .get_item("constraint_type")?
            .map(|value| value.extract::<String>())
            .transpose()?;
        if constraint_type.as_deref() == Some("list_precedence_makespan") {
            if entity_index == target.descriptor_index {
                let mode = match target.mode {
                    DeltaMode::Insert => ListPrecedenceDeltaMode::Insert,
                    DeltaMode::Retract => ListPrecedenceDeltaMode::Retract,
                };
                total = total
                    + list_precedence::evaluate_delta(
                        py,
                        caches.list_precedence,
                        plan_index,
                        solution,
                        entity_index,
                        target.entity_index,
                        plan,
                        mode,
                        retracted_entities,
                    )?;
            }
            continue;
        }
        if constraint_type.as_deref() == Some("list_unassigned_element") {
            if entity_index == target.descriptor_index {
                total = evaluate_list_unassigned_delta(
                    py,
                    total,
                    solution,
                    caches.list_unassigned_count,
                    plan_index,
                    entity_index,
                    target.entity_index,
                    filters,
                    plan,
                    WeightedPlan {
                        impact: impact.as_str(),
                        weight,
                        weight_callback: weight_callback.as_ref(),
                    },
                    target.mode,
                    retracted_entities,
                )?;
            }
            continue;
        }

        if let Some(balance_key) = plan.get_item("balance_key")? {
            if entity_index == target.descriptor_index {
                total = evaluate_balance_delta(
                    py,
                    total,
                    caches.balance_count,
                    plan_index,
                    row_sets,
                    retracted_entities,
                    entity_index,
                    target.entity_index,
                    filters,
                    balance_key,
                    WeightedPlan {
                        impact: impact.as_str(),
                        weight,
                        weight_callback: weight_callback.as_ref(),
                    },
                    target.mode,
                )?;
            }
            continue;
        }
        if let Some(group_key) = plan.get_item("group_key")? {
            if entity_index == target.descriptor_index {
                let group_filters = optional_list(py, plan, "group_filters")?;
                let group_collector = optional_dict(plan, "group_collector")?;
                let before = evaluate_grouped_plan(
                    py,
                    DynamicScore::zero(),
                    GroupedPlanArgs {
                        row_sets,
                        entity_index,
                        filters,
                        group_filters: &group_filters,
                        group_key: &group_key,
                        group_collector: group_collector.as_ref(),
                        activity: GroupActivity::BeforeDelta { retracted_entities },
                        weighted: WeightedPlan {
                            impact: impact.as_str(),
                            weight,
                            weight_callback: weight_callback.as_ref(),
                        },
                    },
                )?;
                let after = evaluate_grouped_plan(
                    py,
                    DynamicScore::zero(),
                    GroupedPlanArgs {
                        row_sets,
                        entity_index,
                        filters,
                        group_filters: &group_filters,
                        group_key: &group_key,
                        group_collector: group_collector.as_ref(),
                        activity: GroupActivity::AfterDelta {
                            retracted_entities,
                            target,
                        },
                        weighted: WeightedPlan {
                            impact: impact.as_str(),
                            weight,
                            weight_callback: weight_callback.as_ref(),
                        },
                    },
                )?;
                total = total + (after - before);
            }
            continue;
        }

        let arity = plan
            .get_item("arity")?
            .map(|value| value.extract::<usize>())
            .transpose()?
            .unwrap_or(1);
        match arity {
            1 => {
                if entity_index != target.descriptor_index {
                    continue;
                }
                let Some(entity) = row_sets
                    .get(entity_index)
                    .and_then(|row_set| row_set.rows.get(target.entity_index))
                else {
                    continue;
                };
                if row_is_retracted(
                    retracted_entities,
                    entity_index,
                    target.entity_index,
                    target,
                ) {
                    continue;
                }
                let entity = entity.bind(py);
                if passes_unary_filters(filters, entity)? {
                    let score = match_score_unary(weight, weight_callback.as_ref(), entity)?;
                    total = apply_delta_impact(total, target.mode, impact.as_str(), score);
                }
            }
            2 => {
                let right_entity_type = plan
                    .get_item("right_entity_type")?
                    .ok_or_else(|| py_err("binary constraint missing `right_entity_type`"))?
                    .extract::<String>()?;
                let right_entity_index = row_sets
                    .iter()
                    .position(|row_set| row_set.type_name == right_entity_type)
                    .ok_or_else(|| {
                        py_err(format!(
                            "unknown binary constraint entity type `{right_entity_type}`"
                        ))
                    })?;
                if entity_index != target.descriptor_index
                    && right_entity_index != target.descriptor_index
                {
                    continue;
                }
                total = evaluate_impacted_binary_plan(
                    py,
                    total,
                    row_sets,
                    caches.join_key,
                    caches.join_index,
                    plan_index,
                    entity_index,
                    right_entity_index,
                    target.entity_index,
                    target.descriptor_index,
                    retracted_entities,
                    target,
                    optional_list(py, plan, "left_filters")?,
                    optional_list(py, plan, "right_filters")?,
                    filters,
                    joiners,
                    impact.as_str(),
                    weight,
                    weight_callback.as_ref(),
                    target.mode,
                )?;
            }
            other => {
                return Err(py_err(format!(
                    "unsupported dynamic constraint arity `{other}`"
                )));
            }
        }
    }
    Ok(total)
}

fn evaluate_balance_plan(
    total: DynamicScore,
    row_sets: &[PythonRowSet],
    entity_index: usize,
    filters: &Bound<'_, PyList>,
    balance_key: Bound<'_, PyAny>,
    weighted: WeightedPlan<'_, '_>,
) -> PyResult<DynamicScore> {
    let rows = row_sets
        .get(entity_index)
        .map(|row_set| &row_set.rows)
        .ok_or_else(|| {
            py_err(format!(
                "missing state rows for entity index `{entity_index}`"
            ))
        })?;
    let py = balance_key.py();
    let mut counts = Vec::<(Py<PyAny>, i64)>::new();
    for entity in rows {
        let entity = entity.bind(py);
        if !passes_unary_filters(filters, entity)? {
            continue;
        }
        let key = balance_key.call1((entity,))?;
        if key.is_none() {
            continue;
        }
        increment_key_count(py, &mut counts, key, 1)?;
    }
    Ok(total + balance_score_from_counts(&counts, weighted)?)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_balance_delta(
    py: Python<'_>,
    total: DynamicScore,
    balance_count_cache: &mut BalanceCountCache,
    plan_index: usize,
    row_sets: &[PythonRowSet],
    retracted_entities: &RetractedEntitySet,
    entity_index: usize,
    changed_entity_index: usize,
    filters: &Bound<'_, PyList>,
    balance_key: Bound<'_, PyAny>,
    weighted: WeightedPlan<'_, '_>,
    mode: DeltaMode,
) -> PyResult<DynamicScore> {
    ensure_balance_counts(
        balance_count_cache,
        plan_index,
        row_sets,
        entity_index,
        filters,
        &balance_key,
        retracted_entities,
    )?;
    let counts = balance_count_cache
        .get_mut(&plan_index)
        .expect("balance counts must exist after ensure_balance_counts");
    let before = balance_score_from_counts(counts, weighted)?;
    let Some(row) = row_sets
        .get(entity_index)
        .and_then(|row_set| row_set.rows.get(changed_entity_index))
    else {
        return Ok(total);
    };
    let row = row.bind(py);
    if passes_unary_filters(filters, row)? {
        let key = balance_key.call1((row,))?;
        if !key.is_none() {
            match mode {
                DeltaMode::Insert => {
                    increment_key_count(py, counts, key, 1)?;
                }
                DeltaMode::Retract => {
                    decrement_key_count(py, counts, &key)?;
                }
            }
        }
    }
    let after = balance_score_from_counts(counts, weighted)?;
    Ok(total + (after - before))
}

fn evaluate_list_unassigned_plan(
    py: Python<'_>,
    mut total: DynamicScore,
    solution: &PyDynamicSolution,
    entity_index: usize,
    filters: &Bound<'_, PyList>,
    plan: &Bound<'_, PyDict>,
    weighted: WeightedPlan<'_, '_>,
) -> PyResult<DynamicScore> {
    let variable_name = required_plan_string(plan, "variable_name")?;
    let counts = assigned_list_counts(solution, entity_index, variable_name.as_str(), None)?;
    for element in list_elements_for_variable(solution, entity_index, variable_name.as_str())? {
        if counts.get(element).copied().unwrap_or(0) == 0 {
            total = total + list_unassigned_contribution(py, filters, weighted, *element)?;
        }
    }
    Ok(total)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_list_unassigned_delta(
    py: Python<'_>,
    mut total: DynamicScore,
    solution: &PyDynamicSolution,
    list_unassigned_count_cache: &mut ListUnassignedCountCache,
    plan_index: usize,
    entity_index: usize,
    changed_entity_index: usize,
    filters: &Bound<'_, PyList>,
    plan: &Bound<'_, PyDict>,
    weighted: WeightedPlan<'_, '_>,
    mode: DeltaMode,
    retracted_entities: &RetractedEntitySet,
) -> PyResult<DynamicScore> {
    let variable_name = required_plan_string(plan, "variable_name")?;
    ensure_list_unassigned_counts(
        list_unassigned_count_cache,
        plan_index,
        solution,
        entity_index,
        variable_name.as_str(),
        retracted_entities,
    )?;
    let universe = list_elements_for_variable(solution, entity_index, variable_name.as_str())?;
    let Some(changed_values) = solution
        .state
        .entities
        .get(entity_index)
        .and_then(|rows| rows.get(changed_entity_index))
        .and_then(|row| row.lists.get(variable_name.as_str()))
    else {
        return Ok(total);
    };
    let counts = list_unassigned_count_cache
        .get_mut(&plan_index)
        .expect("list unassigned counts must exist after ensure_list_unassigned_counts");
    for element in changed_values {
        let before = counts.get(element).copied().unwrap_or(0);
        let after = match mode {
            DeltaMode::Insert => before + 1,
            DeltaMode::Retract => before.saturating_sub(1),
        };
        if after == 0 {
            counts.remove(element);
        } else {
            counts.insert(*element, after);
        }
        if !universe.contains(element) {
            continue;
        }
        let contribution = list_unassigned_contribution(py, filters, weighted, *element)?;
        match mode {
            DeltaMode::Insert if before == 0 && after == 1 => {
                total = total - contribution;
            }
            DeltaMode::Retract if before == 1 && after == 0 => {
                total = total + contribution;
            }
            _ => {}
        }
    }
    Ok(total)
}

fn ensure_list_unassigned_counts(
    list_unassigned_count_cache: &mut ListUnassignedCountCache,
    plan_index: usize,
    solution: &PyDynamicSolution,
    entity_index: usize,
    variable_name: &str,
    retracted_entities: &RetractedEntitySet,
) -> PyResult<()> {
    if list_unassigned_count_cache.contains_key(&plan_index) {
        return Ok(());
    }
    list_unassigned_count_cache.insert(
        plan_index,
        assigned_list_counts(
            solution,
            entity_index,
            variable_name,
            Some(retracted_entities),
        )?,
    );
    Ok(())
}

fn assigned_list_counts(
    solution: &PyDynamicSolution,
    entity_index: usize,
    variable_name: &str,
    retracted_entities: Option<&RetractedEntitySet>,
) -> PyResult<BTreeMap<usize, i64>> {
    let rows = solution.state.entities.get(entity_index).ok_or_else(|| {
        py_err(format!(
            "missing state rows for entity index `{entity_index}`"
        ))
    })?;
    let mut counts = BTreeMap::new();
    for (row_index, row) in rows.iter().enumerate() {
        if retracted_entities.is_some_and(|entities| entities.contains(&(entity_index, row_index)))
        {
            continue;
        }
        for element in row.lists.get(variable_name).into_iter().flatten() {
            *counts.entry(*element).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

fn list_elements_for_variable<'a>(
    solution: &'a PyDynamicSolution,
    entity_index: usize,
    variable_name: &str,
) -> PyResult<&'a [usize]> {
    solution
        .state
        .list_elements
        .get(entity_index)
        .and_then(|variables| variables.get(variable_name))
        .map(Vec::as_slice)
        .ok_or_else(|| {
            py_err(format!(
                "planning list variable `{variable_name}` has no element collection"
            ))
        })
}

fn list_unassigned_contribution(
    _py: Python<'_>,
    filters: &Bound<'_, PyList>,
    weighted: WeightedPlan<'_, '_>,
    element: usize,
) -> PyResult<DynamicScore> {
    if !passes_element_filters(filters, element)? {
        return Ok(DynamicScore::zero());
    }
    let score = match_score_element(weighted.weight, weighted.weight_callback, element)?;
    Ok(apply_impact(DynamicScore::zero(), weighted.impact, score))
}

fn ensure_balance_counts(
    balance_count_cache: &mut BalanceCountCache,
    plan_index: usize,
    row_sets: &[PythonRowSet],
    entity_index: usize,
    filters: &Bound<'_, PyList>,
    balance_key: &Bound<'_, PyAny>,
    retracted_entities: &RetractedEntitySet,
) -> PyResult<()> {
    if balance_count_cache.contains_key(&plan_index) {
        return Ok(());
    }
    let rows = row_sets
        .get(entity_index)
        .map(|row_set| &row_set.rows)
        .ok_or_else(|| {
            py_err(format!(
                "missing state rows for entity index `{entity_index}`"
            ))
        })?;
    let py = balance_key.py();
    let mut counts = Vec::<(Py<PyAny>, i64)>::new();
    for (row_index, entity) in rows.iter().enumerate() {
        if retracted_entities.contains(&(entity_index, row_index)) {
            continue;
        }
        let entity = entity.bind(py);
        if !passes_unary_filters(filters, entity)? {
            continue;
        }
        let key = balance_key.call1((entity,))?;
        if key.is_none() {
            continue;
        }
        increment_key_count(py, &mut counts, key, 1)?;
    }
    balance_count_cache.insert(plan_index, counts);
    Ok(())
}

fn balance_score_from_counts(
    counts: &[(Py<PyAny>, i64)],
    weighted: WeightedPlan<'_, '_>,
) -> PyResult<DynamicScore> {
    if counts.is_empty() {
        return Ok(DynamicScore::zero());
    }
    let group_count = counts.len() as f64;
    let total_count: i64 = counts.iter().map(|(_, count)| *count).sum();
    let sum_squared: i64 = counts.iter().map(|(_, count)| count * count).sum();
    let mean = total_count as f64 / group_count;
    let variance = (sum_squared as f64 / group_count) - (mean * mean);
    let std_dev = if variance > 0.0 { variance.sqrt() } else { 0.0 };
    let weight = match_score_balance(weighted.weight, weighted.weight_callback)?;
    Ok(apply_impact(
        DynamicScore::zero(),
        weighted.impact,
        weight.multiply(std_dev),
    ))
}

#[allow(clippy::too_many_arguments)]
fn evaluate_impacted_binary_plan(
    py: Python<'_>,
    mut total: DynamicScore,
    row_sets: &[PythonRowSet],
    join_key_cache: &mut JoinKeyCache,
    join_index_cache: &mut JoinIndexCache,
    plan_index: usize,
    left_index: usize,
    right_index: usize,
    changed_entity_index: usize,
    changed_descriptor_index: usize,
    retracted_entities: &RetractedEntitySet,
    target: DeltaTarget,
    left_filters: Bound<'_, PyList>,
    right_filters: Bound<'_, PyList>,
    filters: &Bound<'_, PyList>,
    joiners: Option<&Bound<'_, PyList>>,
    impact: &str,
    weight: DynamicScore,
    weight_callback: Option<&Bound<'_, PyAny>>,
    mode: DeltaMode,
) -> PyResult<DynamicScore> {
    let left_rows = row_sets
        .get(left_index)
        .map(|row_set| &row_set.rows)
        .ok_or_else(|| {
            py_err(format!(
                "missing state rows for entity index `{left_index}`"
            ))
        })?;
    let right_rows = row_sets
        .get(right_index)
        .map(|row_set| &row_set.rows)
        .ok_or_else(|| {
            py_err(format!(
                "missing state rows for entity index `{right_index}`"
            ))
        })?;
    let left_changed = left_index == changed_descriptor_index;
    let right_changed = right_index == changed_descriptor_index;
    if let Some(equal_joiners) = parse_equal_joiners(joiners)? {
        return evaluate_impacted_equal_join_plan(
            py,
            total,
            join_key_cache,
            join_index_cache,
            plan_index,
            left_index,
            right_index,
            left_rows,
            right_rows,
            &equal_joiners,
            changed_entity_index,
            left_changed,
            right_changed,
            retracted_entities,
            target,
            left_filters,
            right_filters,
            filters,
            impact,
            weight,
            weight_callback,
            mode,
        );
    }
    let mut seen = std::collections::HashSet::<(usize, usize)>::new();

    if left_changed {
        if let Some(left) = indexed_row_if_active_and_passes(
            py,
            left_rows,
            left_index,
            changed_entity_index,
            &left_filters,
            retracted_entities,
            target,
        )? {
            let left_bound = left.bind(py);
            for (right_row_index, right) in indexed_rows_maybe_filtered_active(
                py,
                right_rows,
                right_index,
                &right_filters,
                retracted_entities,
                target,
            )? {
                if !seen.insert((changed_entity_index, right_row_index)) {
                    continue;
                }
                let right_bound = right.bind(py);
                if !passes_joiners(joiners, left_bound, right_bound)? {
                    continue;
                }
                if !passes_binary_filters(Some(filters), left_bound, right_bound)? {
                    continue;
                }
                let score = match_score_binary(weight, weight_callback, left_bound, right_bound)?;
                total = apply_delta_impact(total, mode, impact, score);
            }
        }
    }

    if right_changed {
        if let Some(right) = indexed_row_if_active_and_passes(
            py,
            right_rows,
            right_index,
            changed_entity_index,
            &right_filters,
            retracted_entities,
            target,
        )? {
            let right_bound = right.bind(py);
            for (left_row_index, left) in indexed_rows_maybe_filtered_active(
                py,
                left_rows,
                left_index,
                &left_filters,
                retracted_entities,
                target,
            )? {
                if !seen.insert((left_row_index, changed_entity_index)) {
                    continue;
                }
                let left_bound = left.bind(py);
                if !passes_joiners(joiners, left_bound, right_bound)? {
                    continue;
                }
                if !passes_binary_filters(Some(filters), left_bound, right_bound)? {
                    continue;
                }
                let score = match_score_binary(weight, weight_callback, left_bound, right_bound)?;
                total = apply_delta_impact(total, mode, impact, score);
            }
        }
    }
    Ok(total)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_impacted_equal_join_plan(
    py: Python<'_>,
    mut total: DynamicScore,
    join_key_cache: &mut JoinKeyCache,
    join_index_cache: &mut JoinIndexCache,
    plan_index: usize,
    left_index: usize,
    right_index: usize,
    left_rows: &[Py<PyAny>],
    right_rows: &[Py<PyAny>],
    joiners: &[EqualJoiner],
    changed_entity_index: usize,
    left_changed: bool,
    right_changed: bool,
    retracted_entities: &RetractedEntitySet,
    target: DeltaTarget,
    left_filters: Bound<'_, PyList>,
    right_filters: Bound<'_, PyList>,
    filters: &Bound<'_, PyList>,
    impact: &str,
    weight: DynamicScore,
    weight_callback: Option<&Bound<'_, PyAny>>,
    mode: DeltaMode,
) -> PyResult<DynamicScore> {
    ensure_join_keys(
        py,
        join_key_cache,
        join_index_cache,
        JoinKeySide {
            plan_index,
            side: 0,
            rows: left_rows,
            joiners,
        },
    )?;
    ensure_join_keys(
        py,
        join_key_cache,
        join_index_cache,
        JoinKeySide {
            plan_index,
            side: 1,
            rows: right_rows,
            joiners,
        },
    )?;
    if left_changed {
        update_join_key(
            py,
            join_key_cache,
            join_index_cache,
            JoinKeySide {
                plan_index,
                side: 0,
                rows: left_rows,
                joiners,
            },
            changed_entity_index,
        )?;
    }
    if right_changed {
        update_join_key(
            py,
            join_key_cache,
            join_index_cache,
            JoinKeySide {
                plan_index,
                side: 1,
                rows: right_rows,
                joiners,
            },
            changed_entity_index,
        )?;
    }

    let left_keys = join_key_cache
        .get(&(plan_index, 0))
        .expect("left join key cache must exist");
    let right_keys = join_key_cache
        .get(&(plan_index, 1))
        .expect("right join key cache must exist");
    let left_index_by_key = join_index_cache
        .get(&(plan_index, 0))
        .expect("left join index cache must exist");
    let right_index_by_key = join_index_cache
        .get(&(plan_index, 1))
        .expect("right join index cache must exist");
    let mut seen = std::collections::HashSet::<(usize, usize)>::new();

    if left_changed {
        if let Some(left) = indexed_row_if_active_and_passes(
            py,
            left_rows,
            left_index,
            changed_entity_index,
            &left_filters,
            retracted_entities,
            target,
        )? {
            if let Some(left_key) = left_keys.get(changed_entity_index) {
                let left_bound = left.bind(py);
                if let Some(right_indices) = find_join_indices(py, right_index_by_key, left_key)? {
                    for right_row_index in right_indices {
                        if !seen.insert((changed_entity_index, *right_row_index)) {
                            continue;
                        }
                        let Some(right) = indexed_row_if_active_and_passes(
                            py,
                            right_rows,
                            right_index,
                            *right_row_index,
                            &right_filters,
                            retracted_entities,
                            target,
                        )?
                        else {
                            continue;
                        };
                        let right_bound = right.bind(py);
                        if !passes_binary_filters(Some(filters), left_bound, right_bound)? {
                            continue;
                        }
                        let score =
                            match_score_binary(weight, weight_callback, left_bound, right_bound)?;
                        total = apply_delta_impact(total, mode, impact, score);
                    }
                }
            }
        }
    }

    if right_changed {
        if let Some(right) = indexed_row_if_active_and_passes(
            py,
            right_rows,
            right_index,
            changed_entity_index,
            &right_filters,
            retracted_entities,
            target,
        )? {
            if let Some(right_key) = right_keys.get(changed_entity_index) {
                let right_bound = right.bind(py);
                if let Some(left_indices) = find_join_indices(py, left_index_by_key, right_key)? {
                    for left_row_index in left_indices {
                        if !seen.insert((*left_row_index, changed_entity_index)) {
                            continue;
                        }
                        let Some(left) = indexed_row_if_active_and_passes(
                            py,
                            left_rows,
                            left_index,
                            *left_row_index,
                            &left_filters,
                            retracted_entities,
                            target,
                        )?
                        else {
                            continue;
                        };
                        let left_bound = left.bind(py);
                        if !passes_binary_filters(Some(filters), left_bound, right_bound)? {
                            continue;
                        }
                        let score =
                            match_score_binary(weight, weight_callback, left_bound, right_bound)?;
                        total = apply_delta_impact(total, mode, impact, score);
                    }
                }
            }
        }
    }

    Ok(total)
}

fn python_rows_by_type(
    py: Python<'_>,
    solution: &PyDynamicSolution,
) -> PyResult<Vec<PythonRowSet>> {
    let mut rows_by_type = Vec::new();
    for (entity_index, rows) in solution.state.entities.iter().enumerate() {
        let mut python_rows = Vec::with_capacity(rows.len());
        for row_index in 0..rows.len() {
            python_rows.push(solution.entity_callback_view(py, entity_index, row_index)?);
        }
        rows_by_type.push(PythonRowSet {
            type_name: solution.schema.entities[entity_index].type_name.clone(),
            rows: python_rows,
        });
    }
    for (fact_index, rows) in solution.state.facts.iter().enumerate() {
        let mut python_rows = Vec::with_capacity(rows.len());
        for row in rows {
            python_rows.push(state_row_to_python_without_variables(py, row)?);
        }
        rows_by_type.push(PythonRowSet {
            type_name: solution.schema.facts[fact_index].type_name.clone(),
            rows: python_rows,
        });
    }
    Ok(rows_by_type)
}

fn ensure_cached_rows(
    py: Python<'_>,
    cached_rows: &mut Option<Vec<PythonRowSet>>,
    solution: &PyDynamicSolution,
) -> PyResult<()> {
    if cached_rows.is_none() {
        *cached_rows = Some(python_rows_by_type(py, solution)?);
    }
    Ok(())
}

fn sync_cached_entity_row(
    py: Python<'_>,
    row_sets: &mut [PythonRowSet],
    solution: &PyDynamicSolution,
    descriptor_index: usize,
    entity_index: usize,
) -> PyResult<()> {
    let Some(py_row) = row_sets
        .get_mut(descriptor_index)
        .and_then(|row_set| row_set.rows.get_mut(entity_index))
    else {
        return Ok(());
    };
    let refreshed = solution.entity_callback_view(py, descriptor_index, entity_index)?;
    *py_row = refreshed;
    Ok(())
}

fn evaluate_binary_plan(
    py: Python<'_>,
    mut total: DynamicScore,
    args: BinaryPlanArgs<'_, '_>,
) -> PyResult<DynamicScore> {
    let left_rows = args
        .row_sets
        .get(args.left_index)
        .map(|row_set| &row_set.rows)
        .ok_or_else(|| {
            py_err(format!(
                "missing state rows for entity index `{}`",
                args.left_index
            ))
        })?;
    let right_rows = args
        .row_sets
        .get(args.right_index)
        .map(|row_set| &row_set.rows)
        .ok_or_else(|| {
            py_err(format!(
                "missing state rows for entity index `{}`",
                args.right_index
            ))
        })?;
    let left_rows = filtered_rows(py, left_rows, &args.left_filters)?;
    let right_rows = filtered_rows(py, right_rows, &args.right_filters)?;

    if let Some(equal_joiners) = parse_equal_joiners(args.joiners)? {
        return evaluate_equal_join_plan(
            py,
            total,
            &left_rows,
            &right_rows,
            &equal_joiners,
            args.filters,
            args.weighted.impact,
            args.weighted.weight,
            args.weighted.weight_callback,
        );
    }

    for left in &left_rows {
        let left = left.bind(py);
        for right in &right_rows {
            let right = right.bind(py);
            if !passes_joiners(args.joiners, left, right)? {
                continue;
            }
            if !passes_binary_filters(Some(args.filters), left, right)? {
                continue;
            }
            let score = match_score_binary(
                args.weighted.weight,
                args.weighted.weight_callback,
                left,
                right,
            )?;
            total = apply_impact(total, args.weighted.impact, score);
        }
    }
    Ok(total)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_equal_join_plan(
    py: Python<'_>,
    mut total: DynamicScore,
    left_rows: &[&Py<PyAny>],
    right_rows: &[&Py<PyAny>],
    joiners: &[EqualJoiner],
    filters: &Bound<'_, PyList>,
    impact: &str,
    weight: DynamicScore,
    weight_callback: Option<&Bound<'_, PyAny>>,
) -> PyResult<DynamicScore> {
    let mut right_by_key = Vec::<(Py<PyAny>, Vec<&Py<PyAny>>)>::new();
    for right in right_rows {
        let right_bound = right.bind(py);
        let key = joined_key(
            py,
            joiners
                .iter()
                .map(|joiner| joiner.right_key.bind(py).call1((right_bound,))),
        )?;
        if let Some(index) = find_row_group_index(py, &right_by_key, key.bind(py))? {
            right_by_key[index].1.push(*right);
        } else {
            right_by_key.push((key, vec![*right]));
        }
    }

    for left in left_rows {
        let left_bound = left.bind(py);
        let key = joined_key(
            py,
            joiners
                .iter()
                .map(|joiner| joiner.left_key.bind(py).call1((left_bound,))),
        )?;
        let Some(group_index) = find_row_group_index(py, &right_by_key, key.bind(py))? else {
            continue;
        };
        let candidates = &right_by_key[group_index].1;
        for right in candidates {
            let right_bound = right.bind(py);
            if !passes_binary_filters(Some(filters), left_bound, right_bound)? {
                continue;
            }
            let score = match_score_binary(weight, weight_callback, left_bound, right_bound)?;
            total = apply_impact(total, impact, score);
        }
    }
    Ok(total)
}

fn evaluate_grouped_plan(
    py: Python<'_>,
    mut total: DynamicScore,
    args: GroupedPlanArgs<'_, '_>,
) -> PyResult<DynamicScore> {
    let rows = args
        .row_sets
        .get(args.entity_index)
        .map(|row_set| &row_set.rows)
        .ok_or_else(|| {
            py_err(format!(
                "missing state rows for entity index `{}`",
                args.entity_index
            ))
        })?;
    let mut groups = Vec::<DynamicGroup>::new();
    for (row_index, entity) in rows.iter().enumerate() {
        if !grouped_row_active(args.activity, args.entity_index, row_index) {
            continue;
        }
        let entity = entity.bind(py);
        if !passes_unary_filters(args.filters, entity)? {
            continue;
        }
        let key = args.group_key.call1((entity,))?;
        if key.is_none() {
            continue;
        }
        accumulate_group_value(py, &mut groups, key, entity, args.group_collector)?;
    }
    for group in groups {
        let key = group.key.bind(py);
        if !passes_group_filters(py, args.group_filters, key, &group.aggregate)? {
            continue;
        }
        let score = match_score_group(
            py,
            args.weighted.weight,
            args.weighted.weight_callback,
            key,
            &group.aggregate,
        )?;
        total = apply_impact(total, args.weighted.impact, score);
    }
    Ok(total)
}

fn accumulate_group_value(
    py: Python<'_>,
    groups: &mut Vec<DynamicGroup>,
    key: Bound<'_, PyAny>,
    entity: &Bound<'_, PyAny>,
    collector: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let Some(collector) = collector else {
        return increment_group_count(py, groups, key);
    };
    let collector_type = required_plan_string(collector, "type")?;
    if collector_type != "indexed_presence" {
        return Err(py_err(format!(
            "unsupported dynamic group collector `{collector_type}`"
        )));
    }
    let index_callback = collector
        .get_item("index")?
        .ok_or_else(|| py_err("indexed_presence collector missing `index`"))?;
    let index = index_callback.call1((entity,))?;
    if index.is_none() {
        return Ok(());
    }
    increment_group_presence(py, groups, key, index.extract::<i64>()?)
}

fn grouped_row_active(
    activity: GroupActivity<'_>,
    descriptor_index: usize,
    row_index: usize,
) -> bool {
    match activity {
        GroupActivity::All => true,
        GroupActivity::BeforeDelta { retracted_entities } => {
            !retracted_entities.contains(&(descriptor_index, row_index))
        }
        GroupActivity::AfterDelta {
            retracted_entities,
            target,
        } => {
            let is_retracted = retracted_entities.contains(&(descriptor_index, row_index));
            let is_target =
                descriptor_index == target.descriptor_index && row_index == target.entity_index;
            match target.mode {
                DeltaMode::Insert => !is_retracted || is_target,
                DeltaMode::Retract => !(is_retracted || is_target),
            }
        }
    }
}

fn increment_group_count(
    py: Python<'_>,
    groups: &mut Vec<DynamicGroup>,
    key: Bound<'_, PyAny>,
) -> PyResult<()> {
    if let Some(index) = find_group_index(py, groups, &key)? {
        match &mut groups[index].aggregate {
            GroupAggregate::Count(count) => *count += 1,
            GroupAggregate::IndexedPresence(_) => {
                return Err(py_err("group collector changed for existing key"));
            }
        }
    } else {
        groups.push(DynamicGroup {
            key: key.unbind(),
            aggregate: GroupAggregate::Count(1),
        });
    }
    Ok(())
}

fn increment_group_presence(
    py: Python<'_>,
    groups: &mut Vec<DynamicGroup>,
    key: Bound<'_, PyAny>,
    point: i64,
) -> PyResult<()> {
    if let Some(index) = find_group_index(py, groups, &key)? {
        match &mut groups[index].aggregate {
            GroupAggregate::IndexedPresence(presence) => presence.insert(point),
            GroupAggregate::Count(_) => {
                return Err(py_err("group collector changed for existing key"));
            }
        }
    } else {
        let mut presence = PyIndexedPresence::default();
        presence.insert(point);
        groups.push(DynamicGroup {
            key: key.unbind(),
            aggregate: GroupAggregate::IndexedPresence(presence),
        });
    }
    Ok(())
}

fn increment_key_count(
    py: Python<'_>,
    counts: &mut Vec<(Py<PyAny>, i64)>,
    key: Bound<'_, PyAny>,
    amount: i64,
) -> PyResult<()> {
    if let Some(index) = find_key_count_index(py, counts, &key)? {
        counts[index].1 += amount;
    } else {
        counts.push((key.unbind(), amount));
    }
    Ok(())
}

fn decrement_key_count(
    py: Python<'_>,
    counts: &mut Vec<(Py<PyAny>, i64)>,
    key: &Bound<'_, PyAny>,
) -> PyResult<()> {
    if let Some(index) = find_key_count_index(py, counts, key)? {
        counts[index].1 -= 1;
        if counts[index].1 <= 0 {
            counts.remove(index);
        }
    }
    Ok(())
}

fn find_group_index(
    py: Python<'_>,
    groups: &[DynamicGroup],
    key: &Bound<'_, PyAny>,
) -> PyResult<Option<usize>> {
    for (index, group) in groups.iter().enumerate() {
        if group.key.bind(py).eq(key)? {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn find_key_count_index(
    py: Python<'_>,
    counts: &[(Py<PyAny>, i64)],
    key: &Bound<'_, PyAny>,
) -> PyResult<Option<usize>> {
    for (index, (existing, _)) in counts.iter().enumerate() {
        if existing.bind(py).eq(key)? {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn find_row_group_index(
    py: Python<'_>,
    groups: &[(Py<PyAny>, Vec<&Py<PyAny>>)],
    key: &Bound<'_, PyAny>,
) -> PyResult<Option<usize>> {
    for (index, (existing, _)) in groups.iter().enumerate() {
        if existing.bind(py).eq(key)? {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn passes_unary_filters(filters: &Bound<'_, PyList>, entity: &Bound<'_, PyAny>) -> PyResult<bool> {
    for filter in filters.iter() {
        if !filter.call1((entity,))?.extract::<bool>()? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn passes_element_filters(filters: &Bound<'_, PyList>, element: usize) -> PyResult<bool> {
    for filter in filters.iter() {
        if !filter.call1((element,))?.extract::<bool>()? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn filtered_rows<'a>(
    py: Python<'_>,
    rows: &'a [Py<PyAny>],
    filters: &Bound<'_, PyList>,
) -> PyResult<Vec<&'a Py<PyAny>>> {
    let mut filtered = Vec::new();
    for row in rows {
        let bound = row.bind(py);
        if passes_unary_filters(filters, bound)? {
            filtered.push(row);
        }
    }
    Ok(filtered)
}

fn indexed_rows_maybe_filtered_active<'a>(
    py: Python<'_>,
    rows: &'a [Py<PyAny>],
    descriptor_index: usize,
    filters: &Bound<'_, PyList>,
    retracted_entities: &RetractedEntitySet,
    target: DeltaTarget,
) -> PyResult<Vec<(usize, &'a Py<PyAny>)>> {
    let mut filtered = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if row_is_retracted(retracted_entities, descriptor_index, index, target) {
            continue;
        }
        let bound = row.bind(py);
        if filters.is_empty() || passes_unary_filters(filters, bound)? {
            filtered.push((index, row));
        }
    }
    Ok(filtered)
}

fn indexed_row_if_passes<'a>(
    py: Python<'_>,
    rows: &'a [Py<PyAny>],
    index: usize,
    filters: &Bound<'_, PyList>,
) -> PyResult<Option<&'a Py<PyAny>>> {
    let Some(row) = rows.get(index) else {
        return Ok(None);
    };
    let bound = row.bind(py);
    if passes_unary_filters(filters, bound)? {
        Ok(Some(row))
    } else {
        Ok(None)
    }
}

fn indexed_row_if_active_and_passes<'a>(
    py: Python<'_>,
    rows: &'a [Py<PyAny>],
    descriptor_index: usize,
    index: usize,
    filters: &Bound<'_, PyList>,
    retracted_entities: &RetractedEntitySet,
    target: DeltaTarget,
) -> PyResult<Option<&'a Py<PyAny>>> {
    if row_is_retracted(retracted_entities, descriptor_index, index, target) {
        return Ok(None);
    }
    indexed_row_if_passes(py, rows, index, filters)
}

fn row_is_retracted(
    retracted_entities: &RetractedEntitySet,
    descriptor_index: usize,
    row_index: usize,
    target: DeltaTarget,
) -> bool {
    if !retracted_entities.contains(&(descriptor_index, row_index)) {
        return false;
    }
    !(matches!(target.mode, DeltaMode::Insert)
        && descriptor_index == target.descriptor_index
        && row_index == target.entity_index)
}

fn optional_list<'py>(
    py: Python<'py>,
    plan: &Bound<'py, PyDict>,
    key: &str,
) -> PyResult<Bound<'py, PyList>> {
    match plan.get_item(key)? {
        Some(value) => Ok(value.cast::<PyList>()?.clone()),
        None => Ok(PyList::empty(py)),
    }
}

fn optional_dict<'py>(
    plan: &Bound<'py, PyDict>,
    key: &str,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    match plan.get_item(key)? {
        Some(value) => Ok(Some(value.cast::<PyDict>()?.clone())),
        None => Ok(None),
    }
}

struct EqualJoiner {
    left_key: Py<PyAny>,
    right_key: Py<PyAny>,
}

fn parse_equal_joiners(joiners: Option<&Bound<'_, PyList>>) -> PyResult<Option<Vec<EqualJoiner>>> {
    let Some(joiners) = joiners else {
        return Ok(None);
    };
    if joiners.is_empty() {
        return Ok(None);
    }
    let mut parsed = Vec::new();
    for joiner in joiners.iter() {
        let Ok(dict) = joiner.cast::<PyDict>() else {
            return Ok(None);
        };
        let kind = dict
            .get_item("type")?
            .ok_or_else(|| py_err("joiner dictionary missing `type`"))?
            .extract::<String>()?;
        if kind != "equal" {
            return Ok(None);
        }
        let left_key = dict
            .get_item("left_key")?
            .ok_or_else(|| py_err("equal joiner missing `left_key`"))?
            .unbind();
        let right_key = dict
            .get_item("right_key")?
            .ok_or_else(|| py_err("equal joiner missing `right_key`"))?
            .unbind();
        parsed.push(EqualJoiner {
            left_key,
            right_key,
        });
    }
    Ok(Some(parsed))
}

fn joined_key<'py, I>(py: Python<'py>, parts: I) -> PyResult<Py<PyAny>>
where
    I: IntoIterator<Item = PyResult<Bound<'py, PyAny>>>,
{
    let mut values = Vec::new();
    for part in parts {
        values.push(part?.unbind());
    }
    Ok(PyTuple::new(py, values)?.into_any().unbind())
}

fn ensure_join_keys(
    py: Python<'_>,
    join_key_cache: &mut JoinKeyCache,
    join_index_cache: &mut JoinIndexCache,
    side: JoinKeySide<'_>,
) -> PyResult<()> {
    let cache_key = (side.plan_index, side.side);
    if join_key_cache.contains_key(&cache_key) {
        if let std::collections::btree_map::Entry::Vacant(entry) = join_index_cache.entry(cache_key)
        {
            let keys = join_key_cache
                .get(&cache_key)
                .expect("join key cache must exist");
            entry.insert(build_join_index(py, keys)?);
        }
        return Ok(());
    }
    let mut keys = Vec::with_capacity(side.rows.len());
    for row in side.rows {
        keys.push(equal_join_key_for_row(py, row, side.joiners, side.side)?);
    }
    join_index_cache.insert(cache_key, build_join_index(py, &keys)?);
    join_key_cache.insert(cache_key, keys);
    Ok(())
}

fn update_join_key(
    py: Python<'_>,
    join_key_cache: &mut JoinKeyCache,
    join_index_cache: &mut JoinIndexCache,
    side: JoinKeySide<'_>,
    row_index: usize,
) -> PyResult<()> {
    let Some(row) = side.rows.get(row_index) else {
        return Ok(());
    };
    let cache_key = (side.plan_index, side.side);
    let key = equal_join_key_for_row(py, row, side.joiners, side.side)?;
    if let Some(keys) = join_key_cache.get_mut(&cache_key) {
        if let Some(slot) = keys.get_mut(row_index) {
            *slot = key;
        }
        if let Some(index_by_key) = join_index_cache.get_mut(&cache_key) {
            remove_join_row(index_by_key, row_index);
            if let Some(updated_key) = keys.get(row_index) {
                insert_join_row(py, index_by_key, updated_key, row_index)?;
            }
        }
    }
    Ok(())
}

fn build_join_index(py: Python<'_>, keys: &[Py<PyAny>]) -> PyResult<Vec<(Py<PyAny>, Vec<usize>)>> {
    let mut index = Vec::<(Py<PyAny>, Vec<usize>)>::new();
    for (row_index, key) in keys.iter().enumerate() {
        insert_join_row(py, &mut index, key, row_index)?;
    }
    Ok(index)
}

fn insert_join_row(
    py: Python<'_>,
    index: &mut Vec<(Py<PyAny>, Vec<usize>)>,
    key: &Py<PyAny>,
    row_index: usize,
) -> PyResult<()> {
    if let Some(group_index) = find_join_group_index(py, index, key)? {
        index[group_index].1.push(row_index);
    } else {
        index.push((key.clone_ref(py), vec![row_index]));
    }
    Ok(())
}

fn remove_join_row(index: &mut Vec<(Py<PyAny>, Vec<usize>)>, row_index: usize) {
    let mut group_index = 0;
    while group_index < index.len() {
        index[group_index].1.retain(|index| *index != row_index);
        if index[group_index].1.is_empty() {
            index.remove(group_index);
        } else {
            group_index += 1;
        }
    }
}

fn find_join_indices<'a>(
    py: Python<'_>,
    index: &'a [(Py<PyAny>, Vec<usize>)],
    key: &Py<PyAny>,
) -> PyResult<Option<&'a [usize]>> {
    let Some(group_index) = find_join_group_index(py, index, key)? else {
        return Ok(None);
    };
    Ok(Some(index[group_index].1.as_slice()))
}

fn find_join_group_index(
    py: Python<'_>,
    index: &[(Py<PyAny>, Vec<usize>)],
    key: &Py<PyAny>,
) -> PyResult<Option<usize>> {
    let key = key.bind(py);
    for (group_index, (existing, _)) in index.iter().enumerate() {
        if existing.bind(py).eq(key)? {
            return Ok(Some(group_index));
        }
    }
    Ok(None)
}

fn equal_join_key_for_row(
    py: Python<'_>,
    row: &Py<PyAny>,
    joiners: &[EqualJoiner],
    side: u8,
) -> PyResult<Py<PyAny>> {
    let row = row.bind(py);
    joined_key(
        py,
        joiners.iter().map(|joiner| {
            if side == 0 {
                joiner.left_key.bind(py).call1((row,))
            } else {
                joiner.right_key.bind(py).call1((row,))
            }
        }),
    )
}

fn passes_joiners(
    joiners: Option<&Bound<'_, PyList>>,
    left: &Bound<'_, PyAny>,
    right: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let Some(joiners) = joiners else {
        return Ok(true);
    };
    for joiner in joiners.iter() {
        if let Ok(dict) = joiner.cast::<PyDict>() {
            let kind = dict
                .get_item("type")?
                .ok_or_else(|| py_err("joiner dictionary missing `type`"))?
                .extract::<String>()?;
            if kind == "equal" {
                let left_key = dict
                    .get_item("left_key")?
                    .ok_or_else(|| py_err("equal joiner missing `left_key`"))?
                    .call1((left,))?;
                let right_key = dict
                    .get_item("right_key")?
                    .ok_or_else(|| py_err("equal joiner missing `right_key`"))?
                    .call1((right,))?;
                if !left_key.eq(right_key)? {
                    return Ok(false);
                }
                continue;
            }
        }
        if !joiner.call1((left, right))?.extract::<bool>()? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn passes_binary_filters(
    filters: Option<&Bound<'_, PyList>>,
    left: &Bound<'_, PyAny>,
    right: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let Some(filters) = filters else {
        return Ok(true);
    };
    for filter in filters.iter() {
        if !filter.call1((left, right))?.extract::<bool>()? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn passes_group_filters(
    py: Python<'_>,
    filters: &Bound<'_, PyList>,
    key: &Bound<'_, PyAny>,
    aggregate: &GroupAggregate,
) -> PyResult<bool> {
    for filter in filters.iter() {
        let passes = match aggregate {
            GroupAggregate::Count(count) => filter.call1((key, *count))?,
            GroupAggregate::IndexedPresence(presence) => {
                let presence = Py::new(py, presence.clone())?;
                filter.call1((key, presence.bind(py)))?
            }
        };
        if !passes.extract::<bool>()? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn match_score_unary(
    fixed: DynamicScore,
    callback: Option<&Bound<'_, PyAny>>,
    entity: &Bound<'_, PyAny>,
) -> PyResult<DynamicScore> {
    match callback {
        Some(callback) => dynamic_score_from_native(&callback.call1((entity,))?),
        None => Ok(fixed),
    }
}

fn match_score_element(
    fixed: DynamicScore,
    callback: Option<&Bound<'_, PyAny>>,
    element: usize,
) -> PyResult<DynamicScore> {
    match callback {
        Some(callback) => dynamic_score_from_native(&callback.call1((element,))?),
        None => Ok(fixed),
    }
}

fn match_score_binary(
    fixed: DynamicScore,
    callback: Option<&Bound<'_, PyAny>>,
    left: &Bound<'_, PyAny>,
    right: &Bound<'_, PyAny>,
) -> PyResult<DynamicScore> {
    match callback {
        Some(callback) => dynamic_score_from_native(&callback.call1((left, right))?),
        None => Ok(fixed),
    }
}

fn match_score_group(
    py: Python<'_>,
    fixed: DynamicScore,
    callback: Option<&Bound<'_, PyAny>>,
    key: &Bound<'_, PyAny>,
    aggregate: &GroupAggregate,
) -> PyResult<DynamicScore> {
    match callback {
        Some(callback) => match aggregate {
            GroupAggregate::Count(count) => {
                dynamic_score_from_native(&callback.call1((key, *count))?)
            }
            GroupAggregate::IndexedPresence(presence) => {
                let presence = Py::new(py, presence.clone())?;
                dynamic_score_from_native(&callback.call1((key, presence.bind(py)))?)
            }
        },
        None => Ok(fixed),
    }
}

fn match_score_balance(
    fixed: DynamicScore,
    callback: Option<&Bound<'_, PyAny>>,
) -> PyResult<DynamicScore> {
    match callback {
        Some(callback) => dynamic_score_from_native(&callback.call0()?),
        None => Ok(fixed),
    }
}

fn apply_impact(total: DynamicScore, impact: &str, score: DynamicScore) -> DynamicScore {
    if impact == "reward" {
        total + score
    } else {
        total - score
    }
}

fn apply_delta_impact(
    total: DynamicScore,
    mode: DeltaMode,
    impact: &str,
    score: DynamicScore,
) -> DynamicScore {
    let contribution = apply_impact(DynamicScore::zero(), impact, score);
    match mode {
        DeltaMode::Insert => total + contribution,
        DeltaMode::Retract => total - contribution,
    }
}

fn required_plan_string(plan: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    plan.get_item(key)?
        .ok_or_else(|| py_err(format!("constraint missing {key}")))?
        .extract::<String>()
}

fn state_row_to_python_without_variables(
    py: Python<'_>,
    row: &crate::state::entity_table::DynamicEntityRow,
) -> PyResult<Py<PyAny>> {
    let kwargs = PyDict::new(py);
    for (name, value) in &row.fields {
        let value = value.to_python(py)?;
        kwargs.set_item(name, value.bind(py))?;
    }
    let types = py.import("types")?;
    let namespace = types.getattr("SimpleNamespace")?;
    Ok(namespace.call((), Some(&kwargs))?.unbind())
}
