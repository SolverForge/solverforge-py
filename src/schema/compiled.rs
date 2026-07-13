use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::error::py_err;
use crate::schema::parse::parse_schema;
use crate::schema::runtime_plan::CompiledRuntimePlan;
use crate::schema::validate::validate_dynamic_schema;
use std::sync::Arc;

#[pyclass(name = "CompiledSchema")]
pub struct CompiledSchema {
    plan: Arc<CompiledRuntimePlan>,
}

impl CompiledSchema {
    pub fn plan(&self) -> Arc<CompiledRuntimePlan> {
        Arc::clone(&self.plan)
    }
}

#[pymethods]
impl CompiledSchema {
    #[getter]
    fn solution_type(&self) -> &str {
        self.plan.schema().solution_type.as_str()
    }

    #[getter]
    fn score_family(&self) -> &str {
        self.plan.schema().score_family.as_str()
    }
}

#[pyfunction]
pub fn compile_schema(schema: &Bound<'_, PyDict>) -> PyResult<CompiledSchema> {
    let parsed = Arc::new(parse_schema(schema)?);
    validate_dynamic_schema(&parsed)?;
    let plan = CompiledRuntimePlan::from_schema(parsed)
        .map_err(|err| py_err(format!("failed to resolve dynamic runtime model: {err}")))?;
    Ok(CompiledSchema {
        plan: Arc::new(plan),
    })
}
