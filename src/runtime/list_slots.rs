use std::sync::Arc;

use pyo3::prelude::*;
use solverforge_bridge::{
    DynamicListAccess, DynamicListAccessCapabilities, DynamicListMetadata,
    DynamicListMetadataCapabilities, DynamicListVariableSlot, EntityClassId, VariableId,
};

use crate::error::panic_with_py_err;
use crate::intern::intern;
use crate::schema::types::{
    ListMetadataFieldSourceSchema, ListMetadataSchema, ListMetadataSourceSchema,
    MetadataSourceSchema, VariableSchema,
};
use crate::schema::DynamicSchema;
use crate::state::PyDynamicSolution;
use crate::value::DynamicValue;

/// Compiles one immutable core slot binding per Python planning-list variable.
///
/// List access and metadata deliberately live together at this schema boundary:
/// neither consults the current phase, a thread-local slot, or an inferred
/// fallback.  The core graph owns construction and local-search execution;
/// this adapter owns only Python-state access plus explicitly declared Python
/// metadata/callback interpretation.
pub fn list_slots(
    schema: &Arc<DynamicSchema>,
) -> Result<Vec<DynamicListVariableSlot<PyDynamicSolution>>, String> {
    schema
        .entities
        .iter()
        .enumerate()
        .flat_map(|(entity_index, entity)| {
            entity
                .variables
                .iter()
                .enumerate()
                .filter(|(_, variable)| variable.kind == "planning_list_variable")
                .map(move |(variable_index, variable)| {
                    let entity_class = EntityClassId(entity_index);
                    let variable_id = VariableId(variable_index);
                    let entity_type_name = intern(entity.type_name.clone());
                    let variable_name = intern(variable.name.clone());
                    DynamicListVariableSlot::with_access_and_metadata(
                        entity_class,
                        variable_id,
                        entity_type_name,
                        variable_name,
                        Arc::new(PyDynamicListAccess {
                            entity: entity_class,
                            variable: variable_id,
                        }),
                        Arc::new(PyDynamicListMetadata::from_variable(
                            entity_class,
                            variable_id,
                            variable,
                        )),
                    )
                    .map_err(|error| {
                        format!(
                            "failed to compile dynamic list slot {}.{}: {error}",
                            entity.type_name, variable.name
                        )
                    })
                })
        })
        .collect()
}

#[derive(Debug)]
struct PyDynamicListAccess {
    entity: EntityClassId,
    variable: VariableId,
}

impl DynamicListAccess<PyDynamicSolution> for PyDynamicListAccess {
    fn entity_class(&self) -> EntityClassId {
        self.entity
    }

    fn variable(&self) -> VariableId {
        self.variable
    }

    fn entity_count(&self, solution: &PyDynamicSolution) -> usize {
        solution.entity_count(self.entity.0)
    }

    fn element_count(&self, solution: &PyDynamicSolution) -> usize {
        solution
            .state
            .list_elements_at(self.entity.0, self.variable.0)
            .map(<[usize]>::len)
            .unwrap_or(0)
    }

    fn element(&self, solution: &PyDynamicSolution, element_index: usize) -> Option<usize> {
        solution
            .state
            .list_elements_at(self.entity.0, self.variable.0)?
            .get(element_index)
            .copied()
    }

    fn assigned_elements(&self, solution: &PyDynamicSolution) -> Vec<usize> {
        solution
            .state
            .entities
            .get(self.entity.0)
            .into_iter()
            .flat_map(|rows| rows.iter())
            .filter_map(|row| row.list_at(self.variable.0))
            .flat_map(|values| values.iter().copied())
            .collect()
    }

    fn len(&self, solution: &PyDynamicSolution, row: usize) -> usize {
        solution
            .state
            .entities
            .get(self.entity.0)
            .and_then(|rows| rows.get(row))
            .and_then(|row| row.list_at(self.variable.0))
            .map(<[usize]>::len)
            .unwrap_or(0)
    }

