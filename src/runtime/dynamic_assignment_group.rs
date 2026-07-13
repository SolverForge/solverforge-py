use std::sync::Arc;

use pyo3::prelude::*;
use solverforge_bridge::{
    DynamicScalarAssignmentMetadata, DynamicScalarAssignmentMetadataCapabilities,
    DynamicScalarVariableSlot, EntityClassId, VariableId,
};
use solverforge_config::{ConstructionHeuristicType, PhaseConfig, SolverConfig};
use solverforge_solver::{ScalarGroupBinding, ScalarGroupLimits};

use crate::error::{panic_with_py_err, py_err};
use crate::intern::intern;
use crate::schema::types::{AssignmentScalarGroupSchema, MetadataSourceSchema};
use crate::schema::DynamicSchema;
use crate::state::PyDynamicSolution;
use crate::value::DynamicValue;

/// Compiles each Python assignment-group declaration into one core-owned
/// binding.  The group index is structural plan identity, never an active
/// thread-local selector or a per-solve name lookup.
pub(crate) fn assignment_groups(
    schema: Arc<DynamicSchema>,
) -> Result<Vec<ScalarGroupBinding<PyDynamicSolution>>, String> {
    schema
        .assignment_scalar_groups
        .iter()
        .enumerate()
        .map(|(group_index, group)| build_assignment_group(Arc::clone(&schema), group_index, group))
        .collect()
}

/// Preserve the Python configuration contract at the binding boundary while
/// leaving construction itself entirely to the canonical core runtime.
///
/// Named scalar construction in the dynamic model has always meant a declared
/// assignment group.  Validate that structural declaration before the core
/// begins phase construction so callers receive the same actionable Python
/// error instead of a generic core lookup failure.
pub(crate) fn validate_assignment_construction_groups(
    config: &SolverConfig,
    schema: &DynamicSchema,
) -> PyResult<()> {
    for phase in &config.phases {
        let PhaseConfig::ConstructionHeuristic(construction) = phase else {
            continue;
        };
        if !matches!(
            construction.construction_heuristic_type,
            ConstructionHeuristicType::FirstFit | ConstructionHeuristicType::CheapestInsertion
        ) {
            continue;
        }
        let Some(group_name) = construction.group_name.as_deref() else {
            continue;
        };
        if schema.assignment_scalar_group(group_name).is_none() {
            return Err(py_err(format!(
                "dynamic assignment construction configured for `{group_name}`, but no matching assignment scalar group was declared"
            )));
        }
    }
    Ok(())
}

fn build_assignment_group(
    schema: Arc<DynamicSchema>,
    group_index: usize,
    group: &AssignmentScalarGroupSchema,
) -> Result<ScalarGroupBinding<PyDynamicSolution>, String> {
    let entity_index = schema
        .entity_index_by_type(group.entity_class.as_str())
        .ok_or_else(|| {
            format!(
                "assignment scalar group `{}` targets unknown entity `{}`",
                group.name, group.entity_class
            )
        })?;
    let variable_index = schema
        .variable_index(
            entity_index,
            group.variable_name.as_str(),
            Some("planning_variable"),
        )
        .ok_or_else(|| {
            format!(
                "assignment scalar group `{}` targets unknown scalar variable `{}.{}`",
                group.name, group.entity_class, group.variable_name
            )
        })?;
    let variable = &schema.entities[entity_index].variables[variable_index];
    if !variable.allows_unassigned {
        return Err(format!(
            "assignment scalar group `{}` target `{}.{}` must allow unassigned values",
            group.name, group.entity_class, group.variable_name
        ));
    }
    if group.assignment_rule.is_some() && group.sequence_key.is_none() {
        return Err(format!(
            "assignment scalar group `{}` declares assignment_rule but no sequence_key",
            group.name
        ));
    }

    let slot = DynamicScalarVariableSlot::new(
        EntityClassId(entity_index),
        VariableId(variable_index),
        intern(group.entity_class.clone()),
        intern(group.variable_name.clone()),
        true,
    );
    let metadata = Arc::new(PyDynamicAssignmentMetadata {
        schema,
        group_index,
        entity_index,
    });
    Ok(ScalarGroupBinding::dynamic_assignment(
        intern(group.name.clone()),
        slot,
        metadata,
        scalar_group_limits(group),
    ))
}

fn scalar_group_limits(group: &AssignmentScalarGroupSchema) -> ScalarGroupLimits {
    ScalarGroupLimits {
        value_candidate_limit: group.limits.value_candidate_limit,
        group_candidate_limit: group.limits.group_candidate_limit,
        max_moves_per_step: group.limits.max_moves_per_step,
        max_augmenting_depth: group.limits.max_augmenting_depth,
        max_rematch_size: group.limits.max_rematch_size,
    }
}

struct PyDynamicAssignmentMetadata {
    schema: Arc<DynamicSchema>,
    group_index: usize,
    entity_index: usize,
}

