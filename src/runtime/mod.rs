pub mod distance;
pub mod dynamic_scalar_search;
pub mod list_slots;
pub mod scalar_slots;
pub mod static_fns;
pub mod thread_local;

use solverforge_core::domain::SolutionDescriptor;
use solverforge_solver::{RuntimeModel, VariableSlot};

use crate::schema::DynamicSchema;
use crate::state::PyDynamicSolution;

use distance::PyDistanceMeter;
use list_slots::list_slots;
use scalar_slots::scalar_slots;

pub fn dynamic_runtime_model(
    schema: &DynamicSchema,
    descriptor: &SolutionDescriptor,
) -> Result<RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>, String> {
    let variables = scalar_slots(schema)
        .into_iter()
        .map(VariableSlot::DynamicScalar)
        .chain(
            list_slots(schema)
                .into_iter()
                .map(VariableSlot::DynamicList),
        )
        .collect();
    RuntimeModel::new(variables).resolve_dynamic_descriptor_indexes(descriptor)
}
