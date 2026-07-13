use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyInt, PyList, PyString};
use solverforge_config::SolverConfig;

use crate::schema::runtime_plan::CompiledRuntimePlan;
use crate::schema::types::{
    ListMetadataFieldSourceSchema, ListMetadataSchema, ListMetadataSourceSchema,
    MetadataSourceSchema,
};
use crate::schema::DynamicSchema;
use crate::state::callback_view::PythonCallbackView;
use crate::state::entity_table::{DynamicEntityRow, DynamicState};
use crate::state::PyDynamicSolution;
use crate::value::DynamicValue;

pub fn import_solution(
    py_solution: &Bound<'_, PyAny>,
    runtime_plan: Arc<CompiledRuntimePlan>,
) -> PyResult<PyDynamicSolution> {
    let schema = runtime_plan.schema();
    let solution_fields = import_shared_solution_fields(py_solution, schema)?;
    let callback_root_fields = import_callback_root_context(py_solution, schema)?;
    let shared_field_names = solution_fields.keys().cloned().collect::<BTreeSet<_>>();
    let row_metadata_fields = row_metadata_fields(schema);
    let mut entity_tables = Vec::new();
    let mut entity_objects = Vec::new();
    let mut scalar_value_ranges = Vec::new();
    let mut list_elements = Vec::new();
    for entity in &schema.entities {
        let collection = py_solution.getattr(entity.collection.as_str())?;
        let collection = collection.cast::<PyList>()?;
        let variable_names = entity
            .variables
            .iter()
            .map(|variable| variable.name.as_str())
            .collect::<BTreeSet<_>>();
        let mut provider_values =
            Vec::<Option<Arc<[usize]>>>::with_capacity(entity.variables.len());
        let mut list_element_values =
            Vec::<Option<Arc<[usize]>>>::with_capacity(entity.variables.len());
        for variable in &entity.variables {
            if variable.kind == "planning_variable" {
                let provider = variable.value_range_provider.as_ref().ok_or_else(|| {
                    crate::error::py_err(format!(
                        "planning variable `{}.{}` has no value_range_provider",
                        entity.type_name, variable.name
                    ))
                })?;
                let values = py_solution
                    .getattr(provider.as_str())?
                    .cast::<PyList>()?
                    .iter()
                    .map(|item| item.extract::<usize>())
                    .collect::<PyResult<Vec<_>>>()?;
                provider_values.push(Some(Arc::<[usize]>::from(values)));
                list_element_values.push(None);
            } else if variable.kind == "planning_list_variable" {
                provider_values.push(None);
                if let Some(element_collection) = variable.element_collection.as_ref() {
                    let values = py_solution
                        .getattr(element_collection.as_str())?
                        .cast::<PyList>()?
                        .iter()
                        .map(|item| item.extract::<usize>())
                        .collect::<PyResult<Vec<_>>>()?;
                    list_element_values.push(Some(Arc::<[usize]>::from(values)));
                } else {
                    list_element_values.push(None);
                }
            } else {
                provider_values.push(None);
                list_element_values.push(None);
            }
        }
        let mut rows = Vec::new();
        let mut objects = Vec::new();
        for item in collection.iter() {
            let mut row = DynamicEntityRow::with_variable_count(entity.variables.len());
            for (variable_index, variable) in entity.variables.iter().enumerate() {
                let value = item.getattr(variable.name.as_str())?;
                if variable.kind == "planning_variable" {
                    let scalar = if value.is_none() {
                        None
                    } else {
                        Some(value.extract::<usize>()?)
                    };
                    row.set_scalar_at(variable_index, scalar);
                    if let Some(callback) = variable.candidate_values.as_ref() {
                        let values = callback
                            .bind(py_solution.py())
                            .call1((item.clone(),))?
                            .extract::<Vec<usize>>()?;
                        row.set_candidate_vec_at(variable_index, values);
                    } else if let Some(Some(values)) = provider_values.get(variable_index) {
                        row.set_candidate_arc_at(variable_index, Arc::clone(values));
                    }
                } else if variable.kind == "planning_list_variable" {
                    let values = value
                        .cast::<PyList>()?
                        .iter()
                        .map(|item| item.extract::<usize>())
                        .collect::<PyResult<Vec<_>>>()?;
                    row.set_list_at(variable_index, values);
                } else {
                    row.set_field(variable.name.clone(), DynamicValue::from_python(&value)?);
                }
            }
            import_instance_dict_fields(&item, &variable_names, &mut row)?;
            let extra_field_names = importable_extra_field_names(
                &item,
                &variable_names,
                Some(&shared_field_names),
                Some(&row_metadata_fields),
            )?;
            import_named_fields(&item, &extra_field_names, &mut row)?;
            rows.push(row);
            objects.push(item.unbind());
        }
        entity_tables.push(rows);
        entity_objects.push(objects);
        scalar_value_ranges.push(provider_values);
        list_elements.push(list_element_values);
    }
    validate_list_element_stream_identity(schema, &list_elements)?;
    let mut fact_tables = Vec::new();
    let mut fact_objects = Vec::new();
    for fact in &schema.facts {
        let collection = py_solution.getattr(fact.collection.as_str())?;
        let collection = collection.cast::<PyList>()?;
        let mut rows = Vec::new();
        let mut objects = Vec::new();
        for item in collection.iter() {
            let mut row = DynamicEntityRow::default();
            let skip_names = BTreeSet::new();
            import_instance_dict_fields(&item, &skip_names, &mut row)?;
            let extra_field_names = importable_extra_field_names(&item, &skip_names, None, None)?;
            import_named_fields(&item, &extra_field_names, &mut row)?;
            rows.push(row);
            objects.push(item.unbind());
        }
        fact_tables.push(rows);
        fact_objects.push(objects);
    }
    validate_assignment_group_field_metadata(schema, &entity_tables)?;
    validate_list_element_metadata_field_sources(
        schema,
        &entity_tables,
        &list_elements,
        &solution_fields,
    )?;
    validate_list_metadata_field_sources(schema, &entity_tables, &list_elements, &solution_fields)?;
    let mut solution = PyDynamicSolution::from_runtime_plan(
        runtime_plan,
        DynamicState {
            entities: entity_tables,
            facts: fact_tables,
            scalar_value_ranges,
            list_elements,
            solution_fields: Arc::new(solution_fields),
        },
        PythonCallbackView::from_import(
            py_solution.clone().unbind(),
            entity_objects,
            fact_objects,
            callback_root_fields,
        ),
        None,
        SolverConfig::default(),
        0,
    );
    solution.refresh_all_shadows()?;
    Ok(solution)
}

