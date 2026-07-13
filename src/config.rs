use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use serde_json::{Map, Number, Value};
use solverforge_config::SolverConfig;

use crate::error::py_err;

pub fn config_from_python(config: Option<&Bound<'_, PyDict>>) -> PyResult<SolverConfig> {
    let Some(config) = config else {
        return Ok(SolverConfig::default());
    };
    let config = py_to_json(&config.clone().into_any())?;
    reject_reserved_retained_diagnostic_fields(&config)?;
    serde_json::from_value(config)
        .map_err(|error| py_err(format!("invalid solver config: {error}")))
}

fn reject_reserved_retained_diagnostic_fields(value: &Value) -> PyResult<()> {
    match value {
        Value::Object(object) => {
            if object.contains_key("qualified_candidate_trace_provenance") {
                return Err(py_err(concat!(
                    "`qualified_candidate_trace_provenance` is a retained-diagnostics-only ",
                    "argument to SolverManager.solve(), not a SolverConfig field"
                )));
            }
            for child in object.values() {
                reject_reserved_retained_diagnostic_fields(child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                reject_reserved_retained_diagnostic_fields(child)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn py_to_json(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    if value.is_none() {
        return Ok(Value::Null);
    }
    if let Ok(value) = value.extract::<bool>() {
        return Ok(Value::Bool(value));
    }
    if let Ok(value) = value.extract::<i64>() {
        return Ok(Value::Number(Number::from(value)));
    }
    if let Ok(value) = value.extract::<f64>() {
        return Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| py_err(format!("solver config contains non-finite float `{value}`")));
    }
    if let Ok(value) = value.extract::<String>() {
        return Ok(Value::String(value));
    }
    if let Ok(dict) = value.cast::<PyDict>() {
        let mut object = Map::new();
        for (key, item) in dict.iter() {
            object.insert(key.extract::<String>()?, py_to_json(&item)?);
        }
        return Ok(Value::Object(object));
    }
    if let Ok(list) = value.cast::<PyList>() {
        return list
            .iter()
            .map(|item| py_to_json(&item))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array);
    }
    if let Ok(tuple) = value.cast::<PyTuple>() {
        return tuple
            .iter()
            .map(|item| py_to_json(&item))
            .collect::<PyResult<Vec<_>>>()
            .map(Value::Array);
    }
    Err(py_err(format!(
        "solver config contains unsupported value {value:?}"
    )))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::reject_reserved_retained_diagnostic_fields;

    #[test]
    fn qualified_trace_provenance_is_rejected_at_any_config_depth() {
        assert!(reject_reserved_retained_diagnostic_fields(&json!({
            "qualified_candidate_trace_provenance": {"producer": "wrong-place"}
        }))
        .is_err());
        assert!(reject_reserved_retained_diagnostic_fields(&json!({
            "phases": [{
                "type": "local_search",
                "qualified_candidate_trace_provenance": {"producer": "wrong-place"}
            }]
        }))
        .is_err());
    }

    #[test]
    fn ordinary_candidate_trace_config_remains_a_solver_config() {
        reject_reserved_retained_diagnostic_fields(&json!({
            "candidate_trace": {"max_entries": 128}
        }))
        .expect("ordinary trace settings are not provenance");
    }
}
