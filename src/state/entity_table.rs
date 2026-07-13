use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::value::DynamicValue;

#[derive(Debug, Clone, Default)]
pub struct DynamicState {
    pub entities: Vec<Vec<DynamicEntityRow>>,
    pub facts: Vec<Vec<DynamicEntityRow>>,
    pub scalar_value_ranges: Vec<Vec<Option<Arc<[usize]>>>>,
    pub list_elements: Vec<Vec<Option<Arc<[usize]>>>>,
    pub solution_fields: Arc<BTreeMap<String, DynamicValue>>,
}

impl DynamicState {
    pub fn is_initialized(&self, schema: &crate::schema::DynamicSchema) -> bool {
        for (entity_index, entity) in schema.entities.iter().enumerate() {
            let Some(rows) = self.entities.get(entity_index) else {
                return false;
            };
            for row in rows {
                for (variable_index, variable) in entity.variables.iter().enumerate() {
                    if variable.kind == "planning_variable"
                        && !variable.allows_unassigned
                        && row.scalar_at(variable_index).is_none()
                    {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn list_elements_at(&self, entity_index: usize, variable_index: usize) -> Option<&[usize]> {
        self.list_elements
            .get(entity_index)?
            .get(variable_index)?
            .as_deref()
    }

    pub fn scalar_value_range_at(
        &self,
        entity_index: usize,
        variable_index: usize,
    ) -> Option<&[usize]> {
        self.scalar_value_ranges
            .get(entity_index)?
            .get(variable_index)?
            .as_deref()
    }

    pub fn set_list_elements_at(
        &mut self,
        entity_index: usize,
        variable_index: usize,
        values: Arc<[usize]>,
    ) {
        if entity_index >= self.list_elements.len() {
            self.list_elements.resize_with(entity_index + 1, Vec::new);
        }
        let entity_elements = &mut self.list_elements[entity_index];
        if variable_index >= entity_elements.len() {
            entity_elements.resize(variable_index + 1, None);
        }
        entity_elements[variable_index] = Some(values);
    }
}

#[derive(Debug, Clone, Default)]
pub struct DynamicEntityRow {
    pub fields: Arc<BTreeMap<String, DynamicValue>>,
    pub instance_fields: BTreeSet<String>,
    pub native_equality_fields: BTreeSet<String>,
    pub shadow_fields: BTreeSet<String>,
    pub scalar_values: Vec<Option<usize>>,
    pub candidate_values: Vec<Option<Arc<[usize]>>>,
    pub list_values: Vec<Option<Vec<usize>>>,
}

impl DynamicEntityRow {
    pub fn with_variable_count(variable_count: usize) -> Self {
        Self {
            scalar_values: vec![None; variable_count],
            candidate_values: vec![None; variable_count],
            list_values: vec![None; variable_count],
            ..Self::default()
        }
    }

    pub fn scalar_at(&self, variable_index: usize) -> Option<usize> {
        self.scalar_values.get(variable_index).copied().flatten()
    }

    pub fn set_scalar_at(&mut self, variable_index: usize, value: Option<usize>) {
        if variable_index >= self.scalar_values.len() {
            self.scalar_values.resize(variable_index + 1, None);
        }
        self.scalar_values[variable_index] = value;
    }

    pub fn candidates_at(&self, variable_index: usize) -> Option<&[usize]> {
        self.candidate_values
            .get(variable_index)
            .and_then(Option::as_ref)
            .map(Arc::as_ref)
    }

    pub fn set_candidate_arc_at(&mut self, variable_index: usize, values: Arc<[usize]>) {
        if variable_index >= self.candidate_values.len() {
            self.candidate_values.resize(variable_index + 1, None);
        }
        self.candidate_values[variable_index] = Some(values);
    }

    pub fn set_candidate_vec_at(&mut self, variable_index: usize, values: Vec<usize>) {
        self.set_candidate_arc_at(variable_index, Arc::<[usize]>::from(values));
    }

    pub fn set_field(&mut self, name: String, value: DynamicValue) -> Option<DynamicValue> {
        Arc::make_mut(&mut self.fields).insert(name, value)
    }

    pub fn list_at(&self, variable_index: usize) -> Option<&[usize]> {
        self.list_values
            .get(variable_index)
            .and_then(Option::as_ref)
            .map(Vec::as_slice)
    }

    pub fn list_mut_at(&mut self, variable_index: usize) -> Option<&mut Vec<usize>> {
        self.list_values
            .get_mut(variable_index)
            .and_then(Option::as_mut)
    }

    pub fn set_list_at(&mut self, variable_index: usize, values: Vec<usize>) {
        if variable_index >= self.list_values.len() {
            self.list_values.resize(variable_index + 1, None);
        }
        self.list_values[variable_index] = Some(values);
    }
}
