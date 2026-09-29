use std::any::{Any, TypeId};

use solverforge_bridge::{EntityClassId, VariableId};
use solverforge_core::domain::{EntityDescriptor, SolutionDescriptor, VariableDescriptor};

use crate::descriptor::extractor::DynamicEntityExtractor;
use crate::intern::intern;
use crate::state::entity_table::DynamicEntityRow;
use crate::state::PyDynamicSolution;

use super::DynamicSchema;

/// The one entity pin predicate for every compiled Python schema.
///
/// The declaration is resolved per row at import, so this stays a capture-free
/// function over Rust-owned state: no thread-local slot lookup, no Python
/// callback, and no per-candidate reinterpretation of the declared attribute.
fn pinned_row_predicate(entity: &dyn Any) -> bool {
    entity
        .downcast_ref::<DynamicEntityRow>()
        .is_some_and(|row| row.pinned)
}

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
        if let Some(pin_field) = entity.pin_field.as_deref() {
            entity_descriptor = entity_descriptor
                .with_pin_field(intern(pin_field.to_string()))
                .with_pin_predicate(pinned_row_predicate);
        }
        descriptor = descriptor.with_entity(entity_descriptor);
    }
    descriptor
}
