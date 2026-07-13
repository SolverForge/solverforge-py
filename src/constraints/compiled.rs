use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use solverforge_core::score::Score;

use crate::error::py_err;
use crate::schema::DynamicSchema;
use crate::score::{dynamic_score_from_native, DynamicScore};
use crate::state::{entity_table::DynamicState, PyDynamicSolution};

pub struct CompiledConstraintSet {
    constraints: Py<PyAny>,
    plans: Vec<CompiledConstraintPlan>,
}

impl CompiledConstraintSet {
    pub fn python(constraints: Py<PyAny>) -> Self {
        Self {
            constraints,
            plans: Vec::new(),
        }
    }

    pub fn from_solution(py: Python<'_>, solution: &PyDynamicSolution) -> PyResult<Self> {
        let schema = solution.schema();
        let constraints = schema.constraints.clone_ref(py);
        let plans = compile_plans(schema, &solution.state, constraints.bind(py))?;
        Ok(Self { constraints, plans })
    }

    pub fn python_plans(&self) -> &Py<PyAny> {
        &self.constraints
    }

    pub fn plans(&self) -> &[CompiledConstraintPlan] {
        &self.plans
    }

    pub fn constraint_count(&self, py: Python<'_>) -> usize {
        self.constraints
            .bind(py)
            .cast::<PyList>()
            .map(|items| items.len())
            .unwrap_or(0)
    }
}

pub enum CompiledConstraintPlan {
    Unary(UnaryPlan),
    AttributeJoin(AttributeJoinPlan),
    ListCoverage(ListCoveragePlan),
    Python(PythonPlan),
}

impl CompiledConstraintPlan {
    pub fn python_plan(&self) -> &Py<PyAny> {
        match self {
            Self::Unary(plan) => &plan.python_plan,
            Self::AttributeJoin(plan) => &plan.python_plan,
            Self::ListCoverage(plan) => &plan.python_plan,
            Self::Python(plan) => &plan.python_plan,
        }
    }
}

pub struct UnaryPlan {
    pub python_plan: Py<PyAny>,
    pub entity_index: usize,
    pub impact: Impact,
    pub weight: DynamicScore,
}

pub struct AttributeJoinPlan {
    pub python_plan: Py<PyAny>,
    pub left_entity_index: usize,
    pub right_entity_index: usize,
    pub joiners: Vec<AttributeJoiner>,
    pub impact: Impact,
    pub weight: DynamicScore,
}

pub struct ListCoveragePlan {
    pub python_plan: Py<PyAny>,
    pub entity_index: usize,
    pub variable_index: usize,
    pub impact: Impact,
    pub weight: DynamicScore,
}

pub struct PythonPlan {
    pub python_plan: Py<PyAny>,
}

#[derive(Clone, Copy)]
pub enum Impact {
    Penalty,
    Reward,
}

impl Impact {
    pub fn apply(self, score: DynamicScore) -> DynamicScore {
        match self {
            Self::Penalty => DynamicScore::zero() - score,
            Self::Reward => score,
        }
    }
}

pub struct AttributeJoiner {
    pub left: AttributeSource,
    pub right: AttributeSource,
}

pub enum AttributeSource {
    Scalar(usize),
    Field(String),
    Unsupported,
}

fn compile_plans(
    schema: &DynamicSchema,
    state: &DynamicState,
    constraints: &Bound<'_, PyAny>,
) -> PyResult<Vec<CompiledConstraintPlan>> {
    let constraints = constraints.cast::<PyList>()?;
    let mut plans = Vec::with_capacity(constraints.len());
    for plan_any in constraints.iter() {
        let plan = plan_any.cast::<PyDict>()?;
        let python_plan = plan_any.clone().unbind();
        plans.push(compile_plan(schema, state, plan, python_plan)?);
    }
    Ok(plans)
}

