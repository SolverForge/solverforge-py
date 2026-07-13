use std::sync::Arc;

use pyo3::prelude::*;

#[derive(Debug)]
pub struct EntitySchema {
    pub type_name: String,
    pub collection: String,
    pub variables: Vec<VariableSchema>,
}

#[derive(Debug)]
pub struct FactSchema {
    pub type_name: String,
    pub collection: String,
}

/// A declared field source for list metadata.  A row field and a solution-root
/// field remain distinct in the compiled schema so neither can silently fall
/// back to the other scope.
#[derive(Debug, Clone)]
pub enum ListMetadataFieldSourceSchema {
    Row(String),
    SolutionField(String),
}

impl ListMetadataFieldSourceSchema {
    pub fn row(&self) -> Option<&str> {
        match self {
            Self::Row(field) => Some(field.as_str()),
            Self::SolutionField(_) => None,
        }
    }

    pub fn solution_field(&self) -> Option<&str> {
        match self {
            Self::Row(_) => None,
            Self::SolutionField(field) => Some(field.as_str()),
        }
    }
}

/// One explicitly scoped source used by a list metadata hook.
#[derive(Debug, Clone)]
pub enum ListMetadataSourceSchema {
    Row(String),
    SolutionField(String),
    EntityCallback(Arc<Py<PyAny>>),
    SolutionCallback(Arc<Py<PyAny>>),
    Capacity(ListCapacityFeasibilitySchema),
}

impl ListMetadataSourceSchema {
    pub fn row(&self) -> Option<&str> {
        match self {
            Self::Row(field) => Some(field.as_str()),
            Self::SolutionField(_)
            | Self::EntityCallback(_)
            | Self::SolutionCallback(_)
            | Self::Capacity(_) => None,
        }
    }

    pub fn solution_field(&self) -> Option<&str> {
        match self {
            Self::SolutionField(field) => Some(field.as_str()),
            Self::Row(_)
            | Self::EntityCallback(_)
            | Self::SolutionCallback(_)
            | Self::Capacity(_) => None,
        }
    }

