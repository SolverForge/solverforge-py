use std::collections::{BTreeMap, BTreeSet};

use crate::value::DynamicValue;

#[derive(Debug, Clone, Default)]
pub struct DynamicState {
    pub entities: Vec<Vec<DynamicEntityRow>>,
    pub facts: Vec<Vec<DynamicEntityRow>>,
    pub list_elements: Vec<BTreeMap<String, Vec<usize>>>,
    pub solution_fields: BTreeMap<String, DynamicValue>,
}

impl DynamicState {
    pub fn is_initialized(&self, schema: &crate::schema::DynamicSchema) -> bool {
        for (entity_index, entity) in schema.entities.iter().enumerate() {
            let Some(rows) = self.entities.get(entity_index) else {
                return false;
            };
            for row in rows {
                for variable in &entity.variables {
                    if variable.kind == "planning_variable"
                        && !variable.allows_unassigned
                        && row.scalar(variable.name.as_str()).is_none()
                    {
                        return false;
                    }
                }
            }
        }
        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct DynamicEntityRow {
    pub fields: BTreeMap<String, DynamicValue>,
    pub shadow_fields: BTreeSet<String>,
    pub scalars: BTreeMap<String, Option<usize>>,
    pub candidates: BTreeMap<String, Vec<usize>>,
    pub lists: BTreeMap<String, Vec<usize>>,
}

impl DynamicEntityRow {
    pub fn scalar(&self, name: &str) -> Option<usize> {
        self.scalars.get(name).copied().flatten()
    }

    pub fn set_scalar(&mut self, name: &str, value: Option<usize>) {
        self.scalars.insert(name.to_string(), value);
    }
}
