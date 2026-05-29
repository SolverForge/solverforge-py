use solverforge_bridge::{DynamicScalarVariableSlot, EntityClassId, VariableId};

use crate::intern::intern;
use crate::schema::DynamicSchema;
use crate::state::PyDynamicSolution;

pub fn scalar_slots(schema: &DynamicSchema) -> Vec<DynamicScalarVariableSlot<PyDynamicSolution>> {
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
                    DynamicScalarVariableSlot::new(
                        EntityClassId(entity_index),
                        VariableId(variable_index),
                        intern(entity.type_name.clone()),
                        intern(variable.name.clone()),
                        variable.allows_unassigned,
                    )
                })
        })
        .collect()
}
