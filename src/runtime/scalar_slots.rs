use std::sync::Arc;

use pyo3::prelude::*;
use solverforge_bridge::{
    DynamicScalarAccess, DynamicScalarVariableSlot, EntityClassId, VariableId,
};

use crate::error::panic_with_py_err;
use crate::intern::intern;
use crate::schema::types::{MetadataSourceSchema, VariableSchema};
use crate::schema::DynamicSchema;
use crate::state::PyDynamicSolution;
use crate::value::DynamicValue;

/// Builds one core dynamic-slot access object per Python planning variable.
///
/// The access object owns immutable schema provenance and is the only place
/// that translates Python row metadata/callbacks into core dynamic-neighborhood
/// behavior. Both wrapper-local selectors and canonical core selectors call the
/// same slot methods; there is no second nearby-candidate interpretation.
pub fn scalar_slots(
    schema: &Arc<DynamicSchema>,
) -> Vec<DynamicScalarVariableSlot<PyDynamicSolution>> {
    schema
        .entities
        .iter()
        .enumerate()
        .flat_map(|(entity_index, entity)| {
            entity
                .variables
                .iter()
                .enumerate()
                .filter(|(_, variable)| variable.kind == "planning_variable")
                .map(move |(variable_index, variable)| {
                    let entity_class = EntityClassId(entity_index);
                    let variable_id = VariableId(variable_index);
                    DynamicScalarVariableSlot::with_access(
                        entity_class,
                        variable_id,
                        intern(entity.type_name.clone()),
                        intern(variable.name.clone()),
                        variable.allows_unassigned,
                        Arc::new(PyDynamicScalarAccess {
                            schema: Arc::clone(schema),
                            entity: entity_class,
                            variable: variable_id,
                        }),
                    )
                })
        })
        .collect()
}

struct PyDynamicScalarAccess {
    schema: Arc<DynamicSchema>,
    entity: EntityClassId,
    variable: VariableId,
}

impl PyDynamicScalarAccess {
    fn variable_schema(&self) -> Option<&VariableSchema> {
        self.schema
            .entities
            .get(self.entity.0)?
            .variables
            .get(self.variable.0)
    }

    fn row<'a>(
        &self,
        solution: &'a PyDynamicSolution,
        row: usize,
    ) -> Option<&'a crate::state::entity_table::DynamicEntityRow> {
        solution.state.entities.get(self.entity.0)?.get(row)
    }

    fn visit_python_candidates(
        callback: &Py<PyAny>,
        solution: &PyDynamicSolution,
        entity: EntityClassId,
        row: usize,
        limit: usize,
        visit: &mut dyn FnMut(usize),
    ) -> bool {
        Python::attach(|py| -> PyResult<bool> {
            let entity = solution.entity_callback_view(py, entity.0, row)?;
            let result = callback.bind(py).call1((entity,))?;
            if result.is_none() {
                return Ok(false);
            }
            for candidate in result.try_iter()?.take(limit) {
                visit(candidate?.extract::<usize>()?);
            }
            Ok(true)
        })
        .unwrap_or_else(panic_with_py_err)
    }

    fn callback_value_distance(
        callback: &Py<PyAny>,
        solution: &PyDynamicSolution,
        entity: EntityClassId,
        row: usize,
        other: usize,
    ) -> Option<f64> {
        Python::attach(|py| -> PyResult<Option<f64>> {
            let entity = solution.entity_callback_view(py, entity.0, row)?;
            let result = callback.bind(py).call1((entity, other))?;
            if result.is_none() {
                Ok(None)
            } else {
                result.extract::<f64>().map(Some)
            }
        })
        .unwrap_or_else(panic_with_py_err)
    }

    fn callback_entity_distance(
        callback: &Py<PyAny>,
        solution: &PyDynamicSolution,
        entity: EntityClassId,
        left_row: usize,
        right_row: usize,
    ) -> Option<f64> {
        Python::attach(|py| -> PyResult<Option<f64>> {
            let left = solution.entity_callback_view(py, entity.0, left_row)?;
            let right = solution.entity_callback_view(py, entity.0, right_row)?;
            let result = callback.bind(py).call1((left, right))?;
            if result.is_none() {
                Ok(None)
            } else {
                result.extract::<f64>().map(Some)
            }
        })
        .unwrap_or_else(panic_with_py_err)
    }

    fn row_usize_candidates(
        &self,
        solution: &PyDynamicSolution,
        row: usize,
        field_name: &str,
        limit: usize,
        visit: &mut dyn FnMut(usize),
    ) -> bool {
        let Some(values) = self
            .row(solution, row)
            .and_then(|row| row.fields.get(field_name))
            .and_then(dynamic_value_usize_slice)
        else {
            return false;
        };
        for value in values.iter().take(limit) {
            visit(dynamic_value_usize(value).expect("row candidate values validated"));
        }
        true
    }

    fn row_distance(
        &self,
        solution: &PyDynamicSolution,
        row: usize,
        field_name: &str,
        other: usize,
    ) -> Option<f64> {
        self.row(solution, row)
            .and_then(|row| row.fields.get(field_name))
            .and_then(|value| list_number(value, other))
    }

    fn visit_nearby_candidates(
        &self,
        source: Option<&MetadataSourceSchema>,
        solution: &PyDynamicSolution,
        row: usize,
        limit: usize,
        visit: &mut dyn FnMut(usize),
    ) -> bool {
        match source {
            Some(MetadataSourceSchema::Row(field_name)) => {
                self.row_usize_candidates(solution, row, field_name, limit, visit)
            }
            Some(MetadataSourceSchema::Callback(callback)) => {
                Self::visit_python_candidates(callback, solution, self.entity, row, limit, visit)
            }
            None => false,
        }
    }
}

