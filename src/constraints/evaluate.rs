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
