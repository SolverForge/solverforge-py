use solverforge_bridge::{DynamicListVariableSlot, EntityClassId, VariableId};

use crate::intern::intern;
use crate::schema::DynamicSchema;
use crate::state::PyDynamicSolution;

pub fn list_slots(schema: &DynamicSchema) -> Vec<DynamicListVariableSlot<PyDynamicSolution>> {
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
                    DynamicListVariableSlot::new(
                        EntityClassId(entity_index),
                        VariableId(variable_index),
                        intern(entity.type_name.clone()),
                        intern(variable.name.clone()),
                    )
                })
        })
        .collect()
}
