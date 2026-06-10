use std::collections::BTreeMap;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use solverforge_core::score::Score;

use crate::error::py_err;
use crate::score::{dynamic_score_from_native, DynamicScore};

pub fn evaluate_constraints(
    _py: Python<'_>,
    solution: &Bound<'_, PyAny>,
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
        let collection = find_collection(solution, &entity_type)?;
        let filters_any = plan
            .get_item("filters")?
            .ok_or_else(|| py_err("constraint missing filters"))?;
        let filters = filters_any.cast::<PyList>()?;
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
        if constraint_type.as_deref() == Some("list_unassigned_element") {
            let variable_name = plan
                .get_item("variable_name")?
                .ok_or_else(|| py_err("constraint missing variable_name"))?
                .extract::<String>()?;
            let element_collection = plan
                .get_item("element_collection")?
                .ok_or_else(|| py_err("constraint missing element_collection"))?
                .extract::<String>()?;
            total = evaluate_unassigned_elements(
                solution,
                collection,
                &element_collection,
                &variable_name,
                filters,
                impact.as_str(),
                weight,
                weight_callback.as_ref(),
                total,
            )?;
            continue;
        }
        for entity in collection.iter() {
            let mut matched = true;
            for filter in filters.iter() {
                let result = filter.call1((&entity,))?.extract::<bool>()?;
                if !result {
                    matched = false;
                    break;
                }
            }
            if matched {
                total = if impact == "reward" {
                    total + weight
                } else {
                    total - weight
                };
            }
        }
    }
    Ok(total)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_unassigned_elements(
    solution: &Bound<'_, PyAny>,
    owners: Bound<'_, PyList>,
    element_collection: &str,
    variable_name: &str,
    filters: &Bound<'_, PyList>,
    impact: &str,
    weight: DynamicScore,
    weight_callback: Option<&Bound<'_, PyAny>>,
    mut total: DynamicScore,
) -> PyResult<DynamicScore> {
    let elements = solution
        .getattr(element_collection)?
        .cast::<PyList>()?
        .clone();
    let mut counts = BTreeMap::<usize, i64>::new();
    for owner in owners.iter() {
        let values = owner.getattr(variable_name)?.cast::<PyList>()?.clone();
        for value in values.iter() {
            *counts.entry(value.extract::<usize>()?).or_insert(0) += 1;
        }
    }
    for element in elements.iter() {
        let element_index = element.extract::<usize>()?;
        if counts.get(&element_index).copied().unwrap_or(0) > 0 {
            continue;
        }
        let mut matched = true;
        for filter in filters.iter() {
            if !filter.call1((&element,))?.extract::<bool>()? {
                matched = false;
                break;
            }
        }
        if matched {
            let score = match weight_callback {
                Some(callback) => dynamic_score_from_native(&callback.call1((&element,))?),
                None => Ok(weight),
            }?;
            total = if impact == "reward" {
                total + score
            } else {
                total - score
            };
        }
    }
    Ok(total)
}

fn find_collection<'py>(
    solution: &Bound<'py, PyAny>,
    entity_type: &str,
) -> PyResult<Bound<'py, PyList>> {
    for item in solution.dir()?.iter() {
        let name = item.extract::<String>()?;
        if name.starts_with('_') {
            continue;
        }
        let Ok(value) = solution.getattr(name.as_str()) else {
            continue;
        };
        let Ok(list) = value.cast::<PyList>() else {
            continue;
        };
        if let Some(first) = list.iter().next() {
            let class_name = first.get_type().name()?.to_string();
            if class_name == entity_type {
                return Ok(list.clone());
            }
        }
    }
    Err(py_err(format!(
        "could not find solution collection for entity type `{entity_type}`"
    )))
}
