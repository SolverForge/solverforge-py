use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use solverforge_solver::builder::context::{
    ProviderResolutionError, RawProviderCandidate, RawProviderEdit,
    RuntimeConflictRepairProviderBinding, RuntimeHostCompoundProvider,
    RuntimeHostProviderErrorBoundary, RuntimeProviderLimits, RuntimeProviderRegistry,
    RuntimeScalarGroupProviderBinding,
};

use crate::error::{panic_with_py_err, py_err};
use crate::schema::DynamicSchema;
use crate::state::PyDynamicSolution;

pub(crate) fn provider_registry(
    schema: &DynamicSchema,
) -> Result<RuntimeProviderRegistry<PyDynamicSolution>, String> {
    Python::attach(
        |py| -> PyResult<RuntimeProviderRegistry<PyDynamicSolution>> {
            let groups = schema.scalar_groups.bind(py).cast::<PyList>()?;
            let mut group_bindings = Vec::new();
            for (declared_index, group_any) in groups.iter().enumerate() {
                let group = group_any.cast::<PyDict>()?;
                let kind =
                    optional_dict_str(group, "kind")?.unwrap_or_else(|| "callback".to_string());
                if kind == "assignment" {
                    continue;
                }
                if kind != "callback" {
                    return Err(py_err(format!(
                        "scalar group at index {declared_index} has unsupported kind `{kind}`"
                    )));
                }
                let group_name = required_dict_str(group, "name")?;
                let callback = group
                    .get_item("callback")?
                    .ok_or_else(|| py_err("scalar group missing `callback`"))?
                    .unbind();
                group_bindings.push(RuntimeScalarGroupProviderBinding {
                    declared_index,
                    group_name: Arc::from(group_name),
                    callback: Arc::new(PyCompoundProvider {
                        callback,
                        default_reason: "dynamic_grouped_scalar",
                    }),
                });
            }

            let repairs = schema.conflict_repairs.bind(py).cast::<PyList>()?;
            let mut repair_bindings = Vec::new();
            for (declared_index, repair_any) in repairs.iter().enumerate() {
                let repair = repair_any.cast::<PyDict>()?;
                let constraints = string_list_from_dict(repair, "constraints")?
                    .into_iter()
                    .map(Arc::<str>::from)
                    .collect::<Vec<_>>();
                let callback = repair
                    .get_item("callback")?
                    .ok_or_else(|| py_err("conflict repair missing `callback`"))?
                    .unbind();
                repair_bindings.push(RuntimeConflictRepairProviderBinding {
                    declared_index,
                    declared_constraints: Arc::from(constraints),
                    callback: Arc::new(PyCompoundProvider {
                        callback,
                        default_reason: "dynamic_conflict_repair",
                    }),
                });
            }

            RuntimeProviderRegistry::new(
                group_bindings,
                repair_bindings,
                Arc::new(PyProviderErrorBoundary),
            )
            .map_err(py_err)
        },
    )
    .map_err(|error| error.to_string())
}

struct PyCompoundProvider {
    callback: Py<PyAny>,
    default_reason: &'static str,
}

impl RuntimeHostCompoundProvider<PyDynamicSolution> for PyCompoundProvider {
    fn pull(
        &self,
        solution: &PyDynamicSolution,
        limits: RuntimeProviderLimits,
    ) -> Vec<RawProviderCandidate> {
        Python::attach(|py| -> PyResult<Vec<RawProviderCandidate>> {
            let limits = limits_dict(py, limits)?;
            let callback_view = solution.to_python_callback_view(py)?;
            let result = self.callback.bind(py).call1((callback_view, limits))?;
            parse_candidates(&result, self.default_reason)
        })
        .unwrap_or_else(panic_with_py_err)
    }
}

struct PyProviderErrorBoundary;

impl RuntimeHostProviderErrorBoundary for PyProviderErrorBoundary {
    fn raise(&self, error: ProviderResolutionError) -> ! {
        panic_with_py_err(py_err(error.to_string()))
    }
}

fn limits_dict(py: Python<'_>, limits: RuntimeProviderLimits) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    match limits {
        RuntimeProviderLimits::Group {
            value_candidate_limit,
            max_moves_per_step,
        } => {
            set_optional_usize(&dict, "value_candidate_limit", value_candidate_limit)?;
            set_optional_usize(&dict, "max_moves_per_step", max_moves_per_step)?;
        }
        RuntimeProviderLimits::Repair {
            constraints,
            max_matches_per_step,
            max_repairs_per_match,
            max_moves_per_step,
            include_soft_matches,
        } => {
            dict.set_item(
                "constraints",
                constraints
                    .iter()
                    .map(|constraint| constraint.to_string())
                    .collect::<Vec<_>>(),
            )?;
            dict.set_item("max_matches_per_step", max_matches_per_step)?;
            dict.set_item("max_repairs_per_match", max_repairs_per_match)?;
            dict.set_item("max_moves_per_step", max_moves_per_step)?;
            dict.set_item("include_soft_matches", include_soft_matches)?;
        }
    }
    Ok(dict)
}

