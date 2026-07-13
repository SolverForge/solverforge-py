mod candidate_metric;
pub mod dynamic_assignment_group;
mod dynamic_provider;
pub mod list_slots;
pub mod scalar_slots;

use solverforge_core::domain::SolutionDescriptor;
use solverforge_solver::{DefaultCrossEntityDistanceMeter, RuntimeModel, VariableSlot};

use crate::schema::DynamicSchema;
use crate::state::PyDynamicSolution;

use list_slots::list_slots;
use scalar_slots::scalar_slots;

pub fn dynamic_runtime_model(
    schema: std::sync::Arc<DynamicSchema>,
    descriptor: &SolutionDescriptor,
) -> Result<
    RuntimeModel<
        PyDynamicSolution,
        usize,
        DefaultCrossEntityDistanceMeter,
        DefaultCrossEntityDistanceMeter,
    >,
    String,
> {
    let scalar_slots = scalar_slots(&schema);
    let groups = dynamic_assignment_group::assignment_groups(std::sync::Arc::clone(&schema))?;
    let providers = dynamic_provider::provider_registry(&schema)?;
    let candidate_metrics = candidate_metric::candidate_metrics(&schema)?;
    let variables = scalar_slots
        .into_iter()
        .map(VariableSlot::DynamicScalar)
        .chain(
            list_slots(&schema)?
                .into_iter()
                .map(VariableSlot::DynamicList),
        )
        .collect();
    RuntimeModel::new(variables)
        .with_scalar_groups(groups)
        .with_runtime_provider_registry(providers)
        .with_candidate_metrics(candidate_metrics)
        .resolve_dynamic_descriptor_indexes(descriptor)
}