impl PyDynamicAssignmentMetadata {
    fn group(&self) -> &AssignmentScalarGroupSchema {
        self.schema
            .assignment_scalar_groups
            .get(self.group_index)
            .expect("compiled dynamic assignment group index must remain valid")
    }

    fn metadata_source(&self, hook_name: &str) -> Option<&MetadataSourceSchema> {
        let group = self.group();
        match hook_name {
            "required_entity" => group.required_entity.as_ref(),
            "capacity_key" => group.capacity_key.as_ref(),
            "position_key" => group.position_key.as_ref(),
            "sequence_key" => group.sequence_key.as_ref(),
            _ => None,
        }
    }

    fn field_value<'a>(
        &self,
        solution: &'a PyDynamicSolution,
        hook_name: &str,
        entity_index: usize,
    ) -> Result<Option<&'a DynamicValue>, PyErr> {
        let Some(field_name) = self
            .metadata_source(hook_name)
            .and_then(MetadataSourceSchema::row)
        else {
            return Ok(None);
        };
        solution
            .state
            .entities
            .get(self.entity_index)
            .and_then(|rows| rows.get(entity_index))
            .and_then(|row| row.fields.get(field_name))
            .map(Some)
            .ok_or_else(|| {
                py_err(format!(
                    "assignment scalar group `{}` field `{field_name}` is missing on row {entity_index}",
                    self.group().name
                ))
            })
    }

    fn callback(&self, hook_name: &str) -> Option<&Py<PyAny>> {
        let group = self.group();
        match hook_name {
            "required_entity" => group
                .required_entity
                .as_ref()
                .and_then(MetadataSourceSchema::callback),
            "capacity_key" => group
                .capacity_key
                .as_ref()
                .and_then(MetadataSourceSchema::callback),
            "assignment_rule" => group.assignment_rule.as_ref(),
            "position_key" => group
                .position_key
                .as_ref()
                .and_then(MetadataSourceSchema::callback),
            "sequence_key" => group
                .sequence_key
                .as_ref()
                .and_then(MetadataSourceSchema::callback),
            "entity_order" => group.entity_order.as_ref(),
            "value_order" => group.value_order.as_ref(),
            _ => None,
        }
    }

    fn callback_value<T>(
        &self,
        solution: &PyDynamicSolution,
        hook_name: &str,
        args: &[usize],
        extract: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<T>,
    ) -> Result<Option<T>, PyErr> {
        let Some(callback) = self.callback(hook_name) else {
            return Ok(None);
        };
        Python::attach(|py| {
            let snapshot = if self.group().sync_solution_before_callbacks {
                solution.to_python_callback_view(py)?
            } else {
                solution.to_python_unsynced_callback_view(py)?
            };
            let result = match args {
                [a] => callback.bind(py).call1((snapshot, *a))?,
                [a, b] => callback.bind(py).call1((snapshot, *a, *b))?,
                [a, b, c, d] => callback.bind(py).call1((snapshot, *a, *b, *c, *d))?,
                _ => {
                    return Err(py_err(format!(
                        "unsupported assignment hook arity {}",
                        args.len()
                    )))
                }
            };
            if result.is_none() {
                Ok(None)
            } else {
                extract(&result).map(Some)
            }
        })
    }

    fn required_entity_value(&self, solution: &PyDynamicSolution, entity_index: usize) -> bool {
        if let Some(value) = self
            .field_value(solution, "required_entity", entity_index)
            .unwrap_or_else(panic_with_py_err)
        {
            return match value {
                DynamicValue::Bool(value) => *value,
                _ => {
                    panic!("assignment required_entity field must be bool after import validation")
                }
            };
        }
        self.callback_value(solution, "required_entity", &[entity_index], |value| {
            value.extract::<bool>()
        })
        .unwrap_or_else(panic_with_py_err)
        .unwrap_or(false)
    }

    fn capacity_key_value(
        &self,
        solution: &PyDynamicSolution,
        entity_index: usize,
        candidate_value: usize,
    ) -> Option<usize> {
        if let Some(value) = self
            .field_value(solution, "capacity_key", entity_index)
            .unwrap_or_else(panic_with_py_err)
        {
            return indexed_usize(value, candidate_value).unwrap_or_else(panic_with_py_err);
        }
        self.callback_value(
            solution,
            "capacity_key",
            &[entity_index, candidate_value],
            |value| value.extract::<usize>(),
        )
        .unwrap_or_else(panic_with_py_err)
    }

    fn position_key_value(&self, solution: &PyDynamicSolution, entity_index: usize) -> Option<i64> {
        if let Some(value) = self
            .field_value(solution, "position_key", entity_index)
            .unwrap_or_else(panic_with_py_err)
        {
            return match value {
                DynamicValue::Int(value) => Some(*value),
                _ => panic!(
                    "assignment position_key field must be an integer after import validation"
                ),
            };
        }
        self.callback("position_key")?;
        self.callback_value(solution, "position_key", &[entity_index], |value| {
            value.extract::<i64>()
        })
        .unwrap_or_else(panic_with_py_err)
        .or(Some(0))
    }

    fn sequence_key_value(
        &self,
        solution: &PyDynamicSolution,
        entity_index: usize,
        candidate_value: usize,
    ) -> Option<usize> {
        if let Some(value) = self
            .field_value(solution, "sequence_key", entity_index)
            .unwrap_or_else(panic_with_py_err)
        {
            return match value {
                DynamicValue::Int(value) if *value >= 0 => Some(*value as usize),
                _ => panic!(
                    "assignment sequence_key field must be a non-negative integer after import validation"
                ),
            };
        }
        self.callback_value(
            solution,
            "sequence_key",
            &[entity_index, candidate_value],
            |value| value.extract::<usize>(),
        )
        .unwrap_or_else(panic_with_py_err)
    }

    fn entity_order_key_value(
        &self,
        solution: &PyDynamicSolution,
        entity_index: usize,
    ) -> Option<i64> {
        self.callback("entity_order")?;
        self.callback_value(solution, "entity_order", &[entity_index], |value| {
            value.extract::<i64>()
        })
        .unwrap_or_else(panic_with_py_err)
        .or(Some(0))
    }

    fn value_order_key_value(
        &self,
        solution: &PyDynamicSolution,
        entity_index: usize,
        value_index: usize,
    ) -> Option<i64> {
        self.callback("value_order")?;
        self.callback_value(
            solution,
            "value_order",
            &[entity_index, value_index],
            |value| value.extract::<i64>(),
        )
        .unwrap_or_else(panic_with_py_err)
        .or(Some(0))
    }

    fn assignment_edge_allowed_value(
        &self,
        solution: &PyDynamicSolution,
        left_entity: usize,
        left_value: usize,
        right_entity: usize,
        right_value: usize,
    ) -> bool {
        self.callback_value(
            solution,
            "assignment_rule",
            &[left_entity, left_value, right_entity, right_value],
            |value| value.extract::<bool>(),
        )
        .unwrap_or_else(panic_with_py_err)
        .unwrap_or(true)
    }
}