impl DynamicScalarAccess<PyDynamicSolution> for PyDynamicScalarAccess {
    fn entity_class(&self) -> EntityClassId {
        self.entity
    }

    fn variable(&self) -> VariableId {
        self.variable
    }

    fn entity_count(&self, solution: &PyDynamicSolution) -> usize {
        solution.entity_count(self.entity.0)
    }

    fn get(&self, solution: &PyDynamicSolution, row: usize) -> Option<usize> {
        self.row(solution, row)
            .and_then(|row| row.scalar_at(self.variable.0))
    }

    fn set(&self, solution: &mut PyDynamicSolution, row: usize, value: Option<usize>) {
        solution.set_scalar_value(self.entity.0, row, self.variable.0, value);
    }

    fn candidate_values<'a>(&self, solution: &'a PyDynamicSolution, row: usize) -> &'a [usize] {
        self.row(solution, row)
            .and_then(|row| row.candidates_at(self.variable.0))
            .unwrap_or(&[])
    }

    fn value_is_legal(&self, solution: &PyDynamicSolution, _row: usize, value: usize) -> bool {
        solution
            .state
            .scalar_value_range_at(self.entity.0, self.variable.0)
            .is_some_and(|values| values.contains(&value))
    }

    fn has_nearby_value_candidates(&self) -> bool {
        self.variable_schema()
            .is_some_and(|variable| variable.nearby_value_candidates.is_some())
    }

    fn visit_nearby_value_candidates(
        &self,
        solution: &PyDynamicSolution,
        row: usize,
        limit: usize,
        visit: &mut dyn FnMut(usize),
    ) -> bool {
        self.visit_nearby_candidates(
            self.variable_schema()
                .and_then(|variable| variable.nearby_value_candidates.as_ref()),
            solution,
            row,
            limit,
            visit,
        )
    }

    fn nearby_value_distance(
        &self,
        solution: &PyDynamicSolution,
        row: usize,
        candidate: usize,
    ) -> Option<f64> {
        match self
            .variable_schema()?
            .nearby_value_distance_meter
            .as_ref()?
        {
            MetadataSourceSchema::Row(field_name) => {
                self.row_distance(solution, row, field_name, candidate)
            }
            MetadataSourceSchema::Callback(callback) => {
                Self::callback_value_distance(callback, solution, self.entity, row, candidate)
            }
        }
    }

    fn has_nearby_entity_candidates(&self) -> bool {
        self.variable_schema()
            .is_some_and(|variable| variable.nearby_entity_candidates.is_some())
    }

    fn visit_nearby_entity_candidates(
        &self,
        solution: &PyDynamicSolution,
        left_row: usize,
        limit: usize,
        visit: &mut dyn FnMut(usize),
    ) -> bool {
        self.visit_nearby_candidates(
            self.variable_schema()
                .and_then(|variable| variable.nearby_entity_candidates.as_ref()),
            solution,
            left_row,
            limit,
            visit,
        )
    }

    fn nearby_entity_distance(
        &self,
        solution: &PyDynamicSolution,
        left_row: usize,
        right_row: usize,
    ) -> Option<f64> {
        match self
            .variable_schema()?
            .nearby_entity_distance_meter
            .as_ref()?
        {
            MetadataSourceSchema::Row(field_name) => {
                self.row_distance(solution, left_row, field_name, right_row)
            }
            MetadataSourceSchema::Callback(callback) => {
                Self::callback_entity_distance(callback, solution, self.entity, left_row, right_row)
            }
        }
    }
}

fn dynamic_value_usize_slice(value: &DynamicValue) -> Option<&[DynamicValue]> {
    let DynamicValue::List(values) = value else {
        return None;
    };
    values
        .iter()
        .all(|value| dynamic_value_usize(value).is_some())
        .then_some(values)
}

fn dynamic_value_usize(value: &DynamicValue) -> Option<usize> {
    match value {
        DynamicValue::Int(value) => usize::try_from(*value).ok(),
        _ => None,
    }
}

fn list_number(value: &DynamicValue, index: usize) -> Option<f64> {
    let DynamicValue::List(values) = value else {
        return None;
    };
    match values.get(index)? {
        DynamicValue::Int(value) => Some(*value as f64),
        DynamicValue::Float(value) => Some(*value),
        _ => None,
    }
}