fn compile_plan(
    schema: &DynamicSchema,
    state: &DynamicState,
    plan: &Bound<'_, PyDict>,
    python_plan: Py<PyAny>,
) -> PyResult<CompiledConstraintPlan> {
    if plan.get_item("weight_callback")?.is_some() {
        return Ok(CompiledConstraintPlan::Python(PythonPlan { python_plan }));
    }
    let impact = parse_impact(plan)?;
    let weight_any = plan
        .get_item("weight")?
        .ok_or_else(|| py_err("constraint missing weight"))?;
    let weight = dynamic_score_from_native(&weight_any)?;
    let entity_type = required_plan_string(plan, "entity_type")?;
    let Some(entity_index) = schema.entity_index_by_type(entity_type.as_str()) else {
        return Ok(CompiledConstraintPlan::Python(PythonPlan { python_plan }));
    };
    let constraint_type = optional_plan_string(plan, "constraint_type")?;
    if constraint_type.as_deref() == Some("list_unassigned_element") {
        if !list_is_empty(plan, "filters")? {
            return Ok(CompiledConstraintPlan::Python(PythonPlan { python_plan }));
        }
        let variable_name = required_plan_string(plan, "variable_name")?;
        let Some(variable_index) = schema.variable_index(
            entity_index,
            variable_name.as_str(),
            Some("planning_list_variable"),
        ) else {
            return Ok(CompiledConstraintPlan::Python(PythonPlan { python_plan }));
        };
        return Ok(CompiledConstraintPlan::ListCoverage(ListCoveragePlan {
            python_plan,
            entity_index,
            variable_index,
            impact,
            weight,
        }));
    }
    if constraint_type.is_some()
        || plan.get_item("balance_key")?.is_some()
        || plan.get_item("group_key")?.is_some()
    {
        return Ok(CompiledConstraintPlan::Python(PythonPlan { python_plan }));
    }

    let arity = plan
        .get_item("arity")?
        .map(|value| value.extract::<usize>())
        .transpose()?
        .unwrap_or(1);
    match arity {
        1 if list_is_empty(plan, "filters")? => Ok(CompiledConstraintPlan::Unary(UnaryPlan {
            python_plan,
            entity_index,
            impact,
            weight,
        })),
        2 if binary_filters_are_empty(plan)? => {
            let Some(joiners_any) = plan.get_item("joiners")? else {
                return Ok(CompiledConstraintPlan::Python(PythonPlan { python_plan }));
            };
            let Some(right_entity_index) = right_entity_index(schema, plan)? else {
                return Ok(CompiledConstraintPlan::Python(PythonPlan { python_plan }));
            };
            let Some(joiners) = compile_attribute_joiners(
                schema,
                state,
                entity_index,
                right_entity_index,
                joiners_any.cast::<PyList>()?,
            )?
            else {
                return Ok(CompiledConstraintPlan::Python(PythonPlan { python_plan }));
            };
            Ok(CompiledConstraintPlan::AttributeJoin(AttributeJoinPlan {
                python_plan,
                left_entity_index: entity_index,
                right_entity_index,
                joiners,
                impact,
                weight,
            }))
        }
        _ => Ok(CompiledConstraintPlan::Python(PythonPlan { python_plan })),
    }
}

fn parse_impact(plan: &Bound<'_, PyDict>) -> PyResult<Impact> {
    match required_plan_string(plan, "impact")?.as_str() {
        "reward" => Ok(Impact::Reward),
        _ => Ok(Impact::Penalty),
    }
}

fn right_entity_index(schema: &DynamicSchema, plan: &Bound<'_, PyDict>) -> PyResult<Option<usize>> {
    let right_entity_type = required_plan_string(plan, "right_entity_type")?;
    Ok(schema.entity_index_by_type(right_entity_type.as_str()))
}

fn binary_filters_are_empty(plan: &Bound<'_, PyDict>) -> PyResult<bool> {
    Ok(list_is_empty(plan, "filters")?
        && list_is_empty(plan, "left_filters")?
        && list_is_empty(plan, "right_filters")?)
}

fn compile_attribute_joiners(
    schema: &DynamicSchema,
    state: &DynamicState,
    left_entity_index: usize,
    right_entity_index: usize,
    joiners: &Bound<'_, PyList>,
) -> PyResult<Option<Vec<AttributeJoiner>>> {
    if joiners.is_empty() {
        return Ok(None);
    }
    let mut compiled = Vec::with_capacity(joiners.len());
    for joiner_any in joiners.iter() {
        let Ok(joiner) = joiner_any.cast::<PyDict>() else {
            return Ok(None);
        };
        if required_plan_string(joiner, "type")? != "equal_attr" {
            return Ok(None);
        }
        let left_attr = required_plan_string(joiner, "left_attr")?;
        let right_attr = required_plan_string(joiner, "right_attr")?;
        let left = compile_attribute_source(schema, state, left_entity_index, left_attr);
        let right = compile_attribute_source(schema, state, right_entity_index, right_attr);
        if matches!(left, AttributeSource::Unsupported)
            || matches!(right, AttributeSource::Unsupported)
        {
            return Ok(None);
        }
        compiled.push(AttributeJoiner { left, right });
    }
    Ok(Some(compiled))
}

fn compile_attribute_source(
    schema: &DynamicSchema,
    state: &DynamicState,
    entity_index: usize,
    attr: String,
) -> AttributeSource {
    let Some(entity) = schema.entities.get(entity_index) else {
        return AttributeSource::Unsupported;
    };
    if let Some((variable_index, variable)) = entity
        .variables
        .iter()
        .enumerate()
        .find(|(_, variable)| variable.name == attr)
    {
        if variable.kind == "planning_variable" {
            return AttributeSource::Scalar(variable_index);
        }
        return AttributeSource::Unsupported;
    }
    let Some(rows) = state.entities.get(entity_index) else {
        return AttributeSource::Unsupported;
    };
    if !rows.is_empty()
        && rows.iter().all(|row| {
            row.instance_fields.contains(&attr)
                && row.native_equality_fields.contains(&attr)
                && !row.shadow_fields.contains(&attr)
        })
    {
        return AttributeSource::Field(attr);
    }
    AttributeSource::Unsupported
}

fn required_plan_string(plan: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    plan.get_item(key)?
        .ok_or_else(|| py_err(format!("constraint missing {key}")))?
        .extract()
}

fn optional_plan_string(plan: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<String>> {
    plan.get_item(key)?.map(|value| value.extract()).transpose()
}

fn list_is_empty(plan: &Bound<'_, PyDict>, key: &str) -> PyResult<bool> {
    let Some(value) = plan.get_item(key)? else {
        return Ok(true);
    };
    Ok(value.cast::<PyList>()?.len() == 0)
}