impl DynamicScalarAssignmentMetadata<PyDynamicSolution> for PyDynamicAssignmentMetadata {
    fn capabilities(&self) -> DynamicScalarAssignmentMetadataCapabilities {
        let group = self.group();
        DynamicScalarAssignmentMetadataCapabilities {
            required_entity: group.required_entity.is_some(),
            capacity_key: group.capacity_key.is_some(),
            position_key: group.position_key.is_some(),
            sequence_key: group.sequence_key.is_some(),
            entity_order: group.entity_order.is_some(),
            value_order: group.value_order.is_some(),
            assignment_rule: group.assignment_rule.is_some(),
        }
    }

    fn required_entity(&self, solution: &PyDynamicSolution, entity_index: usize) -> bool {
        self.required_entity_value(solution, entity_index)
    }

    fn capacity_key(
        &self,
        solution: &PyDynamicSolution,
        entity_index: usize,
        value: usize,
    ) -> Option<usize> {
        self.capacity_key_value(solution, entity_index, value)
    }

    fn position_key(&self, solution: &PyDynamicSolution, entity_index: usize) -> Option<i64> {
        self.position_key_value(solution, entity_index)
    }

    fn sequence_key(
        &self,
        solution: &PyDynamicSolution,
        entity_index: usize,
        value: usize,
    ) -> Option<usize> {
        self.sequence_key_value(solution, entity_index, value)
    }

    fn entity_order_key(&self, solution: &PyDynamicSolution, entity_index: usize) -> Option<i64> {
        self.entity_order_key_value(solution, entity_index)
    }

    fn value_order_key(
        &self,
        solution: &PyDynamicSolution,
        entity_index: usize,
        value: usize,
    ) -> Option<i64> {
        self.value_order_key_value(solution, entity_index, value)
    }

    fn assignment_edge_allowed(
        &self,
        solution: &PyDynamicSolution,
        left_entity: usize,
        left_value: usize,
        right_entity: usize,
        right_value: usize,
    ) -> bool {
        self.assignment_edge_allowed_value(
            solution,
            left_entity,
            left_value,
            right_entity,
            right_value,
        )
    }
}

fn indexed_usize(field_value: &DynamicValue, candidate_value: usize) -> PyResult<Option<usize>> {
    let DynamicValue::List(values) = field_value else {
        return Err(py_err(
            "assignment capacity_key field must be a list indexed by candidate value",
        ));
    };
    let value = values.get(candidate_value).ok_or_else(|| {
        py_err(format!(
            "assignment capacity_key field has no entry for candidate {candidate_value}"
        ))
    })?;
    match value {
        DynamicValue::None => Ok(None),
        DynamicValue::Int(value) if *value >= 0 => Ok(Some(*value as usize)),
        _ => Err(py_err(
            "assignment capacity_key field must contain non-negative integers or None",
        )),
    }
}