/// A planning-list element stream is a source-indexed identity space for the
/// canonical core list kernels. Reject duplicate values at import rather than
/// letting a later construction/preview cursor confuse two equal values.
fn validate_list_element_stream_identity(
    schema: &DynamicSchema,
    list_elements: &[Vec<Option<Arc<[usize]>>>],
) -> PyResult<()> {
    for (entity_index, entity) in schema.entities.iter().enumerate() {
        for (variable_index, variable) in entity.variables.iter().enumerate() {
            if variable.kind != "planning_list_variable" {
                continue;
            }
            let Some(Some(elements)) = list_elements
                .get(entity_index)
                .and_then(|variables| variables.get(variable_index))
            else {
                continue;
            };
            let mut first_source_index = BTreeMap::new();
            for (source_index, &element) in elements.iter().enumerate() {
                if let Some(first) = first_source_index.insert(element, source_index) {
                    return Err(crate::error::py_err(format!(
                        "planning list `{}.{}` declares duplicate element value {} at source indexes {} and {}; list elements must be unique",
                        entity.type_name, variable.name, element, first, source_index,
                    )));
                }
            }
        }
    }
    Ok(())
}

fn import_shared_solution_fields(
    py_solution: &Bound<'_, PyAny>,
    schema: &DynamicSchema,
) -> PyResult<BTreeMap<String, DynamicValue>> {
    let mut required_solution_fields = BTreeSet::new();
    let mut element_metadata_contexts = BTreeMap::<String, BTreeSet<String>>::new();
    for entity in &schema.entities {
        for variable in &entity.variables {
            if variable.kind == "planning_list_variable" {
                let target = format!("{}.{}", entity.type_name, variable.name);
                for (source_name, source) in [
                    ("element_owner", variable.element_owner.as_ref()),
                    (
                        "construction_element_order_key",
                        variable.construction_element_order.as_ref(),
                    ),
                    ("precedence_duration", variable.precedence_duration.as_ref()),
                    (
                        "precedence_successors",
                        variable.precedence_successors.as_ref(),
                    ),
                ] {
                    let Some(MetadataSourceSchema::Row(field_name)) = source else {
                        continue;
                    };
                    required_solution_fields.insert(field_name.clone());
                    element_metadata_contexts
                        .entry(field_name.clone())
                        .or_default()
                        .insert(format!("planning list `{target}` {source_name}"));
                }
            }
            if let Some(metadata) = variable.list_metadata.as_ref() {
                collect_list_metadata_solution_fields(metadata, &mut required_solution_fields);
            }
        }
    }

    let mut fields = BTreeMap::new();
    for name in required_solution_fields {
        let element_context = element_metadata_contexts
            .get(&name)
            .map(|contexts| contexts.iter().cloned().collect::<Vec<_>>().join(", "));
        let value = match py_solution.getattr(name.as_str()) {
            Ok(value) => value,
            Err(_) if let Some(context) = element_context.as_deref() => {
                return Err(crate::error::py_err(format!(
                    "declared solution-level sequence `{name}` for {context} is missing"
                )));
            }
            Err(_) => {
                return Err(crate::error::py_err(format!(
                    "declared solution metadata field `{name}` is missing; solution-scoped list metadata never falls back to an entity row"
                )));
            }
        };
        if value.is_callable() {
            if let Some(context) = element_context.as_deref() {
                return Err(crate::error::py_err(format!(
                    "declared solution-level sequence `{name}` for {context} must not be callable"
                )));
            }
            return Err(crate::error::py_err(format!(
                "declared solution metadata field `{name}` must not be callable"
            )));
        }
        fields.insert(name, DynamicValue::from_python(&value)?);
    }
    Ok(fields)
}

