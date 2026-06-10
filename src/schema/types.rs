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

#[derive(Debug)]
pub struct VariableSchema {
    pub name: String,
    pub kind: String,
    pub value_range_provider: Option<String>,
    pub allows_unassigned: bool,
    pub element_collection: Option<String>,
    pub element_owner: Option<Py<PyAny>>,
    pub route_depot: Option<Py<PyAny>>,
    pub route_metric_class: Option<Py<PyAny>>,
    pub route_distance: Option<Py<PyAny>>,
    pub route_feasible: Option<Py<PyAny>>,
}

#[derive(Debug)]
pub struct ShadowUpdateSchema {
    pub list_owner: String,
    pub post_update_listener: Py<PyAny>,
}

pub struct DynamicSchema {
    pub solution_type: String,
    pub score_family: String,
    pub entities: Vec<EntitySchema>,
    pub facts: Vec<FactSchema>,
    pub constraints: Py<PyAny>,
    pub scalar_groups: Py<PyAny>,
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
            .finish_non_exhaustive()
    }
}