fn set_optional_usize(dict: &Bound<'_, PyDict>, key: &str, value: Option<usize>) -> PyResult<()> {
    match value {
        Some(value) => dict.set_item(key, value),
        None => dict.set_item(key, dict.py().None()),
    }
}

fn parse_candidates(
    result: &Bound<'_, PyAny>,
    default_reason: &'static str,
) -> PyResult<Vec<RawProviderCandidate>> {
    if result.is_none() {
        return Ok(Vec::new());
    }
    if result.cast::<PyDict>().is_ok() {
        return parse_candidate(result, default_reason).map(|candidate| vec![candidate]);
    }
    if let Ok(items) = result.cast::<PyList>() {
        return items
            .iter()
            .filter(|item| !item.is_none())
            .map(|item| parse_candidate(&item, default_reason))
            .collect();
    }
    if let Ok(items) = result.cast::<PyTuple>() {
        return items
            .iter()
            .filter(|item| !item.is_none())
            .map(|item| parse_candidate(&item, default_reason))
            .collect();
    }
    Err(py_err(format!(
        "dynamic compound callback returned unsupported candidate container {result:?}"
    )))
}

fn parse_candidate(
    candidate_any: &Bound<'_, PyAny>,
    default_reason: &'static str,
) -> PyResult<RawProviderCandidate> {
    let (reason, edits_any) = if let Ok(candidate) = candidate_any.cast::<PyDict>() {
        let reason =
            optional_dict_str(candidate, "reason")?.unwrap_or_else(|| default_reason.to_string());
        let edits = candidate
            .get_item("edits")?
            .ok_or_else(|| py_err("dynamic compound candidate missing `edits`"))?;
        (reason, edits)
    } else {
        (default_reason.to_string(), candidate_any.clone())
    };
    Ok(RawProviderCandidate {
        reason: Arc::from(reason),
        edits: parse_edits(&edits_any)?,
    })
}

fn parse_edits(edits_any: &Bound<'_, PyAny>) -> PyResult<Vec<RawProviderEdit>> {
    if let Ok(edits) = edits_any.cast::<PyList>() {
        return edits.iter().map(|edit| parse_edit(&edit)).collect();
    }
    if let Ok(edits) = edits_any.cast::<PyTuple>() {
        return edits.iter().map(|edit| parse_edit(&edit)).collect();
    }
    Err(py_err(format!(
        "dynamic compound candidate edits must be a list or tuple, got {edits_any:?}"
    )))
}

fn parse_edit(edit_any: &Bound<'_, PyAny>) -> PyResult<RawProviderEdit> {
    let edit = edit_any.cast::<PyDict>()?;
    let entity_object = edit.get_item("entity")?;
    let entity_class = optional_dict_str(edit, "entity_class")?
        .or(optional_dict_str(edit, "entity_type")?)
        .or_else(|| {
            entity_object
                .as_ref()
                .and_then(|entity| entity.getattr("_solverforge_entity_class").ok())
                .and_then(|value| value.extract::<String>().ok())
        });
    let variable_name = required_dict_str(edit, "variable_name")?;
    let entity_index = optional_dict_usize(edit, "entity_index")?
        .or_else(|| {
            entity_object
                .as_ref()
                .and_then(|entity| entity.getattr("_solverforge_entity_index").ok())
                .and_then(|value| value.extract::<usize>().ok())
        })
        .ok_or_else(|| py_err("dynamic compound edit missing `entity_index` or `entity`"))?;
    let to_value_any = edit
        .get_item("to_value")?
        .or(edit.get_item("value")?)
        .ok_or_else(|| py_err("dynamic compound edit missing `to_value`"))?;
    let to_value = if to_value_any.is_none() {
        None
    } else {
        Some(to_value_any.extract::<usize>()?)
    };
    Ok(RawProviderEdit {
        entity_class: entity_class.map(Arc::from),
        variable_name: Arc::from(variable_name),
        entity_index,
        to_value,
    })
}

fn required_dict_str(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    dict.get_item(key)?
        .ok_or_else(|| py_err(format!("dynamic callback dict missing `{key}`")))?
        .extract::<String>()
}

fn optional_dict_str(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<String>> {
    dict.get_item(key)?
        .map(|value| {
            if value.is_none() {
                Ok(None)
            } else {
                value.extract::<String>().map(Some)
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn optional_dict_usize(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<usize>> {
    dict.get_item(key)?
        .map(|value| {
            if value.is_none() {
                Ok(None)
            } else {
                value.extract::<usize>().map(Some)
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn string_list_from_dict(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Vec<String>> {
    let values = dict
        .get_item(key)?
        .ok_or_else(|| py_err(format!("dynamic callback dict missing `{key}`")))?;
    values
        .cast::<PyList>()?
        .iter()
        .map(|item| item.extract::<String>())
        .collect()
}