fn collect_list_metadata_solution_fields(
    metadata: &ListMetadataSchema,
    field_names: &mut BTreeSet<String>,
) {
    let mut collect_source = |source: &ListMetadataSourceSchema| match source {
        ListMetadataSourceSchema::SolutionField(field) => {
            field_names.insert(field.clone());
        }
        ListMetadataSourceSchema::Capacity(capacity) => {
            collect_list_metadata_capacity_solution_fields(capacity, field_names);
        }
        ListMetadataSourceSchema::Row(_)
        | ListMetadataSourceSchema::EntityCallback(_)
        | ListMetadataSourceSchema::SolutionCallback(_) => {}
    };
    if let Some(route) = metadata.route.as_ref() {
        collect_source(&route.depot);
        collect_source(&route.distance);
        collect_source(&route.feasible);
    }
    if let Some(savings) = metadata.savings.as_ref() {
        collect_source(&savings.depot);
        collect_source(&savings.metric_class);
        collect_source(&savings.distance);
        collect_source(&savings.feasible);
    }
    if let Some(source) = metadata.cross_position_distance.as_ref() {
        collect_source(source);
    }
    if let Some(source) = metadata.intra_position_distance.as_ref() {
        collect_source(source);
    }
}

fn collect_list_metadata_capacity_solution_fields(
    capacity: &crate::schema::types::ListCapacityFeasibilitySchema,
    field_names: &mut BTreeSet<String>,
) {
    for source in [&capacity.capacity, &capacity.demand] {
        if let ListMetadataFieldSourceSchema::SolutionField(field) = source {
            field_names.insert(field.clone());
        }
    }
}

fn import_callback_root_context(
    py_solution: &Bound<'_, PyAny>,
    schema: &DynamicSchema,
) -> PyResult<BTreeMap<String, Py<PyAny>>> {
    let skip_names = callback_root_skip_names(schema);
    let mut fields = BTreeMap::new();
    import_callback_root_dict_fields(py_solution, &skip_names, &mut fields)?;
    for attr in py_solution.dir()?.iter() {
        let name = attr.extract::<String>()?;
        if name.starts_with('_') || skip_names.contains(name.as_str()) {
            continue;
        }
        let Ok(value) = py_solution.getattr(name.as_str()) else {
            continue;
        };
        if value.is_callable() {
            continue;
        }
        fields.insert(name, value.unbind());
    }
    Ok(fields)
}

fn callback_root_skip_names(schema: &DynamicSchema) -> BTreeSet<String> {
    let mut skip_names = BTreeSet::new();
    for entity in &schema.entities {
        skip_names.insert(entity.collection.clone());
    }
    for fact in &schema.facts {
        skip_names.insert(fact.collection.clone());
    }
    skip_names
}

fn import_callback_root_dict_fields(
    py_solution: &Bound<'_, PyAny>,
    skip_names: &BTreeSet<String>,
    fields: &mut BTreeMap<String, Py<PyAny>>,
) -> PyResult<()> {
    let Ok(dict_any) = py_solution.getattr("__dict__") else {
        return Ok(());
    };
    let Ok(dict) = dict_any.cast::<PyDict>() else {
        return Ok(());
    };
    for (key, value) in dict.iter() {
        let name = key.extract::<String>()?;
        if name.starts_with('_') || skip_names.contains(name.as_str()) || value.is_callable() {
            continue;
        }
        fields.insert(name, value.unbind());
    }
    Ok(())
}

fn row_metadata_fields(schema: &DynamicSchema) -> BTreeSet<String> {
    let mut field_names = BTreeSet::new();
    for entity in &schema.entities {
        for variable in &entity.variables {
            for source in [
                &variable.nearby_value_candidates,
                &variable.nearby_entity_candidates,
                &variable.nearby_value_distance_meter,
                &variable.nearby_entity_distance_meter,
            ] {
                if let Some(field_name) = source.as_ref().and_then(MetadataSourceSchema::row) {
                    field_names.insert(field_name.to_string());
                }
            }
            if let Some(field_name) = variable
                .element_owner
                .as_ref()
                .and_then(|source| source.row())
            {
                field_names.insert(field_name.to_string());
            }
            if let Some(field_name) = variable
                .construction_element_order
                .as_ref()
                .and_then(|source| source.row())
            {
                field_names.insert(field_name.to_string());
            }
            if let Some(field_name) = variable
                .precedence_duration
                .as_ref()
                .and_then(|source| source.row())
            {
                field_names.insert(field_name.to_string());
            }
            if let Some(field_name) = variable
                .precedence_successors
                .as_ref()
                .and_then(|source| source.row())
            {
                field_names.insert(field_name.to_string());
            }
            if let Some(metadata) = variable.list_metadata.as_ref() {
                collect_list_metadata_row_fields(metadata, &mut field_names);
            }
        }
    }
    for group in &schema.assignment_scalar_groups {
        for source in [
            group.required_entity.as_ref(),
            group.capacity_key.as_ref(),
            group.position_key.as_ref(),
            group.sequence_key.as_ref(),
        ] {
            if let Some(MetadataSourceSchema::Row(field)) = source {
                field_names.insert(field.clone());
            }
        }
    }
    field_names
}

