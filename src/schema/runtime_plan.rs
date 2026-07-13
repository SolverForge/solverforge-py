use std::sync::Arc;

use solverforge_core::domain::SolutionDescriptor;
use solverforge_solver::{DefaultCrossEntityDistanceMeter, RuntimeModel};

use crate::runtime::dynamic_runtime_model;
use crate::schema::build::solution_descriptor;
use crate::state::PyDynamicSolution;

use super::DynamicSchema;

/// The immutable runtime identity of one compiled Python schema.
///
/// A solution retains this plan instead of independently retaining a schema,
/// descriptor, or runtime model.  That makes every direct, retained, and
/// cloned solve use the exact same compiled runtime behavior.
pub type DynamicRuntimeModel = RuntimeModel<
    PyDynamicSolution,
    usize,
    DefaultCrossEntityDistanceMeter,
    DefaultCrossEntityDistanceMeter,
>;

#[derive(Debug)]
pub struct CompiledRuntimePlan {
    schema: Arc<DynamicSchema>,
    descriptor: SolutionDescriptor,
    model: DynamicRuntimeModel,
}

impl CompiledRuntimePlan {
    /// Compile a schema into the one runtime plan which may own solutions.
    ///
    /// This is intentionally the only Rust construction route for a dynamic
    /// solution: the schema, descriptor, and runtime model cannot be mixed
    /// across independently compiled plans.
    pub fn from_schema(schema: Arc<DynamicSchema>) -> Result<Self, String> {
        let descriptor = solution_descriptor(schema.as_ref());
        let model = dynamic_runtime_model(Arc::clone(&schema), &descriptor)?;
        Ok(Self {
            schema,
            descriptor,
            model,
        })
    }

    pub fn schema(&self) -> &DynamicSchema {
        self.schema.as_ref()
    }

    pub fn descriptor(&self) -> &SolutionDescriptor {
        &self.descriptor
    }

    pub(crate) fn model(&self) -> &DynamicRuntimeModel {
        &self.model
    }

    pub(crate) fn default_time_limit_secs(&self) -> u64 {
        if self.schema.entities.iter().any(|entity| {
            entity
                .variables
                .iter()
                .any(|variable| variable.kind == "planning_list_variable")
        }) {
            60
        } else {
            30
        }
    }
}
