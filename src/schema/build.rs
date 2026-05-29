use std::any::TypeId;

use solverforge_bridge::{EntityClassId, VariableId};
use solverforge_core::domain::{EntityDescriptor, SolutionDescriptor, VariableDescriptor};

use crate::descriptor::extractor::DynamicEntityExtractor;
use crate::intern::intern;
use crate::state::PyDynamicSolution;

use super::DynamicSchema;

pub fn solution_descriptor(schema: &DynamicSchema) -> SolutionDescriptor {
    let mut descriptor = SolutionDescriptor::new(
        intern(schema.solution_type.clone()),
        TypeId::of::<PyDynamicSolution>(),
    );
    for (descriptor_index, entity) in schema.entities.iter().enumerate() {
        let type_name = intern(entity.type_name.clone());
        let collection = intern(entity.collection.clone());
        let mut entity_descriptor = EntityDescriptor::new(
            type_name,
            TypeId::of::<crate::state::entity_table::DynamicEntityRow>(),
            collection,
        )
        .with_logical_id(EntityClassId(descriptor_index))
        .with_extractor(Box::new(DynamicEntityExtractor::new(
            descriptor_index,
            type_name,
            collection,
        )));
        for (variable_index, variable) in entity.variables.iter().enumerate() {
            let descriptor_variable = match variable.kind.as_str() {
                "planning_list_variable" => VariableDescriptor::list(intern(variable.name.clone())),
                _ => VariableDescriptor::genuine(intern(variable.name.clone()))
                    .with_allows_unassigned(variable.allows_unassigned),
            }
            .with_logical_id(VariableId(variable_index));
            entity_descriptor = entity_descriptor.with_variable(descriptor_variable);
        }
        descriptor = descriptor.with_entity(entity_descriptor);
    }
    descriptor
}