fn collect_list_metadata_row_fields(
    metadata: &ListMetadataSchema,
    field_names: &mut BTreeSet<String>,
) {
    let mut collect_source = |source: &ListMetadataSourceSchema| match source {
        ListMetadataSourceSchema::Row(field) => {
            field_names.insert(field.clone());
        }
        ListMetadataSourceSchema::Capacity(capacity) => {
            for source in [&capacity.capacity, &capacity.demand] {
                if let ListMetadataFieldSourceSchema::Row(field) = source {
                    field_names.insert(field.clone());
                }
            }
        }
        ListMetadataSourceSchema::SolutionField(_)
        | ListMetadataSourceSchema::EntityCallback(_)
        | ListMetadataSourceSchema::SolutionCallback(_) => {}
    };
    if let Some(route) = metadata.route.as_ref() {
        collect_source(&route.depot);
        collect_source(&route.distance);
        collect_source(&route.feasible);
    }
    if let Some(savings) = metadata.savings.as_ref() {
        collect_source(&savings.depot);
        collect_source(&savings.metric_class);
        collect_source(&savings.distance);
        collect_source(&savings.feasible);
    }
    if let Some(source) = metadata.cross_position_distance.as_ref() {
        collect_source(source);
    }
    if let Some(source) = metadata.intra_position_distance.as_ref() {
        collect_source(source);
    }
}

