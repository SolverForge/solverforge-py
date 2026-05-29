use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use solverforge_core::score::Score;

use crate::error::py_err;

pub use solverforge_bridge::{
    scoped_dynamic_score_family, DynamicScore, DynamicScoreFamily as ScoreFamily,
};

pub fn dynamic_score_from_native(value: &Bound<'_, PyAny>) -> PyResult<DynamicScore> {
    if let Ok(number) = value.extract::<i64>() {
        return Ok(DynamicScore::soft(number));
    }
    if let Ok(dict) = value.cast::<PyDict>() {
        return dynamic_score_from_dict(dict);
    }
    if let Ok(levels) = value.cast::<PyList>() {
        return dynamic_score_from_sequence(levels.iter());
    }
    if let Ok(levels) = value.cast::<PyTuple>() {
        return dynamic_score_from_sequence(levels.iter());
    }
    if let Ok(method) = value.getattr("to_native") {
        let native = method.call0()?;
        if let Ok(dict) = native.cast::<PyDict>() {
            return dynamic_score_from_dict(dict);
        }
    }
    Err(py_err(format!("cannot convert {value:?} to DynamicScore")))
}

pub fn dynamic_score_from_dict(dict: &Bound<'_, PyDict>) -> PyResult<DynamicScore> {
    let family = dict
        .get_item("family")?
        .and_then(|value| value.extract::<String>().ok());
    let levels_any = dict
        .get_item("levels")?
        .ok_or_else(|| py_err("score dictionary is missing `levels`"))?;
    let levels = levels_any.cast::<PyList>()?;
    let numbers = levels
        .iter()
        .map(|item| item.extract::<i64>())
        .collect::<PyResult<Vec<_>>>()?;
    score_from_levels(family.as_deref(), numbers.as_slice(), None)
}

fn dynamic_score_from_sequence<'py>(
    items: impl Iterator<Item = Bound<'py, PyAny>>,
) -> PyResult<DynamicScore> {
    let numbers = items
        .map(|item| item.extract::<i64>())
        .collect::<PyResult<Vec<_>>>()?;
    score_from_levels(None, numbers.as_slice(), Some(DynamicScore::zero().family))
}

fn score_from_levels(
    family: Option<&str>,
    numbers: &[i64],
    preferred_family: Option<ScoreFamily>,
) -> PyResult<DynamicScore> {
    Ok(match (family, numbers) {
        (Some("soft"), [soft]) => DynamicScore::soft(*soft),
        (Some("hard_soft"), [hard, soft]) => DynamicScore::hard_soft(*hard, *soft),
        (Some("hard_soft_decimal"), [hard, soft]) => DynamicScore::hard_soft_decimal(*hard, *soft),
        (Some("hard_medium_soft"), [hard, medium, soft]) => {
            DynamicScore::hard_medium_soft(*hard, *medium, *soft)
        }
        (Some(family), _) => {
            return Err(py_err(format!(
                "score family `{family}` is incompatible with {} levels",
                numbers.len()
            )));
        }
        (None, [soft]) => DynamicScore::soft(*soft),
        (None, [hard, soft]) if preferred_family == Some(ScoreFamily::HardSoftDecimal) => {
            DynamicScore::hard_soft_decimal(*hard, *soft)
        }
        (None, [hard, soft]) => DynamicScore::hard_soft(*hard, *soft),
        (None, [hard, medium, soft]) => DynamicScore::hard_medium_soft(*hard, *medium, *soft),
        (None, other) => {
            return Err(py_err(format!(
                "unsupported dynamic score level count {}",
                other.len()
            )));
        }
    })
}

pub trait DynamicScorePythonExt {
    fn to_python(self, py: Python<'_>) -> PyResult<Py<PyAny>>;
    fn to_python_for_family(self, py: Python<'_>, family: &str) -> PyResult<Py<PyAny>>;
}

impl DynamicScorePythonExt for DynamicScore {
    fn to_python(self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.to_python_for_family(py, "hard_medium_soft")
    }

    fn to_python_for_family(self, py: Python<'_>, family: &str) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        match family {
            "soft" => {
                dict.set_item("family", "soft")?;
                dict.set_item("levels", [self.soft])?;
            }
            "hard_soft" => {
                dict.set_item("family", "hard_soft")?;
                dict.set_item("levels", [self.hard, self.soft])?;
            }
            "hard_soft_decimal" => {
                dict.set_item("family", "hard_soft_decimal")?;
                dict.set_item("levels", [self.hard, self.soft])?;
            }
            _ => {
                dict.set_item("family", "hard_medium_soft")?;
                dict.set_item("levels", [self.hard, self.medium, self.soft])?;
            }
        }
        Ok(dict.into_any().unbind())
    }
}

pub fn score_family_from_name(name: &str) -> PyResult<ScoreFamily> {
    match name {
        "soft" => Ok(ScoreFamily::Soft),
        "hard_soft" => Ok(ScoreFamily::HardSoft),
        "hard_soft_decimal" => Ok(ScoreFamily::HardSoftDecimal),
        "hard_medium_soft" => Ok(ScoreFamily::HardMediumSoft),
        _ => Err(py_err(format!("unsupported score family `{name}`"))),
    }
}
