use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use solverforge_solver::stats::{CandidateTraceCoordinate, CandidateTraceIdentity};
use solverforge_solver::{
    RuntimeCandidateMetric, RuntimeCandidateMetricBinding, RuntimeCandidateMetricRegistry,
};

use crate::error::{panic_with_py_err, py_err};
use crate::schema::DynamicSchema;
use crate::state::PyDynamicSolution;

pub(crate) fn candidate_metrics(
    schema: &DynamicSchema,
) -> Result<RuntimeCandidateMetricRegistry<PyDynamicSolution>, String> {
    Python::attach(
        |py| -> PyResult<RuntimeCandidateMetricRegistry<PyDynamicSolution>> {
            let metrics = schema.candidate_metrics.bind(py).cast::<PyList>()?;
            let entity_classes = Arc::<[Arc<str>]>::from(
                schema
                    .entities
                    .iter()
                    .map(|entity| Arc::<str>::from(entity.type_name.as_str()))
                    .collect::<Vec<_>>(),
            );
            let mut bindings = Vec::with_capacity(metrics.len());
            for metric_any in metrics.iter() {
                let metric = metric_any.cast::<PyDict>()?;
                let name = metric
                    .get_item("name")?
                    .ok_or_else(|| py_err("candidate metric is missing `name`"))?
                    .extract::<String>()?;
                let callback = metric
                    .get_item("callback")?
                    .ok_or_else(|| py_err("candidate metric is missing `callback`"))?
                    .unbind();
                bindings.push(
                    RuntimeCandidateMetricBinding::new(
                        name,
                        Arc::new(PyCandidateMetric {
                            callback,
                            entity_classes: Arc::clone(&entity_classes),
                        }),
                    )
                    .map_err(py_err)?,
                );
            }
            RuntimeCandidateMetricRegistry::new(bindings).map_err(py_err)
        },
    )
    .map_err(|error| error.to_string())
}

struct PyCandidateMetric {
    callback: Py<PyAny>,
    entity_classes: Arc<[Arc<str>]>,
}

impl RuntimeCandidateMetric<PyDynamicSolution> for PyCandidateMetric {
    fn measure(&self, solution: &PyDynamicSolution, candidate: &CandidateTraceIdentity) -> f64 {
        Python::attach(|py| -> PyResult<f64> {
            let solution = solution.to_python_callback_view(py)?;
            let candidate = candidate_dict(py, candidate, &self.entity_classes)?;
            self.callback
                .bind(py)
                .call1((solution, candidate))?
                .extract::<f64>()
        })
        .unwrap_or_else(panic_with_py_err)
    }
}

fn candidate_dict<'py>(
    py: Python<'py>,
    candidate: &CandidateTraceIdentity,
    entity_classes: &[Arc<str>],
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    match candidate {
        CandidateTraceIdentity::Operation(operation) => {
            let entity_class = entity_classes
                .get(operation.descriptor_index)
                .ok_or_else(|| {
                    py_err(format!(
                        "candidate identity refers to unknown descriptor {}",
                        operation.descriptor_index
                    ))
                })?;
            dict.set_item("type", "operation")?;
            dict.set_item("operation", &operation.operation)?;
            dict.set_item("descriptor_index", operation.descriptor_index)?;
            dict.set_item("entity_class", entity_class.as_ref())?;
            dict.set_item("variable_name", operation.variable_name.as_deref())?;
            let coordinates = PyList::empty(py);
            for coordinate in &operation.components {
                match coordinate {
                    CandidateTraceCoordinate::Unsigned(value) => coordinates.append(*value)?,
                    CandidateTraceCoordinate::Absent => coordinates.append(py.None())?,
                    CandidateTraceCoordinate::Text(value) => coordinates.append(value)?,
                    CandidateTraceCoordinate::Bytes(value) => {
                        coordinates.append(PyBytes::new(py, value))?
                    }
                }
            }
            dict.set_item("coordinates", coordinates)?;
        }
        CandidateTraceIdentity::Composite(composite) => {
            dict.set_item("type", "composite")?;
            dict.set_item("operation", &composite.operation)?;
            let children = PyList::empty(py);
            for child in &composite.children {
                children.append(candidate_dict(py, child, entity_classes)?)?;
            }
            dict.set_item("children", children)?;
        }
    }
    Ok(dict)
}