fn validate_assignment_group_field_metadata(
    schema: &DynamicSchema,
    entity_tables: &[Vec<DynamicEntityRow>],
) -> PyResult<()> {
    for group in &schema.assignment_scalar_groups {
        let required_field = group
            .required_entity
            .as_ref()
            .and_then(MetadataSourceSchema::row);
        let capacity_field = group
            .capacity_key
            .as_ref()
            .and_then(MetadataSourceSchema::row);
        let position_field = group
            .position_key
            .as_ref()
            .and_then(MetadataSourceSchema::row);
        let sequence_field = group
            .sequence_key
            .as_ref()
            .and_then(MetadataSourceSchema::row);
        if required_field.is_none()
            && capacity_field.is_none()
            && position_field.is_none()
            && sequence_field.is_none()
        {
            continue;
        }
        let entity_index = schema
            .entities
            .iter()
            .position(|entity| entity.type_name == group.entity_class)
            .ok_or_else(|| {
                crate::error::py_err(format!(
                    "assignment scalar group `{}` targets unknown entity `{}`",
                    group.name, group.entity_class
                ))
            })?;
        let variable_index = schema.entities[entity_index]
            .variables
            .iter()
            .position(|variable| variable.name == group.variable_name)
            .ok_or_else(|| {
                crate::error::py_err(format!(
                    "assignment scalar group `{}` targets unknown variable `{}.{}`",
                    group.name, group.entity_class, group.variable_name
                ))
            })?;
        let rows = entity_tables.get(entity_index).ok_or_else(|| {
            crate::error::py_err(format!(
                "assignment scalar group `{}` has no imported rows for `{}`",
                group.name, group.entity_class
            ))
        })?;
        for (row_index, row) in rows.iter().enumerate() {
            if let Some(field) = required_field {
                let value = assignment_metadata_field(row, &group.name, row_index, field)?;
                if !matches!(value, DynamicValue::Bool(_)) {
                    return Err(crate::error::py_err(format!(
                        "assignment scalar group `{}` field `{field}` on row {row_index} must be bool",
                        group.name
                    )));
                }
            }
            if let Some(field) = position_field {
                let value = assignment_metadata_field(row, &group.name, row_index, field)?;
                if !matches!(value, DynamicValue::Int(_)) {
                    return Err(crate::error::py_err(format!(
                        "assignment scalar group `{}` field `{field}` on row {row_index} must be an integer",
                        group.name
                    )));
                }
            }
            if let Some(field) = sequence_field {
                let value = assignment_metadata_field(row, &group.name, row_index, field)?;
                if !matches!(value, DynamicValue::Int(value) if *value >= 0) {
                    return Err(crate::error::py_err(format!(
                        "assignment scalar group `{}` field `{field}` on row {row_index} must be a non-negative integer",
                        group.name
                    )));
                }
            }
            if let Some(field) = capacity_field {
                let value = assignment_metadata_field(row, &group.name, row_index, field)?;
                let DynamicValue::List(values) = value else {
                    return Err(crate::error::py_err(format!(
                        "assignment scalar group `{}` field `{field}` on row {row_index} must be a list indexed by candidate value",
                        group.name
                    )));
                };
                if let Some(candidates) = row.candidates_at(variable_index) {
                    for candidate in candidates {
                        let Some(key) = values.get(*candidate) else {
                            return Err(crate::error::py_err(format!(
                                "assignment scalar group `{}` field `{field}` on row {row_index} has no entry for candidate {candidate}",
                                group.name
                            )));
                        };
                        if !matches!(key, DynamicValue::None)
                            && !matches!(key, DynamicValue::Int(value) if *value >= 0)
                        {
                            return Err(crate::error::py_err(format!(
                                "assignment scalar group `{}` field `{field}` on row {row_index} must contain non-negative integers or None",
                                group.name
                            )));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validates declared solution-level element metadata before a dynamic state is
/// constructed. These sources are structural: a missing or malformed sequence
/// must not turn into an unrestricted owner, a default order, or absent
/// precedence. Callback sources remain lazy and are never invoked here.
fn validate_list_element_metadata_field_sources(
    schema: &DynamicSchema,
    entity_tables: &[Vec<DynamicEntityRow>],
    list_elements: &[Vec<Option<Arc<[usize]>>>],
    solution_fields: &BTreeMap<String, DynamicValue>,
) -> PyResult<()> {
    for (entity_index, entity) in schema.entities.iter().enumerate() {
        let rows = entity_tables.get(entity_index).ok_or_else(|| {
            crate::error::py_err(format!(
                "planning list metadata for `{}` has no imported entity rows",
                entity.type_name
            ))
        })?;
        for (variable_index, variable) in entity.variables.iter().enumerate() {
            if variable.kind != "planning_list_variable" {
                continue;
            }
            let target = format!("{}.{}", entity.type_name, variable.name);
            let elements =
                declared_list_elements(rows, list_elements, entity_index, variable_index);
            validate_list_element_metadata_sequence(
                variable.element_owner.as_ref(),
                solution_fields,
                &target,
                "element_owner",
                &elements,
                "None or a non-negative integer",
                |value| {
                    matches!(value, DynamicValue::None)
                        || matches!(value, DynamicValue::Int(value) if *value >= 0)
                },
            )?;
            validate_list_element_metadata_sequence(
                variable.construction_element_order.as_ref(),
                solution_fields,
                &target,
                "construction_element_order_key",
                &elements,
                "an integer",
                |value| matches!(value, DynamicValue::Int(_)),
            )?;
            validate_list_element_metadata_sequence(
                variable.precedence_duration.as_ref(),
                solution_fields,
                &target,
                "precedence_duration",
                &elements,
                "a non-negative integer",
                |value| matches!(value, DynamicValue::Int(value) if *value >= 0),
            )?;
            validate_list_element_metadata_sequence(
                variable.precedence_successors.as_ref(),
                solution_fields,
                &target,
                "precedence_successors",
                &elements,
                "a sequence of non-negative integers",
                |value| {
                    matches!(
                        value,
                        DynamicValue::List(values)
                            if values.iter().all(
                                |value| matches!(value, DynamicValue::Int(value) if *value >= 0)
                            )
                    )
                },
            )?;
        }
    }
    Ok(())
}

fn validate_list_element_metadata_sequence(
    source: Option<&MetadataSourceSchema>,
    solution_fields: &BTreeMap<String, DynamicValue>,
    target: &str,
    source_name: &str,
    elements: &BTreeSet<usize>,
    expected_entry: &str,
    validate_entry: impl Fn(&DynamicValue) -> bool,
) -> PyResult<()> {
    let Some(MetadataSourceSchema::Row(field)) = source else {
        return Ok(());
    };
    let value = solution_fields.get(field).ok_or_else(|| {
        crate::error::py_err(format!(
            "planning list `{target}` {source_name} solution-level sequence `{field}` is missing"
        ))
    })?;
    let DynamicValue::List(values) = value else {
        return Err(crate::error::py_err(format!(
            "planning list `{target}` {source_name} solution-level sequence `{field}` must be a sequence indexed by element value"
        )));
    };
    for element in elements {
        let entry = values.get(*element).ok_or_else(|| {
            crate::error::py_err(format!(
                "planning list `{target}` {source_name} solution-level sequence `{field}` has no entry for element {element}"
            ))
        })?;
        if !validate_entry(entry) {
            return Err(crate::error::py_err(format!(
                "planning list `{target}` {source_name} solution-level sequence `{field}` entry for element {element} must be {expected_entry}"
            )));
        }
    }
    Ok(())
}

/// Validates every declared field-backed list metadata source while importing
/// a Python solution. Row and solution sources are resolved only from their
/// declared containers; callbacks remain lazy and are never called here.
fn validate_list_metadata_field_sources(
    schema: &DynamicSchema,
    entity_tables: &[Vec<DynamicEntityRow>],
    list_elements: &[Vec<Option<Arc<[usize]>>>],
    solution_fields: &BTreeMap<String, DynamicValue>,
) -> PyResult<()> {
    for (entity_index, entity) in schema.entities.iter().enumerate() {
        let rows = entity_tables.get(entity_index).ok_or_else(|| {
            crate::error::py_err(format!(
                "planning list metadata for `{}` has no imported entity rows",
                entity.type_name
            ))
        })?;
        for (variable_index, variable) in entity.variables.iter().enumerate() {
            if variable.kind != "planning_list_variable" {
                continue;
            }
            let target = format!("{}.{}", entity.type_name, variable.name);
            let metadata = variable.list_metadata.as_ref().ok_or_else(|| {
                crate::error::py_err(format!(
                    "planning list `{target}` is missing canonical list metadata"
                ))
            })?;
            let declared_elements =
                declared_list_elements(rows, list_elements, entity_index, variable_index);
            for (row_index, row) in rows.iter().enumerate() {
                if let Some(route) = metadata.route.as_ref() {
                    let depot = validate_list_metadata_usize_source(
                        &route.depot,
                        row,
                        solution_fields,
                        &target,
                        row_index,
                        "route.depot",
                    )?;
                    let mut required_indexes = declared_elements.clone();
                    if let Some(depot) = depot {
                        required_indexes.insert(depot);
                    }
                    validate_list_metadata_distance_source(
                        &route.distance,
                        row,
                        solution_fields,
                        &target,
                        row_index,
                        "route.distance",
                        &required_indexes,
                    )?;
                    validate_list_metadata_feasibility_source(
                        &route.feasible,
                        row,
                        solution_fields,
                        &target,
                        row_index,
                        "route.feasible",
                        &declared_elements,
                    )?;
                }
                if let Some(savings) = metadata.savings.as_ref() {
                    let depot = validate_list_metadata_usize_source(
                        &savings.depot,
                        row,
                        solution_fields,
                        &target,
                        row_index,
                        "savings.depot",
                    )?;
                    let _ = validate_list_metadata_usize_source(
                        &savings.metric_class,
                        row,
                        solution_fields,
                        &target,
                        row_index,
                        "savings.metric_class",
                    )?;
                    let mut required_indexes = declared_elements.clone();
                    if let Some(depot) = depot {
                        required_indexes.insert(depot);
                    }
                    validate_list_metadata_distance_source(
                        &savings.distance,
                        row,
                        solution_fields,
                        &target,
                        row_index,
                        "savings.distance",
                        &required_indexes,
                    )?;
                    validate_list_metadata_feasibility_source(
                        &savings.feasible,
                        row,
                        solution_fields,
                        &target,
                        row_index,
                        "savings.feasible",
                        &declared_elements,
                    )?;
                }
                for (source_name, source) in [
                    (
                        "cross_position_distance",
                        metadata.cross_position_distance.as_ref(),
                    ),
                    (
                        "intra_position_distance",
                        metadata.intra_position_distance.as_ref(),
                    ),
                ] {
                    if let Some(source) = source {
                        validate_list_metadata_distance_source(
                            source,
                            row,
                            solution_fields,
                            &target,
                            row_index,
                            source_name,
                            &declared_elements,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn declared_list_elements(
    rows: &[DynamicEntityRow],
    list_elements: &[Vec<Option<Arc<[usize]>>>],
    entity_index: usize,
    variable_index: usize,
) -> BTreeSet<usize> {
    let mut elements = BTreeSet::new();
    if let Some(Some(values)) = list_elements
        .get(entity_index)
        .and_then(|variables| variables.get(variable_index))
    {
        elements.extend(values.iter().copied());
    }
    for row in rows {
        if let Some(values) = row.list_at(variable_index) {
            elements.extend(values.iter().copied());
        }
    }
    elements
}

fn list_metadata_source_value<'a>(
    source: &ListMetadataSourceSchema,
    row: &'a DynamicEntityRow,
    solution_fields: &'a BTreeMap<String, DynamicValue>,
    target: &str,
    row_index: usize,
    source_name: &str,
) -> PyResult<Option<&'a DynamicValue>> {
    match source {
        ListMetadataSourceSchema::Row(field) => row.fields.get(field).map(Some).ok_or_else(|| {
            crate::error::py_err(format!(
                "planning list `{target}` {source_name} row field `{field}` is missing on row {row_index}; row-scoped metadata never falls back to a solution field"
            ))
        }),
        ListMetadataSourceSchema::SolutionField(field) => {
            solution_fields.get(field).map(Some).ok_or_else(|| {
                crate::error::py_err(format!(
                    "planning list `{target}` {source_name} solution field `{field}` is missing; solution-scoped metadata never falls back to an entity row"
                ))
            })
        }
        ListMetadataSourceSchema::EntityCallback(_)
        | ListMetadataSourceSchema::SolutionCallback(_)
        | ListMetadataSourceSchema::Capacity(_) => Ok(None),
    }
}

fn list_metadata_field_source_value<'a>(
    source: &ListMetadataFieldSourceSchema,
    row: &'a DynamicEntityRow,
    solution_fields: &'a BTreeMap<String, DynamicValue>,
    target: &str,
    row_index: usize,
    source_name: &str,
) -> PyResult<&'a DynamicValue> {
    match source {
        ListMetadataFieldSourceSchema::Row(field) => row.fields.get(field).ok_or_else(|| {
            crate::error::py_err(format!(
                "planning list `{target}` {source_name} row field `{field}` is missing on row {row_index}; row-scoped metadata never falls back to a solution field"
            ))
        }),
        ListMetadataFieldSourceSchema::SolutionField(field) => {
            solution_fields.get(field).ok_or_else(|| {
                crate::error::py_err(format!(
                    "planning list `{target}` {source_name} solution field `{field}` is missing; solution-scoped metadata never falls back to an entity row"
                ))
            })
        }
    }
}

fn validate_list_metadata_usize_source(
    source: &ListMetadataSourceSchema,
    row: &DynamicEntityRow,
    solution_fields: &BTreeMap<String, DynamicValue>,
    target: &str,
    row_index: usize,
    source_name: &str,
) -> PyResult<Option<usize>> {
    let Some(value) =
        list_metadata_source_value(source, row, solution_fields, target, row_index, source_name)?
    else {
        return Ok(None);
    };
    match value {
        DynamicValue::Int(value) if *value >= 0 => Ok(Some(*value as usize)),
        _ => Err(crate::error::py_err(format!(
            "planning list `{target}` {source_name} on row {row_index} must be a non-negative integer"
        ))),
    }
}

fn validate_list_metadata_distance_source(
    source: &ListMetadataSourceSchema,
    row: &DynamicEntityRow,
    solution_fields: &BTreeMap<String, DynamicValue>,
    target: &str,
    row_index: usize,
    source_name: &str,
    required_indexes: &BTreeSet<usize>,
) -> PyResult<()> {
    let Some(matrix) =
        list_metadata_source_value(source, row, solution_fields, target, row_index, source_name)?
    else {
        return Ok(());
    };
    let DynamicValue::List(rows) = matrix else {
        return Err(crate::error::py_err(format!(
            "planning list `{target}` {source_name} on row {row_index} must be a square integer matrix"
        )));
    };
    for from in required_indexes {
        let Some(DynamicValue::List(columns)) = rows.get(*from) else {
            return Err(crate::error::py_err(format!(
                "planning list `{target}` {source_name} on row {row_index} has no matrix row for value {from}"
            )));
        };
        for to in required_indexes {
            if !matches!(columns.get(*to), Some(DynamicValue::Int(_))) {
                return Err(crate::error::py_err(format!(
                    "planning list `{target}` {source_name} on row {row_index} has no integer distance from {from} to {to}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_list_metadata_feasibility_source(
    source: &ListMetadataSourceSchema,
    row: &DynamicEntityRow,
    solution_fields: &BTreeMap<String, DynamicValue>,
    target: &str,
    row_index: usize,
    source_name: &str,
    elements: &BTreeSet<usize>,
) -> PyResult<()> {
    let ListMetadataSourceSchema::Capacity(capacity) = source else {
        return Ok(());
    };
    let capacity_value = list_metadata_field_source_value(
        &capacity.capacity,
        row,
        solution_fields,
        target,
        row_index,
        &format!("{source_name}.capacity"),
    )?;
    if !matches!(capacity_value, DynamicValue::Int(value) if *value >= 0) {
        return Err(crate::error::py_err(format!(
            "planning list `{target}` {source_name}.capacity on row {row_index} must be a non-negative integer"
        )));
    }
    let demands = list_metadata_field_source_value(
        &capacity.demand,
        row,
        solution_fields,
        target,
        row_index,
        &format!("{source_name}.demand"),
    )?;
    let DynamicValue::List(values) = demands else {
        return Err(crate::error::py_err(format!(
            "planning list `{target}` {source_name}.demand on row {row_index} must be a list indexed by element value"
        )));
    };
    for element in elements {
        if !matches!(values.get(*element), Some(DynamicValue::Int(_))) {
            return Err(crate::error::py_err(format!(
                "planning list `{target}` {source_name}.demand on row {row_index} has no integer demand for element {element}"
            )));
        }
    }
    Ok(())
}

fn assignment_metadata_field<'a>(
    row: &'a DynamicEntityRow,
    group_name: &str,
    row_index: usize,
    field: &str,
) -> PyResult<&'a DynamicValue> {
    row.fields.get(field).ok_or_else(|| {
        crate::error::py_err(format!(
            "assignment scalar group `{group_name}` field `{field}` is missing on row {row_index}"
        ))
    })
}

fn import_instance_dict_fields(
    item: &Bound<'_, PyAny>,
    skip_names: &BTreeSet<&str>,
    row: &mut DynamicEntityRow,
) -> PyResult<()> {
    let Ok(dict_any) = item.getattr("__dict__") else {
        return Ok(());
    };
    let Ok(dict) = dict_any.cast::<PyDict>() else {
        return Ok(());
    };
    let default_attribute_access = has_default_attribute_access(item)?;
    for (key, value) in dict.iter() {
        let name = key.extract::<String>()?;
        if name.starts_with('_') || skip_names.contains(name.as_str()) || value.is_callable() {
            continue;
        }
        if has_data_descriptor(item, name.as_str())? {
            continue;
        }
        row.instance_fields.insert(name.clone());
        if default_attribute_access && supports_native_equality(&value) {
            row.native_equality_fields.insert(name.clone());
        }
        row.set_field(name, DynamicValue::from_python(&value)?);
    }
    Ok(())
}

fn has_default_attribute_access(item: &Bound<'_, PyAny>) -> PyResult<bool> {
    let item_getattribute = item.get_type().getattr("__getattribute__")?;
    let object_getattribute = item
        .py()
        .import("builtins")?
        .getattr("object")?
        .getattr("__getattribute__")?;
    Ok(item_getattribute.is(&object_getattribute))
}

fn supports_native_equality(value: &Bound<'_, PyAny>) -> bool {
    value.is_none()
        || value.get_type().is(value.py().get_type::<PyInt>())
        || value.get_type().is(value.py().get_type::<PyString>())
}

fn instance_dict_field_names(item: &Bound<'_, PyAny>) -> PyResult<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    let Ok(dict_any) = item.getattr("__dict__") else {
        return Ok(names);
    };
    let Ok(dict) = dict_any.cast::<PyDict>() else {
        return Ok(names);
    };
    for key in dict.keys().iter() {
        let name = key.extract::<String>()?;
        if !has_data_descriptor(item, name.as_str())? {
            names.insert(name);
        }
    }
    Ok(names)
}

fn has_data_descriptor(item: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    let Ok(descriptor) = item.get_type().getattr(name) else {
        return Ok(false);
    };
    Ok(descriptor.hasattr("__get__")?
        && (descriptor.hasattr("__set__")? || descriptor.hasattr("__delete__")?))
}

fn importable_extra_field_names(
    item: &Bound<'_, PyAny>,
    skip_names: &BTreeSet<&str>,
    shared_field_names: Option<&BTreeSet<String>>,
    row_metadata_fields: Option<&BTreeSet<String>>,
) -> PyResult<Vec<String>> {
    let instance_names = instance_dict_field_names(item)?;
    let mut names = Vec::new();
    for attr in item.dir()?.iter() {
        let name = attr.extract::<String>()?;
        if name.starts_with('_')
            || skip_names.contains(name.as_str())
            || instance_names.contains(name.as_str())
            || shared_field_names.is_some_and(|shared| {
                shared.contains(name.as_str())
                    && !row_metadata_fields.is_some_and(|row| row.contains(name.as_str()))
            })
        {
            continue;
        }
        let Ok(value) = item.getattr(name.as_str()) else {
            continue;
        };
        if value.is_callable() {
            continue;
        }
        names.push(name);
    }
    Ok(names)
}

fn import_named_fields(
    item: &Bound<'_, PyAny>,
    names: &[String],
    row: &mut DynamicEntityRow,
) -> PyResult<()> {
    for name in names {
        let Ok(value) = item.getattr(name.as_str()) else {
            continue;
        };
        if value.is_callable() {
            continue;
        }
        row.set_field(name.clone(), DynamicValue::from_python(&value)?);
    }
    Ok(())
}

fn is_read_only_property(
    item: &Bound<'_, PyAny>,
    name: &str,
    property_type: &Bound<'_, PyAny>,
) -> PyResult<bool> {
    let Ok(class_attr) = item.get_type().getattr(name) else {
        return Ok(false);
    };
    if !class_attr.is_instance(property_type)? {
        return Ok(false);
    }
    Ok(class_attr.getattr("fset")?.is_none())
}

pub fn export_solution(
    py_solution: &Bound<'_, PyAny>,
    solution: &PyDynamicSolution,
) -> PyResult<Py<PyAny>> {
    let builtins = py_solution.py().import("builtins")?;
    let property_type = builtins.getattr("property")?;
    for (entity_index, entity) in solution.schema().entities.iter().enumerate() {
        let collection = py_solution.getattr(entity.collection.as_str())?;
        let collection = collection.cast::<PyList>()?;
        for (row_index, item) in collection.iter().enumerate() {
            let row = &solution.state.entities[entity_index][row_index];
            for (variable_index, variable) in entity.variables.iter().enumerate() {
                if variable.kind == "planning_variable" {
                    match row.scalar_at(variable_index) {
                        Some(value) => item.setattr(variable.name.as_str(), value)?,
                        None => item.setattr(variable.name.as_str(), py_solution.py().None())?,
                    }
                } else if variable.kind == "planning_list_variable" {
                    if let Some(values) = row.list_at(variable_index) {
                        item.setattr(variable.name.as_str(), values.to_vec())?;
                    }
                }
            }
            for name in &row.shadow_fields {
                let Some(value) = row.fields.get(name) else {
                    continue;
                };
                if is_read_only_property(&item, name.as_str(), &property_type)? {
                    continue;
                }
                let value = value.to_python(py_solution.py())?;
                item.setattr(name.as_str(), value.bind(py_solution.py()))?;
            }
        }
    }
    Ok(py_solution.clone().unbind())
}