    fn get(&self, solution: &PyDynamicSolution, row: usize, pos: usize) -> Option<usize> {
        solution
            .state
            .entities
            .get(self.entity.0)?
            .get(row)?
            .list_at(self.variable.0)?
            .get(pos)
            .copied()
    }

    fn insert(&self, solution: &mut PyDynamicSolution, row: usize, pos: usize, value: usize) {
        assert!(
            solution.insert_existing_list_value(self.entity.0, row, self.variable.0, pos, value),
            "core supplied an invalid dynamic list insertion for entity ID {} variable ID {}",
            self.entity.0,
            self.variable.0,
        );
    }

    fn remove(&self, solution: &mut PyDynamicSolution, row: usize, pos: usize) -> Option<usize> {
        let end = pos.checked_add(1)?;
        let mut removed =
            solution.remove_existing_list_range(self.entity.0, row, self.variable.0, pos, end)?;
        debug_assert_eq!(
            removed.len(),
            1,
            "single-position dynamic list removal removed an unexpected range"
        );
        removed.pop()
    }

    fn capabilities(&self) -> DynamicListAccessCapabilities {
        DynamicListAccessCapabilities {
            set: true,
            replace: true,
            reverse: true,
            sublist: true,
        }
    }

    fn set(&self, solution: &mut PyDynamicSolution, row: usize, pos: usize, value: usize) -> bool {
        solution.set_existing_list_value(self.entity.0, row, self.variable.0, pos, value)
    }

    fn replace(&self, solution: &mut PyDynamicSolution, row: usize, values: Vec<usize>) -> bool {
        solution.replace_existing_list_value(self.entity.0, row, self.variable.0, values)
    }

    fn reverse(
        &self,
        solution: &mut PyDynamicSolution,
        row: usize,
        start: usize,
        end: usize,
    ) -> bool {
        solution.reverse_existing_list_range(self.entity.0, row, self.variable.0, start, end)
    }

    fn sublist_remove(
        &self,
        solution: &mut PyDynamicSolution,
        row: usize,
        start: usize,
        end: usize,
    ) -> Option<Vec<usize>> {
        solution.remove_existing_list_range(self.entity.0, row, self.variable.0, start, end)
    }

    fn sublist_insert(
        &self,
        solution: &mut PyDynamicSolution,
        row: usize,
        pos: usize,
        values: Vec<usize>,
    ) -> bool {
        solution.insert_existing_list_range(self.entity.0, row, self.variable.0, pos, values)
    }
}

/// Immutable Python schema metadata for one dynamic list slot.
///
/// All list metadata is already canonical when this slot is compiled. A row
/// source never reads a solution field, a solution field never scans entity
/// rows, and absent capabilities remain absent rather than receiving synthetic
/// defaults.
#[derive(Debug)]
struct PyDynamicListMetadata {
    entity: EntityClassId,
    variable: VariableId,
    element_owner: Option<MetadataSourceSchema>,
    construction_element_order: Option<MetadataSourceSchema>,
    precedence_duration: Option<MetadataSourceSchema>,
    precedence_successors: Option<MetadataSourceSchema>,
    list_metadata: ListMetadataSchema,
}

impl PyDynamicListMetadata {
    fn from_variable(entity: EntityClassId, variable: VariableId, schema: &VariableSchema) -> Self {
        Self {
            entity,
            variable,
            element_owner: schema.element_owner.clone(),
            construction_element_order: schema.construction_element_order.clone(),
            precedence_duration: schema.precedence_duration.clone(),
            precedence_successors: schema.precedence_successors.clone(),
            list_metadata: schema
                .list_metadata
                .clone()
                .expect("validated planning-list variable must retain canonical list metadata"),
        }
    }

