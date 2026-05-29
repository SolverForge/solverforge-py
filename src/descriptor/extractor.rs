use std::any::{Any, TypeId};

use solverforge_core::domain::{EntityExtractor, EntityRef};

use crate::state::entity_table::DynamicEntityRow;
use crate::state::PyDynamicSolution;

#[derive(Debug, Clone)]
pub struct DynamicEntityExtractor {
    descriptor_index: usize,
    type_name: &'static str,
    collection_field: &'static str,
}

impl DynamicEntityExtractor {
    pub fn new(
        descriptor_index: usize,
        type_name: &'static str,
        collection_field: &'static str,
    ) -> Self {
        Self {
            descriptor_index,
            type_name,
            collection_field,
        }
    }

    fn rows<'a>(&self, solution: &'a dyn Any) -> Option<&'a [DynamicEntityRow]> {
        let solution = solution.downcast_ref::<PyDynamicSolution>()?;
        solution
            .state
            .entities
            .get(self.descriptor_index)
            .map(Vec::as_slice)
    }

    fn rows_mut<'a>(&self, solution: &'a mut dyn Any) -> Option<&'a mut [DynamicEntityRow]> {
        let solution = solution.downcast_mut::<PyDynamicSolution>()?;
        solution
            .state
            .entities
            .get_mut(self.descriptor_index)
            .map(Vec::as_mut_slice)
    }
}

impl EntityExtractor for DynamicEntityExtractor {
    fn count(&self, solution: &dyn Any) -> Option<usize> {
        Some(self.rows(solution)?.len())
    }

    fn get<'a>(&self, solution: &'a dyn Any, index: usize) -> Option<&'a dyn Any> {
        self.rows(solution)?.get(index).map(|row| row as &dyn Any)
    }

    fn get_mut<'a>(&self, solution: &'a mut dyn Any, index: usize) -> Option<&'a mut dyn Any> {
        self.rows_mut(solution)?
            .get_mut(index)
            .map(|row| row as &mut dyn Any)
    }

    fn entity_refs(&self, solution: &dyn Any) -> Vec<EntityRef> {
        let Some(rows) = self.rows(solution) else {
            return Vec::new();
        };
        (0..rows.len())
            .map(|index| EntityRef::new(index, self.type_name, self.collection_field))
            .collect()
    }

    fn clone_box(&self) -> Box<dyn EntityExtractor> {
        Box::new(self.clone())
    }

    fn clone_entity_boxed(
        &self,
        solution: &dyn Any,
        index: usize,
    ) -> Option<Box<dyn Any + Send + Sync>> {
        self.rows(solution)?
            .get(index)
            .cloned()
            .map(|row| Box::new(row) as Box<dyn Any + Send + Sync>)
    }

    fn entity_type_id(&self) -> TypeId {
        TypeId::of::<DynamicEntityRow>()
    }
}
