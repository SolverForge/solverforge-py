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

#[derive(Debug, Default)]
pub struct VariableSchema {
    pub name: String,
    pub storage_name: String,
    pub kind: String,
    pub value_range_provider: Option<String>,
    pub candidate_values: Option<Py<PyAny>>,
    pub nearby_value_candidates: Option<Py<PyAny>>,
    pub nearby_entity_candidates: Option<Py<PyAny>>,
    pub nearby_value_distance_meter: Option<Py<PyAny>>,
    pub nearby_entity_distance_meter: Option<Py<PyAny>>,
    pub allows_unassigned: bool,
    pub element_collection: Option<String>,
    pub element_owner: Option<Py<PyAny>>,
    pub construction_element_order_key: Option<Py<PyAny>>,
    pub precedence_duration: Option<Py<PyAny>>,
    pub precedence_successors: Option<Py<PyAny>>,
    pub route_depot: Option<Py<PyAny>>,
    pub route_depot_entity: Option<Py<PyAny>>,
    pub route_depot_field: Option<String>,
    pub route_metric_class: Option<Py<PyAny>>,
    pub route_metric_class_entity: Option<Py<PyAny>>,
    pub route_metric_class_field: Option<String>,
    pub route_distance: Option<Py<PyAny>>,
    pub route_distance_entity: Option<Py<PyAny>>,
    pub route_distance_matrix_field: Option<String>,
    pub route_feasible: Option<Py<PyAny>>,
    pub route_feasible_entity: Option<Py<PyAny>>,
    pub route_capacity_field: Option<String>,
    pub route_demand_field: Option<String>,
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
    pub required_entity: Option<Py<PyAny>>,
    pub capacity_key: Option<Py<PyAny>>,
    pub assignment_rule: Option<Py<PyAny>>,
    pub position_key: Option<Py<PyAny>>,
    pub sequence_key: Option<Py<PyAny>>,
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
}