    fn row<'a>(
        &self,
        solution: &'a PyDynamicSolution,
        row: usize,
    ) -> Option<&'a crate::state::entity_table::DynamicEntityRow> {
        solution.state.entities.get(self.entity.0)?.get(row)
    }

    fn row_value<'a>(
        &self,
        solution: &'a PyDynamicSolution,
        row: usize,
        field_name: &str,
    ) -> Option<&'a DynamicValue> {
        self.row(solution, row)?.fields.get(field_name)
    }

    fn source_value<'a>(
        &self,
        source: &ListMetadataSourceSchema,
        solution: &'a PyDynamicSolution,
        row: usize,
    ) -> Option<&'a DynamicValue> {
        match source {
            ListMetadataSourceSchema::Row(field_name) => self.row_value(solution, row, field_name),
            ListMetadataSourceSchema::SolutionField(field_name) => {
                solution.state.solution_fields.get(field_name)
            }
            ListMetadataSourceSchema::EntityCallback(_)
            | ListMetadataSourceSchema::SolutionCallback(_)
            | ListMetadataSourceSchema::Capacity(_) => None,
        }
    }

    fn capacity_field_value<'a>(
        &self,
        source: &ListMetadataFieldSourceSchema,
        solution: &'a PyDynamicSolution,
        row: usize,
    ) -> Option<&'a DynamicValue> {
        match source {
            ListMetadataFieldSourceSchema::Row(field_name) => {
                self.row_value(solution, row, field_name)
            }
            ListMetadataFieldSourceSchema::SolutionField(field_name) => {
                solution.state.solution_fields.get(field_name)
            }
        }
    }

    fn source_usize(
        &self,
        source: &ListMetadataSourceSchema,
        solution: &PyDynamicSolution,
        row: usize,
    ) -> Option<usize> {
        match source {
            ListMetadataSourceSchema::Row(_) | ListMetadataSourceSchema::SolutionField(_) => self
                .source_value(source, solution, row)
                .and_then(dynamic_value_usize),
            ListMetadataSourceSchema::EntityCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<usize>> {
                    let entity = solution.entity_callback_view(py, self.entity.0, row)?;
                    callback
                        .bind(py)
                        .call1((entity,))?
                        .extract::<usize>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::SolutionCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<usize>> {
                    let snapshot = solution.to_python_callback_view(py)?;
                    callback
                        .bind(py)
                        .call1((snapshot, row))?
                        .extract::<usize>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::Capacity(_) => None,
        }
    }

    fn source_distance(
        &self,
        source: &ListMetadataSourceSchema,
        solution: &PyDynamicSolution,
        row: usize,
        from: usize,
        to: usize,
    ) -> Option<i64> {
        match source {
            ListMetadataSourceSchema::Row(_) | ListMetadataSourceSchema::SolutionField(_) => self
                .source_value(source, solution, row)
                .and_then(|value| dynamic_nested_i64(value, &[from, to])),
            ListMetadataSourceSchema::EntityCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<i64>> {
                    let entity = solution.entity_callback_view(py, self.entity.0, row)?;
                    callback
                        .bind(py)
                        .call1((entity, from, to))?
                        .extract::<i64>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::SolutionCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<i64>> {
                    let snapshot = solution.to_python_callback_view(py)?;
                    callback
                        .bind(py)
                        .call1((snapshot, row, from, to))?
                        .extract::<i64>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::Capacity(_) => None,
        }
    }

    fn source_feasible(
        &self,
        source: &ListMetadataSourceSchema,
        solution: &PyDynamicSolution,
        row: usize,
        route: &[usize],
    ) -> Option<bool> {
        match source {
            ListMetadataSourceSchema::Capacity(capacity_schema) => {
                let capacity_limit = self
                    .capacity_field_value(&capacity_schema.capacity, solution, row)
                    .and_then(dynamic_value_i64)?;
                let demands = self.capacity_field_value(&capacity_schema.demand, solution, row)?;
                let mut load = 0_i64;
                for element in route {
                    load = load.checked_add(dynamic_nested_i64(demands, &[*element])?)?;
                }
                Some(load <= capacity_limit)
            }
            ListMetadataSourceSchema::EntityCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<bool>> {
                    let entity = solution.entity_callback_view(py, self.entity.0, row)?;
                    callback
                        .bind(py)
                        .call1((entity, route.to_vec()))?
                        .extract::<bool>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::SolutionCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<bool>> {
                    let snapshot = solution.to_python_callback_view(py)?;
                    callback
                        .bind(py)
                        .call1((snapshot, row, route.to_vec()))?
                        .extract::<bool>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::Row(_) | ListMetadataSourceSchema::SolutionField(_) => None,
        }
    }

    fn cross_position_source_distance(
        &self,
        source: &ListMetadataSourceSchema,
        solution: &PyDynamicSolution,
        from_entity: usize,
        from_position: usize,
        to_entity: usize,
        to_position: usize,
    ) -> Option<f64> {
        match source {
            ListMetadataSourceSchema::Row(_) | ListMetadataSourceSchema::SolutionField(_) => {
                let rows = solution.state.entities.get(self.entity.0)?;
                let from_values = rows.get(from_entity)?.list_at(self.variable.0)?;
                let to_values = rows.get(to_entity)?.list_at(self.variable.0)?;
                let (Some(from_value), Some(to_value)) = (
                    from_values.get(from_position).copied(),
                    to_values.get(to_position).copied(),
                ) else {
                    return Some(f64::INFINITY);
                };
                self.source_value(source, solution, from_entity)
                    .and_then(|value| dynamic_nested_i64(value, &[from_value, to_value]))
                    .map(|distance| distance as f64)
            }
            ListMetadataSourceSchema::EntityCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<f64>> {
                    let from = solution.entity_callback_view(py, self.entity.0, from_entity)?;
                    let to = solution.entity_callback_view(py, self.entity.0, to_entity)?;
                    callback
                        .bind(py)
                        .call1((from, from_position, to, to_position))?
                        .extract::<f64>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::SolutionCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<f64>> {
                    let snapshot = solution.to_python_callback_view(py)?;
                    callback
                        .bind(py)
                        .call1((snapshot, from_entity, from_position, to_entity, to_position))?
                        .extract::<f64>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::Capacity(_) => None,
        }
    }

    fn intra_position_source_distance(
        &self,
        source: &ListMetadataSourceSchema,
        solution: &PyDynamicSolution,
        entity: usize,
        from_position: usize,
        to_position: usize,
    ) -> Option<f64> {
        match source {
            ListMetadataSourceSchema::Row(_) | ListMetadataSourceSchema::SolutionField(_) => {
                let values = solution
                    .state
                    .entities
                    .get(self.entity.0)?
                    .get(entity)?
                    .list_at(self.variable.0)?;
                let (Some(from_value), Some(to_value)) = (
                    values.get(from_position).copied(),
                    values.get(to_position).copied(),
                ) else {
                    return Some(f64::INFINITY);
                };
                self.source_value(source, solution, entity)
                    .and_then(|value| dynamic_nested_i64(value, &[from_value, to_value]))
                    .map(|distance| distance as f64)
            }
            ListMetadataSourceSchema::EntityCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<f64>> {
                    let route = solution.entity_callback_view(py, self.entity.0, entity)?;
                    callback
                        .bind(py)
                        .call1((route, from_position, to_position))?
                        .extract::<f64>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::SolutionCallback(callback) => {
                Python::attach(|py| -> PyResult<Option<f64>> {
                    let snapshot = solution.to_python_callback_view(py)?;
                    callback
                        .bind(py)
                        .call1((snapshot, entity, from_position, to_position))?
                        .extract::<f64>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
            ListMetadataSourceSchema::Capacity(_) => None,
        }
    }

    fn element_metadata_value<'a>(
        solution: &'a PyDynamicSolution,
        field_name: &str,
        element: usize,
    ) -> Option<&'a DynamicValue> {
        let DynamicValue::List(values) = solution.state.solution_fields.get(field_name)? else {
            return None;
        };
        values.get(element)
    }

    fn metadata_usize(
        &self,
        source: Option<&MetadataSourceSchema>,
        solution: &PyDynamicSolution,
        element: usize,
    ) -> Option<usize> {
        match source? {
            MetadataSourceSchema::Row(field_name) => {
                Self::element_metadata_value(solution, field_name, element)
                    .and_then(dynamic_value_usize)
            }
            MetadataSourceSchema::Callback(callback) => {
                Python::attach(|py| -> PyResult<Option<usize>> {
                    let snapshot = solution.to_python_callback_view(py)?;
                    let result = callback.bind(py).call1((snapshot, element))?;
                    if result.is_none() {
                        Ok(None)
                    } else {
                        result.extract::<usize>().map(Some)
                    }
                })
                .unwrap_or_else(panic_with_py_err)
            }
        }
    }

    fn metadata_i64(
        &self,
        source: Option<&MetadataSourceSchema>,
        solution: &PyDynamicSolution,
        element: usize,
    ) -> Option<i64> {
        match source? {
            MetadataSourceSchema::Row(field_name) => {
                Self::element_metadata_value(solution, field_name, element)
                    .and_then(dynamic_value_i64)
            }
            MetadataSourceSchema::Callback(callback) => {
                Python::attach(|py| -> PyResult<Option<i64>> {
                    let snapshot = solution.to_python_callback_view(py)?;
                    callback
                        .bind(py)
                        .call1((snapshot, element))?
                        .extract::<i64>()
                        .map(Some)
                })
                .unwrap_or_else(panic_with_py_err)
            }
        }
    }

    fn extend_metadata_successors(
        &self,
        source: Option<&MetadataSourceSchema>,
        solution: &PyDynamicSolution,
        element: usize,
        successors: &mut Vec<usize>,
    ) -> bool {
        match source {
            None => false,
            Some(MetadataSourceSchema::Row(field_name)) => {
                let Some(DynamicValue::List(values)) =
                    Self::element_metadata_value(solution, field_name, element)
                else {
                    return false;
                };
                for value in values {
                    let Some(value) = dynamic_value_usize(value) else {
                        return false;
                    };
                    successors.push(value);
                }
                true
            }
            Some(MetadataSourceSchema::Callback(callback)) => {
                Python::attach(|py| -> PyResult<bool> {
                    let snapshot = solution.to_python_callback_view(py)?;
                    let result = callback.bind(py).call1((snapshot, element))?;
                    if result.is_none() {
                        return Ok(true);
                    }
                    successors.extend(result.extract::<Vec<usize>>()?);
                    Ok(true)
                })
                .unwrap_or_else(panic_with_py_err)
            }
        }
    }
}

impl DynamicListMetadata<PyDynamicSolution> for PyDynamicListMetadata {
    fn entity_class(&self) -> EntityClassId {
        self.entity
    }

    fn variable(&self) -> VariableId {
        self.variable
    }

    fn capabilities(&self) -> DynamicListMetadataCapabilities {
        DynamicListMetadataCapabilities {
            element_owner: self.element_owner.is_some(),
            construction_order_key: self.construction_element_order.is_some(),
            precedence_duration: self.precedence_duration.is_some(),
            precedence_successors: self.precedence_successors.is_some(),
            // Route distance is a value-to-value metric, not a position
            // metric fallback. Each nearby family must opt in separately.
            cross_position_distance: self.list_metadata.cross_position_distance.is_some(),
            intra_position_distance: self.list_metadata.intra_position_distance.is_some(),
            route: self.list_metadata.route.is_some(),
            savings: self.list_metadata.savings.is_some(),
        }
    }

    fn element_owner(&self, solution: &PyDynamicSolution, element: usize) -> Option<usize> {
        self.metadata_usize(self.element_owner.as_ref(), solution, element)
    }

    fn construction_order_key(&self, solution: &PyDynamicSolution, element: usize) -> Option<i64> {
        self.metadata_i64(self.construction_element_order.as_ref(), solution, element)
    }

    fn precedence_duration(&self, solution: &PyDynamicSolution, element: usize) -> Option<usize> {
        self.metadata_usize(self.precedence_duration.as_ref(), solution, element)
    }

    fn extend_precedence_successors(
        &self,
        solution: &PyDynamicSolution,
        element: usize,
        successors: &mut Vec<usize>,
    ) -> bool {
        self.extend_metadata_successors(
            self.precedence_successors.as_ref(),
            solution,
            element,
            successors,
        )
    }

    fn cross_position_distance(
        &self,
        solution: &PyDynamicSolution,
        from_entity: usize,
        from_position: usize,
        to_entity: usize,
        to_position: usize,
    ) -> Option<f64> {
        self.cross_position_source_distance(
            self.list_metadata.cross_position_distance.as_ref()?,
            solution,
            from_entity,
            from_position,
            to_entity,
            to_position,
        )
    }

    fn intra_position_distance(
        &self,
        solution: &PyDynamicSolution,
        entity: usize,
        from_position: usize,
        to_position: usize,
    ) -> Option<f64> {
        self.intra_position_source_distance(
            self.list_metadata.intra_position_distance.as_ref()?,
            solution,
            entity,
            from_position,
            to_position,
        )
    }

    fn route_depot(&self, solution: &PyDynamicSolution, entity: usize) -> Option<usize> {
        self.source_usize(&self.list_metadata.route.as_ref()?.depot, solution, entity)
    }

    fn route_distance(
        &self,
        solution: &PyDynamicSolution,
        entity: usize,
        from: usize,
        to: usize,
    ) -> Option<i64> {
        self.source_distance(
            &self.list_metadata.route.as_ref()?.distance,
            solution,
            entity,
            from,
            to,
        )
    }

    fn route_feasible(
        &self,
        solution: &PyDynamicSolution,
        entity: usize,
        route: &[usize],
    ) -> Option<bool> {
        self.source_feasible(
            &self.list_metadata.route.as_ref()?.feasible,
            solution,
            entity,
            route,
        )
    }

    fn savings_depot(&self, solution: &PyDynamicSolution, entity: usize) -> Option<usize> {
        self.source_usize(
            &self.list_metadata.savings.as_ref()?.depot,
            solution,
            entity,
        )
    }

    fn savings_metric_class(&self, solution: &PyDynamicSolution, entity: usize) -> Option<usize> {
        self.source_usize(
            &self.list_metadata.savings.as_ref()?.metric_class,
            solution,
            entity,
        )
    }

    fn savings_distance(
        &self,
        solution: &PyDynamicSolution,
        entity: usize,
        from: usize,
        to: usize,
    ) -> Option<i64> {
        self.source_distance(
            &self.list_metadata.savings.as_ref()?.distance,
            solution,
            entity,
            from,
            to,
        )
    }

    fn savings_feasible(
        &self,
        solution: &PyDynamicSolution,
        entity: usize,
        route: &[usize],
    ) -> Option<bool> {
        self.source_feasible(
            &self.list_metadata.savings.as_ref()?.feasible,
            solution,
            entity,
            route,
        )
    }
}

fn dynamic_nested_i64(value: &DynamicValue, path: &[usize]) -> Option<i64> {
    if path.is_empty() {
        return dynamic_value_i64(value);
    }
    let DynamicValue::List(values) = value else {
        return None;
    };
    dynamic_nested_i64(values.get(path[0])?, &path[1..])
}

fn dynamic_value_i64(value: &DynamicValue) -> Option<i64> {
    match value {
        DynamicValue::Int(value) => Some(*value),
        _ => None,
    }
}

fn dynamic_value_usize(value: &DynamicValue) -> Option<usize> {
    usize::try_from(dynamic_value_i64(value)?).ok()
}