    pub fn capacity(&self) -> Option<&ListCapacityFeasibilitySchema> {
        match self {
            Self::Capacity(capacity) => Some(capacity),
            Self::Row(_)
            | Self::SolutionField(_)
            | Self::EntityCallback(_)
            | Self::SolutionCallback(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListCapacityFeasibilitySchema {
    pub capacity: ListMetadataFieldSourceSchema,
    pub demand: ListMetadataFieldSourceSchema,
}

/// The independently complete route bundle used by route-local construction
/// and neighborhood phases.
#[derive(Debug, Clone)]
pub struct ListRouteMetadataSchema {
    pub depot: ListMetadataSourceSchema,
    pub distance: ListMetadataSourceSchema,
    pub feasible: ListMetadataSourceSchema,
}

/// The independently complete Clarke-Wright/savings bundle.  It is never
/// inferred from the route bundle at runtime.
#[derive(Debug, Clone)]
pub struct ListSavingsMetadataSchema {
    pub depot: ListMetadataSourceSchema,
    pub metric_class: ListMetadataSourceSchema,
    pub distance: ListMetadataSourceSchema,
    pub feasible: ListMetadataSourceSchema,
}

/// Canonical schema provenance for one planning-list variable.
#[derive(Debug, Default, Clone)]
pub struct ListMetadataSchema {
    pub route: Option<ListRouteMetadataSchema>,
    pub savings: Option<ListSavingsMetadataSchema>,
    pub cross_position_distance: Option<ListMetadataSourceSchema>,
    pub intra_position_distance: Option<ListMetadataSourceSchema>,
}

impl ListMetadataSchema {
    pub fn is_configured(&self) -> bool {
        self.route.is_some()
            || self.savings.is_some()
            || self.cross_position_distance.is_some()
            || self.intra_position_distance.is_some()
    }
}

#[derive(Debug, Clone)]
pub enum MetadataSourceSchema {
    Row(String),
    Callback(Arc<Py<PyAny>>),
}

impl MetadataSourceSchema {
    pub fn row(&self) -> Option<&str> {
        match self {
            Self::Row(field) => Some(field.as_str()),
            Self::Callback(_) => None,
        }
    }

    pub fn callback(&self) -> Option<&Py<PyAny>> {
        match self {
            Self::Row(_) => None,
            Self::Callback(callback) => Some(callback.as_ref()),
        }
    }
}

#[derive(Debug, Default)]
pub struct VariableSchema {
    pub name: String,
    pub storage_name: String,
    pub kind: String,
    pub value_range_provider: Option<String>,
    pub candidate_values: Option<Py<PyAny>>,
    /// The one ordered source for nearby values: either a row field or a
    /// Python callback. The parsed schema never retains both forms.
    pub nearby_value_candidates: Option<MetadataSourceSchema>,
    /// The one ordered source for nearby entities: either a row field or a
    /// Python callback. The parsed schema never retains both forms.
    pub nearby_entity_candidates: Option<MetadataSourceSchema>,
    /// The one distance source for nearby values: either a row field or a
    /// Python callback. `None` preserves source-order distance semantics.
    pub nearby_value_distance_meter: Option<MetadataSourceSchema>,
    /// The one distance source for nearby entities: either a row field or a
    /// Python callback. `None` preserves source-order distance semantics.
    pub nearby_entity_distance_meter: Option<MetadataSourceSchema>,
    pub allows_unassigned: bool,
    pub element_collection: Option<String>,
    pub element_owner: Option<MetadataSourceSchema>,
    pub construction_element_order: Option<MetadataSourceSchema>,
    pub precedence_duration: Option<MetadataSourceSchema>,
    pub precedence_successors: Option<MetadataSourceSchema>,
    /// Present exactly for planning-list variables.  The nested object is the
    /// sole source of route, savings, and position-metric provenance.
    pub list_metadata: Option<ListMetadataSchema>,
}

#[derive(Debug)]
pub struct ShadowUpdateSchema {
    pub list_owner: String,
    pub post_update_listener: Py<PyAny>,
}

#[derive(Debug, Default)]
pub struct AssignmentScalarGroupLimitsSchema {
    pub value_candidate_limit: Option<usize>,
    pub group_candidate_limit: Option<usize>,
    pub max_moves_per_step: Option<usize>,
    pub max_augmenting_depth: Option<usize>,
    pub max_rematch_size: Option<usize>,
}

#[derive(Debug)]
pub struct AssignmentScalarGroupSchema {
    pub name: String,
    pub entity_class: String,
    pub variable_name: String,
    pub required_entity: Option<MetadataSourceSchema>,
    pub capacity_key: Option<MetadataSourceSchema>,
    pub assignment_rule: Option<Py<PyAny>>,
    pub position_key: Option<MetadataSourceSchema>,
    pub sequence_key: Option<MetadataSourceSchema>,
    pub entity_order: Option<Py<PyAny>>,
    pub value_order: Option<Py<PyAny>>,
    pub sync_solution_before_callbacks: bool,
    pub limits: AssignmentScalarGroupLimitsSchema,
}

pub struct DynamicSchema {
    pub solution_type: String,
    pub score_family: String,
    pub entities: Vec<EntitySchema>,
    pub facts: Vec<FactSchema>,
    pub constraints: Py<PyAny>,
    pub scalar_groups: Py<PyAny>,
    pub assignment_scalar_groups: Vec<AssignmentScalarGroupSchema>,
    pub conflict_repairs: Py<PyAny>,
    pub candidate_metrics: Py<PyAny>,
    pub shadow_updates: Vec<ShadowUpdateSchema>,
}

impl std::fmt::Debug for DynamicSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynamicSchema")
            .field("solution_type", &self.solution_type)
            .field("score_family", &self.score_family)
            .field("entities", &self.entities)
            .field("facts", &self.facts)
            .field("assignment_scalar_groups", &self.assignment_scalar_groups)
            .finish_non_exhaustive()
    }
}

impl DynamicSchema {
    pub fn assignment_scalar_group(
        &self,
        group_name: &str,
    ) -> Option<&AssignmentScalarGroupSchema> {
        self.assignment_scalar_groups
            .iter()
            .find(|group| group.name == group_name)
    }

    pub fn entity_index_by_type(&self, type_name: &str) -> Option<usize> {
        self.entities
            .iter()
            .position(|entity| entity.type_name == type_name)
    }

    pub fn variable_index(
        &self,
        entity_index: usize,
        variable_name: &str,
        kind: Option<&str>,
    ) -> Option<usize> {
        self.entities
            .get(entity_index)?
            .variables
            .iter()
            .position(|variable| {
                variable.name == variable_name && kind.is_none_or(|kind| variable.kind == kind)
            })
    }
}
