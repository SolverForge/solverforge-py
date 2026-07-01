use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::{self, Debug};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use solverforge_bridge::{
    DynamicListVariableSlot, DynamicModelBackend, DynamicScalarVariableSlot, EntityClassId,
    VariableId,
};
use solverforge_config::{
    AcceptedCountForagerConfig, AcceptorConfig, ConstructionHeuristicConfig,
    ConstructionHeuristicType, ConstructionObligation, DiversifiedLateAcceptanceConfig,
    ForagerConfig, LateAcceptanceConfig, LocalSearchConfig, LocalSearchType, MoveSelectorConfig,
    PhaseConfig, SimulatedAnnealingConfig, SolverConfig, VariableTargetConfig,
};
use solverforge_core::domain::{PlanningSolution, SolutionDescriptor};
use solverforge_scoring::{ConstraintMetadata, ConstraintSet, Director, DirectorScoreState};
use solverforge_solver::builder::selector::{GroupedScalarCursor, GroupedScalarSelector};
use solverforge_solver::builder::{
    ScalarAssignmentBinding, ScalarGroupBinding, ScalarGroupBindingKind, ScalarGroupLimits,
    ScalarGroupMemberBinding, ValueSource,
};
use solverforge_solver::heuristic::r#move::{
    CompoundScalarMove, KOptMove, ListChangeMove, ListMoveUnion, ListMultiSwapMove,
    ListPermuteMove, ListReverseMove, ListRuinMove, ListSwapMove, MoveTabuSignature,
    PillarChangeMove, PillarSwapMove, RuinRecreateMove, ScalarMoveUnion, ScalarRecreateValueSource,
    SublistChangeMove, SublistSwapMove, SwapMove,
};
use solverforge_solver::heuristic::selector::move_selector::{
    ArenaMoveCursor, CandidateId, CandidateStore, MoveCandidateRef, MoveCursor, MoveSelector,
    MoveStreamContext,
};
use solverforge_solver::heuristic::MoveArena;
use solverforge_solver::heuristic::{
    ListChangeMoveSelector, ListPermuteMoveSelector, ListPrecedenceMoveSelector,
    ListReverseMoveSelector, ListSwapMoveSelector, NearbyListChangeMoveSelector,
    NearbyListSwapMoveSelector, SublistChangeMoveSelector, SublistSwapMoveSelector,
};
use solverforge_solver::scope::ProgressCallback;
use solverforge_solver::{
    AcceptorBuilder, AnyAcceptor, AnyForager, BestFitForager, ConstructionHeuristicPhase,
    EntityPlacer, EntityReference, FirstFitForager, FirstLastStepScoreImprovingForager,
    ForagerBuilder, FromSolutionEntitySelector, IntraDistanceAdapter, KOptConfig, KOptMoveSelector,
    ListCheapestInsertionPhase, ListClarkeWrightPhase, ListKOptPhase, ListRegretInsertionPhase,
    ListRuinMoveSelector, ListVariableSlot, LocalSearchPhase, LocalSearchStrategy, Move,
    NearbyKOptMoveSelector, Phase, PhaseSequence, Placement, RuntimeModel,
    ScalarAssignmentMoveOptions, ScalarAssignmentRequiredStreamingCursor,
};

use crate::constraints::PyDynamicConstraintSet;
use crate::intern::intern;
use crate::runtime::distance::PyDistanceMeter;
use crate::schema::{DynamicSchema, VariableSchema};
use crate::score::DynamicScore;
use crate::state::entity_table::DynamicEntityRow;
use crate::state::PyDynamicSolution;
use crate::value::DynamicValue;

type UpstreamRuntimePhase = solverforge_solver::runtime::RuntimePhase<
    solverforge_solver::runtime::Construction<
        PyDynamicSolution,
        usize,
        PyDistanceMeter,
        PyDistanceMeter,
    >,
    LocalSearchStrategy<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
>;

type DynamicLocalSearch = LocalSearchPhase<
    PyDynamicSolution,
    DynamicMove,
    DynamicMoveSelector,
    AnyAcceptor<PyDynamicSolution>,
    AnyForager<PyDynamicSolution>,
>;

const DEFAULT_LOCAL_SEARCH_LATE_ACCEPTANCE_SIZE: usize = 400;
const DEFAULT_LOCAL_SEARCH_ACCEPTED_COUNT: usize = 256;
const DEFAULT_SIMULATED_ANNEALING_DECAY_RATE: f64 = 0.999985;

pub(crate) enum PyDynamicRuntimePhase {
    Upstream(PhaseSequence<UpstreamRuntimePhase>),
    DynamicScalarConstruction(DynamicScalarConstructionPhase),
    DynamicListCheapestInsertion {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        phase: ListCheapestInsertionPhase<PyDynamicSolution, usize>,
    },
    DynamicListRegretInsertion {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        phase: ListRegretInsertionPhase<PyDynamicSolution, usize>,
    },
    DynamicListClarkeWright {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        phase: ListClarkeWrightPhase<PyDynamicSolution, usize>,
    },
    DynamicListKOpt {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        phase: ListKOptPhase<PyDynamicSolution, usize>,
    },
    DynamicAssignmentConstruction(DynamicAssignmentConstructionPhase),
    DynamicLocalSearch(Box<DynamicLocalSearch>),
}

impl Debug for PyDynamicRuntimePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Upstream(phase) => f
                .debug_tuple("PyDynamicRuntimePhase::Upstream")
                .field(phase)
                .finish(),
            Self::DynamicScalarConstruction(phase) => f
                .debug_tuple("PyDynamicRuntimePhase::DynamicScalarConstruction")
                .field(phase)
                .finish(),
            Self::DynamicListCheapestInsertion { phase, .. } => f
                .debug_tuple("PyDynamicRuntimePhase::DynamicListCheapestInsertion")
                .field(phase)
                .finish(),
            Self::DynamicListRegretInsertion { phase, .. } => f
                .debug_tuple("PyDynamicRuntimePhase::DynamicListRegretInsertion")
                .field(phase)
                .finish(),
            Self::DynamicListClarkeWright { phase, .. } => f
                .debug_tuple("PyDynamicRuntimePhase::DynamicListClarkeWright")
                .field(phase)
                .finish(),
            Self::DynamicListKOpt { phase, .. } => f
                .debug_tuple("PyDynamicRuntimePhase::DynamicListKOpt")
                .field(phase)
                .finish(),
            Self::DynamicAssignmentConstruction(phase) => f
                .debug_tuple("PyDynamicRuntimePhase::DynamicAssignmentConstruction")
                .field(phase)
                .finish(),
            Self::DynamicLocalSearch(phase) => f
                .debug_tuple("PyDynamicRuntimePhase::DynamicLocalSearch")
                .field(phase)
                .finish(),
        }
    }
}

impl<D, ProgressCb> Phase<PyDynamicSolution, D, ProgressCb> for PyDynamicRuntimePhase
where
    D: Director<PyDynamicSolution>,
    ProgressCb: ProgressCallback<PyDynamicSolution>,
{
    fn solve(
        &mut self,
        solver_scope: &mut solverforge_solver::SolverScope<'_, PyDynamicSolution, D, ProgressCb>,
    ) {
        match self {
            Self::Upstream(phase) => phase.solve(solver_scope),
            Self::DynamicScalarConstruction(phase) => phase.solve(solver_scope),
            Self::DynamicListCheapestInsertion { slot, phase } => {
                with_dynamic_list_slot(slot, || phase.solve(solver_scope));
                publish_current_list_solution_if_not_worse(solver_scope);
            }
            Self::DynamicListRegretInsertion { slot, phase } => {
                with_dynamic_list_slot(slot, || phase.solve(solver_scope));
                publish_current_list_solution_if_not_worse(solver_scope);
            }
            Self::DynamicListClarkeWright { slot, phase } => {
                with_dynamic_list_slot(slot, || phase.solve(solver_scope));
                publish_current_list_solution_if_not_worse(solver_scope);
            }
            Self::DynamicListKOpt { slot, phase } => {
                with_dynamic_list_slot(slot, || phase.solve(solver_scope));
                publish_current_list_solution_if_not_worse(solver_scope);
            }
            Self::DynamicAssignmentConstruction(phase) => phase.solve(solver_scope),
            Self::DynamicLocalSearch(phase) => phase.solve(solver_scope),
        }
    }

    fn phase_type_name(&self) -> &'static str {
        "PyDynamicRuntimePhase"
    }
}

pub(crate) fn build_dynamic_phases(
    config: &SolverConfig,
    descriptor: &solverforge_core::domain::SolutionDescriptor,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    schema: &DynamicSchema,
) -> PhaseSequence<PyDynamicRuntimePhase> {
    if config.phases.is_empty() {
        return PhaseSequence::new(vec![PyDynamicRuntimePhase::Upstream(
            solverforge_solver::runtime::build_phases(config, descriptor, model),
        )]);
    }

    let mut phases = Vec::new();
    for phase in &config.phases {
        match phase {
            PhaseConfig::ConstructionHeuristic(construction)
                if can_bind_dynamic_assignment_construction(construction, model) =>
            {
                phases.push(PyDynamicRuntimePhase::DynamicAssignmentConstruction(
                    build_dynamic_assignment_construction(construction, model),
                ));
            }
            PhaseConfig::ConstructionHeuristic(construction)
                if can_bind_dynamic_scalar_construction(construction, model) =>
            {
                phases.push(PyDynamicRuntimePhase::DynamicScalarConstruction(
                    build_dynamic_scalar_construction(construction, model),
                ));
            }
            PhaseConfig::ConstructionHeuristic(construction)
                if can_bind_dynamic_list_construction(construction, model, schema) =>
            {
                phases.extend(build_dynamic_list_construction(construction, model, schema));
            }
            PhaseConfig::LocalSearch(local_search) if can_bind_dynamic(local_search, model) => {
                phases.push(PyDynamicRuntimePhase::DynamicLocalSearch(Box::new(
                    build_dynamic_local_search(local_search, model, config.random_seed),
                )));
            }
            _ => {
                let mut single = config.clone();
                single.phases = vec![phase.clone()];
                phases.push(PyDynamicRuntimePhase::Upstream(
                    solverforge_solver::runtime::build_phases(&single, descriptor, model),
                ));
            }
        }
    }
    PhaseSequence::new(phases)
}

pub(crate) fn validate_dynamic_runtime_bindings(
    config: &SolverConfig,
    schema: &DynamicSchema,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
) -> PyResult<()> {
    let scalar_slots = all_dynamic_scalar_slots(model);
    for phase in &config.phases {
        if let PhaseConfig::ConstructionHeuristic(construction) = phase {
            if matches!(
                construction.construction_heuristic_type,
                ConstructionHeuristicType::FirstFit | ConstructionHeuristicType::CheapestInsertion
            ) {
                if let Some(group_name) = construction.group_name.as_deref() {
                    validate_dynamic_assignment_group(schema, group_name, &scalar_slots)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_dynamic_assignment_group(
    schema: &DynamicSchema,
    group_name: &str,
    scalar_slots: &[DynamicScalarVariableSlot<PyDynamicSolution>],
) -> PyResult<()> {
    let group = schema.assignment_scalar_group(group_name).ok_or_else(|| {
        crate::error::py_err(format!(
            "dynamic assignment construction configured for `{group_name}`, but no matching assignment scalar group was declared"
        ))
    })?;
    let slot = scalar_slots
        .iter()
        .find(|slot| {
            slot.matches_target(
                Some(group.entity_class.as_str()),
                Some(group.variable_name.as_str()),
            )
        })
        .ok_or_else(|| {
            crate::error::py_err(format!(
                "assignment scalar group `{}` targets unknown scalar variable `{}.{}`",
                group.name, group.entity_class, group.variable_name
            ))
        })?;
    if !slot.allows_unassigned {
        return Err(crate::error::py_err(format!(
            "assignment scalar group `{}` target `{}.{}` must allow unassigned values",
            group.name, group.entity_class, group.variable_name
        )));
    }
    Ok(())
}

fn can_bind_dynamic_list_construction(
    config: &ConstructionHeuristicConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    schema: &DynamicSchema,
) -> bool {
    match config.construction_heuristic_type {
        ConstructionHeuristicType::ListCheapestInsertion
        | ConstructionHeuristicType::ListRegretInsertion => {
            !matching_list_slots(model, &config.target).is_empty()
        }
        ConstructionHeuristicType::ListClarkeWright | ConstructionHeuristicType::ListKOpt => {
            !matching_route_list_slots(model, &config.target, schema).is_empty()
        }
        _ => false,
    }
}

fn build_dynamic_list_construction(
    config: &ConstructionHeuristicConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    schema: &DynamicSchema,
) -> Vec<PyDynamicRuntimePhase> {
    match config.construction_heuristic_type {
        ConstructionHeuristicType::ListCheapestInsertion
        | ConstructionHeuristicType::ListRegretInsertion => {
            matching_list_slots(model, &config.target)
                .into_iter()
                .map(|slot| match config.construction_heuristic_type {
                    ConstructionHeuristicType::ListCheapestInsertion => {
                        PyDynamicRuntimePhase::DynamicListCheapestInsertion {
                            slot: slot.clone(),
                            phase: ListCheapestInsertionPhase::new(
                                dynamic_list_element_count,
                                dynamic_list_assigned_elements,
                                dynamic_list_entity_count,
                                dynamic_list_len,
                                dynamic_list_insert,
                                dynamic_list_construction_remove,
                                dynamic_list_index_to_element,
                                slot.descriptor_index(),
                            )
                            .with_element_owner_fn(dynamic_element_owner_for_slot(&slot, schema))
                            .with_element_order_key(dynamic_construction_element_order_for_slot(
                                &slot, schema,
                            ))
                            .with_precedence_hooks(
                                dynamic_precedence_duration_for_slot(&slot, schema),
                                dynamic_precedence_successors_for_slot(&slot, schema),
                            ),
                        }
                    }
                    ConstructionHeuristicType::ListRegretInsertion => {
                        PyDynamicRuntimePhase::DynamicListRegretInsertion {
                            slot: slot.clone(),
                            phase: ListRegretInsertionPhase::new(
                                dynamic_list_element_count,
                                dynamic_list_assigned_elements,
                                dynamic_list_entity_count,
                                dynamic_list_len,
                                dynamic_list_insert,
                                dynamic_list_construction_remove,
                                dynamic_list_index_to_element,
                                slot.descriptor_index(),
                            )
                            .with_element_owner_fn(dynamic_element_owner_for_slot(&slot, schema))
                            .with_element_order_key(dynamic_construction_element_order_for_slot(
                                &slot, schema,
                            ))
                            .with_precedence_hooks(
                                dynamic_precedence_duration_for_slot(&slot, schema),
                                dynamic_precedence_successors_for_slot(&slot, schema),
                            ),
                        }
                    }
                    _ => unreachable!("list construction branch checked"),
                })
                .collect()
        }
        _ => matching_route_list_slots(model, &config.target, schema)
            .into_iter()
            .filter_map(|slot| match config.construction_heuristic_type {
                ConstructionHeuristicType::ListClarkeWright => {
                    Some(PyDynamicRuntimePhase::DynamicListClarkeWright {
                        slot: slot.clone(),
                        phase: ListClarkeWrightPhase::new(
                            dynamic_list_element_count,
                            dynamic_list_assigned_elements,
                            dynamic_list_entity_count,
                            dynamic_list_len,
                            dynamic_route_set,
                            dynamic_list_index_to_element,
                            dynamic_route_depot,
                            dynamic_route_distance,
                            dynamic_route_feasible,
                            slot.descriptor_index(),
                        )
                        .with_element_owner_fn(dynamic_element_owner_for_slot(&slot, schema))
                        .with_metric_class_fn(dynamic_route_metric_class),
                    })
                }
                ConstructionHeuristicType::ListKOpt => {
                    Some(PyDynamicRuntimePhase::DynamicListKOpt {
                        slot: slot.clone(),
                        phase: ListKOptPhase::<PyDynamicSolution, usize>::new(
                            config.k,
                            dynamic_list_entity_count,
                            dynamic_route_get,
                            dynamic_route_set,
                            dynamic_route_depot,
                            dynamic_route_distance,
                            Some(dynamic_route_feasible),
                            slot.descriptor_index(),
                        ),
                    })
                }
                _ => None,
            })
            .collect(),
    }
}

fn can_bind_dynamic_scalar_construction(
    config: &ConstructionHeuristicConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
) -> bool {
    matches!(
        config.construction_heuristic_type,
        ConstructionHeuristicType::FirstFit | ConstructionHeuristicType::CheapestInsertion
    ) && !matching_slots(model, &config.target).is_empty()
        && !model.dynamic_list_variables().any(|slot| {
            slot.matches_target(
                config.target.entity_class.as_deref(),
                config.target.variable_name.as_deref(),
            )
        })
}

fn can_bind_dynamic_assignment_construction(
    config: &ConstructionHeuristicConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
) -> bool {
    matches!(
        config.construction_heuristic_type,
        ConstructionHeuristicType::FirstFit | ConstructionHeuristicType::CheapestInsertion
    ) && config.group_name.is_some()
        && model.dynamic_scalar_variables().next().is_some()
}

fn build_dynamic_scalar_construction(
    config: &ConstructionHeuristicConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
) -> DynamicScalarConstructionPhase {
    let placer = DynamicScalarEntityPlacer {
        slots: matching_slots(model, &config.target),
        value_candidate_limit: config.value_candidate_limit,
    };
    match config.construction_heuristic_type {
        ConstructionHeuristicType::FirstFit => DynamicScalarConstructionPhase::FirstFit(
            ConstructionHeuristicPhase::new(placer, FirstFitForager::new())
                .with_live_placement_refresh()
                .with_construction_obligation(config.construction_obligation),
        ),
        ConstructionHeuristicType::CheapestInsertion => DynamicScalarConstructionPhase::BestFit(
            ConstructionHeuristicPhase::new(placer, BestFitForager::new())
                .with_live_placement_refresh()
                .with_construction_obligation(config.construction_obligation),
        ),
        _ => unreachable!("unsupported dynamic scalar construction heuristic"),
    }
}

fn build_dynamic_assignment_construction(
    config: &ConstructionHeuristicConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
) -> DynamicAssignmentConstructionPhase {
    let placer = DynamicAssignmentConstructionPlacer {
        group_name: config
            .group_name
            .clone()
            .expect("dynamic assignment construction requires group_name"),
        scalar_slots: model.dynamic_scalar_variables().cloned().collect(),
        value_candidate_limit: config.value_candidate_limit,
        max_moves_per_step: config.group_candidate_limit,
        construction_heuristic_type: config.construction_heuristic_type,
        required_only: false,
        direct_required_cursor: true,
        required_stream: Arc::new(Mutex::new(DynamicAssignmentRequiredStream::default())),
    };
    let mandatory = build_dynamic_assignment_mandatory_construction(config, placer.required_only());
    DynamicAssignmentConstructionPhase {
        mandatory,
        placer,
        optional_step_limit: config
            .termination
            .as_ref()
            .and_then(|termination| termination.step_count_limit),
    }
}

fn build_dynamic_assignment_mandatory_construction(
    config: &ConstructionHeuristicConfig,
    placer: DynamicAssignmentConstructionPlacer,
) -> DynamicAssignmentMandatoryConstruction {
    match config.construction_heuristic_type {
        ConstructionHeuristicType::FirstFit => DynamicAssignmentMandatoryConstruction::FirstFit(
            ConstructionHeuristicPhase::new(placer, FirstFitForager::new())
                .with_live_placement_refresh()
                .with_construction_obligation(ConstructionObligation::AssignWhenCandidateExists)
                .with_mandatory_construction_completion(),
        ),
        ConstructionHeuristicType::CheapestInsertion => {
            DynamicAssignmentMandatoryConstruction::BestFit(
                ConstructionHeuristicPhase::new(placer, BestFitForager::new())
                    .with_live_placement_refresh()
                    .with_construction_obligation(ConstructionObligation::AssignWhenCandidateExists)
                    .with_mandatory_construction_completion(),
            )
        }
        _ => unreachable!("unsupported dynamic assignment construction heuristic"),
    }
}

fn can_bind_dynamic(
    config: &LocalSearchConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
) -> bool {
    config.local_search_type == LocalSearchType::AcceptorForager
        && config
            .move_selector
            .as_ref()
            .is_some_and(|selector| selector_is_dynamic_bindable(selector, model))
}

fn selector_is_dynamic_bindable(
    config: &MoveSelectorConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
) -> bool {
    DynamicAnySelector::try_new(Some(config), model, None).is_some()
}

fn build_dynamic_local_search(
    config: &LocalSearchConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    random_seed: Option<u64>,
) -> DynamicLocalSearch {
    let selector = DynamicMoveSelector::new(config.move_selector.as_ref(), model, random_seed);
    let acceptor = build_dynamic_acceptor(config.acceptor.as_ref(), model, random_seed);
    let forager = build_dynamic_forager(config.forager.as_ref(), model);
    let step_limit = config
        .termination
        .as_ref()
        .and_then(|termination| termination.step_count_limit);
    LocalSearchPhase::new(selector, acceptor, forager, step_limit)
}

fn build_dynamic_acceptor(
    config: Option<&AcceptorConfig>,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    random_seed: Option<u64>,
) -> AnyAcceptor<PyDynamicSolution> {
    if let Some(config) = config {
        return AcceptorBuilder::build_with_seed::<PyDynamicSolution>(config, random_seed);
    }

    let default_config = if model.has_list_variables() {
        AcceptorConfig::LateAcceptance(LateAcceptanceConfig {
            late_acceptance_size: Some(DEFAULT_LOCAL_SEARCH_LATE_ACCEPTANCE_SIZE),
        })
    } else if model.has_scalar_groups() {
        AcceptorConfig::DiversifiedLateAcceptance(DiversifiedLateAcceptanceConfig {
            late_acceptance_size: Some(DEFAULT_LOCAL_SEARCH_LATE_ACCEPTANCE_SIZE),
            tolerance: None,
        })
    } else {
        AcceptorConfig::SimulatedAnnealing(SimulatedAnnealingConfig {
            decay_rate: Some(DEFAULT_SIMULATED_ANNEALING_DECAY_RATE),
            ..SimulatedAnnealingConfig::default()
        })
    };
    AcceptorBuilder::build_with_seed::<PyDynamicSolution>(&default_config, random_seed)
}

fn build_dynamic_forager(
    config: Option<&ForagerConfig>,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
) -> AnyForager<PyDynamicSolution> {
    if let Some(config) = config {
        return ForagerBuilder::build::<PyDynamicSolution>(Some(config));
    }

    if model.has_scalar_groups() && !model.has_list_variables() {
        return AnyForager::LastStepScoreImproving(FirstLastStepScoreImprovingForager::new());
    }

    if model.has_list_precedence_variables() {
        return AnyForager::LastStepScoreImproving(FirstLastStepScoreImprovingForager::new());
    }

    let limit = if model.has_list_variables()
        || model.has_nearby_scalar_change_variables()
        || model.has_nearby_scalar_swap_variables()
        || model.has_conflict_repairs()
    {
        DEFAULT_LOCAL_SEARCH_ACCEPTED_COUNT
    } else {
        1
    };
    ForagerBuilder::build::<PyDynamicSolution>(Some(&ForagerConfig::AcceptedCount(
        AcceptedCountForagerConfig { limit: Some(limit) },
    )))
}

#[derive(Clone)]
pub(crate) struct DynamicScalarEntityPlacer {
    slots: Vec<DynamicScalarVariableSlot<PyDynamicSolution>>,
    value_candidate_limit: Option<usize>,
}

#[derive(Clone)]
pub(crate) struct DynamicAssignmentConstructionPlacer {
    group_name: String,
    scalar_slots: Vec<DynamicScalarVariableSlot<PyDynamicSolution>>,
    value_candidate_limit: Option<usize>,
    max_moves_per_step: Option<usize>,
    construction_heuristic_type: ConstructionHeuristicType,
    required_only: bool,
    direct_required_cursor: bool,
    required_stream: Arc<Mutex<DynamicAssignmentRequiredStream>>,
}

impl Debug for DynamicAssignmentConstructionPlacer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicAssignmentConstructionPlacer")
            .field("group_name", &self.group_name)
            .field("scalar_slot_count", &self.scalar_slots.len())
            .field("value_candidate_limit", &self.value_candidate_limit)
            .field("max_moves_per_step", &self.max_moves_per_step)
            .field(
                "construction_heuristic_type",
                &self.construction_heuristic_type,
            )
            .field("required_only", &self.required_only)
            .field("direct_required_cursor", &self.direct_required_cursor)
            .finish()
    }
}

#[derive(Default)]
struct DynamicAssignmentRequiredStream {
    cursor: Option<ScalarAssignmentRequiredStreamingCursor<PyDynamicSolution>>,
    pending: Option<CompoundScalarMove<PyDynamicSolution>>,
}

impl Debug for DynamicScalarEntityPlacer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicScalarEntityPlacer")
            .field("slot_count", &self.slots.len())
            .field("value_candidate_limit", &self.value_candidate_limit)
            .finish()
    }
}

impl EntityPlacer<PyDynamicSolution, solverforge_solver::DynamicScalarChangeMove<PyDynamicSolution>>
    for DynamicScalarEntityPlacer
{
    fn get_placements<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> Vec<
        Placement<
            PyDynamicSolution,
            solverforge_solver::DynamicScalarChangeMove<PyDynamicSolution>,
        >,
    > {
        let solution = score_director.working_solution();
        let mut placements = Vec::new();
        for slot in &self.slots {
            let entity_count = slot.entity_count(solution);
            for entity_index in 0..entity_count {
                if slot.current_value(solution, entity_index).is_some() {
                    continue;
                }
                let moves = slot
                    .candidate_values(solution, entity_index)
                    .iter()
                    .copied()
                    .take(self.value_candidate_limit.unwrap_or(usize::MAX))
                    .map(|value| {
                        solverforge_solver::DynamicScalarChangeMove::new(
                            slot.clone(),
                            entity_index,
                            Some(value),
                        )
                    })
                    .collect::<Vec<_>>();
                if moves.is_empty() {
                    continue;
                }
                placements.push(
                    Placement::new(
                        EntityReference::new(slot.descriptor_index(), entity_index),
                        moves,
                    )
                    .with_keep_current_legal(slot.allows_unassigned),
                );
            }
        }
        placements
    }

    fn get_next_placement<D, IsCompleted, ShouldStop>(
        &self,
        score_director: &D,
        mut is_completed: IsCompleted,
        mut should_stop: ShouldStop,
    ) -> Option<(
        Placement<
            PyDynamicSolution,
            solverforge_solver::DynamicScalarChangeMove<PyDynamicSolution>,
        >,
        u64,
    )>
    where
        D: Director<PyDynamicSolution>,
        IsCompleted: FnMut(
            &Placement<
                PyDynamicSolution,
                solverforge_solver::DynamicScalarChangeMove<PyDynamicSolution>,
            >,
        ) -> bool,
        ShouldStop: FnMut() -> bool,
    {
        let solution = score_director.working_solution();
        let mut generated_moves = 0u64;
        for slot in &self.slots {
            let entity_count = slot.entity_count(solution);
            for entity_index in 0..entity_count {
                if should_stop() {
                    return None;
                }
                if slot.current_value(solution, entity_index).is_some() {
                    continue;
                }
                let Some(placement) = dynamic_scalar_placement(
                    slot,
                    solution,
                    entity_index,
                    self.value_candidate_limit,
                ) else {
                    continue;
                };
                generated_moves = generated_moves
                    .saturating_add(u64::try_from(placement.moves.len()).unwrap_or(u64::MAX));
                if is_completed(&placement) {
                    continue;
                }
                return Some((placement, generated_moves));
            }
        }
        None
    }
}

fn dynamic_scalar_placement(
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    solution: &PyDynamicSolution,
    entity_index: usize,
    value_candidate_limit: Option<usize>,
) -> Option<
    Placement<PyDynamicSolution, solverforge_solver::DynamicScalarChangeMove<PyDynamicSolution>>,
> {
    let moves = slot
        .candidate_values(solution, entity_index)
        .iter()
        .copied()
        .take(value_candidate_limit.unwrap_or(usize::MAX))
        .map(|value| {
            solverforge_solver::DynamicScalarChangeMove::new(
                slot.clone(),
                entity_index,
                Some(value),
            )
        })
        .collect::<Vec<_>>();
    if moves.is_empty() {
        return None;
    }
    Some(
        Placement::new(
            EntityReference::new(slot.descriptor_index(), entity_index),
            moves,
        )
        .with_keep_current_legal(slot.allows_unassigned),
    )
}

impl EntityPlacer<PyDynamicSolution, CompoundScalarMove<PyDynamicSolution>>
    for DynamicAssignmentConstructionPlacer
{
    fn get_placements<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> Vec<Placement<PyDynamicSolution, CompoundScalarMove<PyDynamicSolution>>> {
        self.next_construction_assignment_placement(score_director)
            .map(|placement| vec![placement])
            .unwrap_or_default()
    }

    fn get_next_placement<D, IsCompleted, ShouldStop>(
        &self,
        score_director: &D,
        mut is_completed: IsCompleted,
        mut should_stop: ShouldStop,
    ) -> Option<(
        Placement<PyDynamicSolution, CompoundScalarMove<PyDynamicSolution>>,
        u64,
    )>
    where
        D: Director<PyDynamicSolution>,
        IsCompleted:
            FnMut(&Placement<PyDynamicSolution, CompoundScalarMove<PyDynamicSolution>>) -> bool,
        ShouldStop: FnMut() -> bool,
    {
        loop {
            if should_stop() {
                return None;
            }
            let placement = self.next_construction_assignment_placement(score_director)?;
            let generated_moves = u64::try_from(placement.moves.len()).unwrap_or(u64::MAX);
            if is_completed(&placement) {
                continue;
            }
            return Some((placement, generated_moves));
        }
    }
}

impl DynamicAssignmentConstructionPlacer {
    fn required_only(&self) -> Self {
        Self {
            required_only: true,
            required_stream: Arc::new(Mutex::new(DynamicAssignmentRequiredStream::default())),
            ..self.clone()
        }
    }

    fn sync_required_stream(&self, solution: &PyDynamicSolution) {
        let mut stream = self
            .required_stream
            .lock()
            .expect("dynamic assignment required stream mutex poisoned");
        let Some(pending) = stream.pending.take() else {
            return;
        };
        let committed = pending.edits().iter().all(|edit| {
            (edit.getter)(solution, edit.entity_index, edit.variable_index) == edit.to_value
        });
        if committed {
            if let Some(cursor) = &mut stream.cursor {
                with_dynamic_assignment_group(&self.group_name, || {
                    cursor.commit_move(solution, &pending)
                });
            }
        } else {
            stream.cursor = None;
        }
    }

    fn effective_limits(
        &self,
        group: ScalarGroupBinding<PyDynamicSolution>,
        solution: &PyDynamicSolution,
    ) -> ScalarGroupLimits {
        let group_candidate_limit = self
            .max_moves_per_step
            .or(group.limits.group_candidate_limit)
            .or_else(|| match group.kind {
                ScalarGroupBindingKind::Assignment(assignment) => {
                    let max_rematch_size = group.limits.max_rematch_size.unwrap_or(4).max(2);
                    let entity_count = with_dynamic_assignment_group(&self.group_name, || {
                        assignment.entity_count(solution)
                    });
                    Some(
                        entity_count
                            .saturating_mul(max_rematch_size)
                            .clamp(256, 4096),
                    )
                }
                ScalarGroupBindingKind::Candidates { .. } => Some(256),
            });
        ScalarGroupLimits {
            value_candidate_limit: self
                .value_candidate_limit
                .or(group.limits.value_candidate_limit),
            group_candidate_limit,
            max_moves_per_step: group.limits.max_moves_per_step,
            max_augmenting_depth: group.limits.max_augmenting_depth,
            max_rematch_size: group.limits.max_rematch_size,
        }
    }

    fn required_construction_options(
        &self,
        group: ScalarGroupBinding<PyDynamicSolution>,
        solution: &PyDynamicSolution,
    ) -> ScalarAssignmentMoveOptions {
        let limits = self.effective_limits(group, solution);
        let max_moves = if self.required_only
            && matches!(
                self.construction_heuristic_type,
                ConstructionHeuristicType::FirstFit
            ) {
            usize::MAX
        } else {
            limits.group_candidate_limit.unwrap_or(usize::MAX)
        };
        ScalarAssignmentMoveOptions::for_construction(limits).with_max_moves(max_moves)
    }

    fn assignment_unassigned_count<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> Option<u64> {
        let solution = score_director.working_solution();
        let group =
            dynamic_assignment_group_binding(solution, &self.group_name, &self.scalar_slots)
                .unwrap_or_else(|error| panic!("dynamic assignment group binding failed: {error}"));
        let assignment = group.assignment().copied()?;
        Some(with_dynamic_assignment_group(&self.group_name, || {
            assignment.unassigned_count(solution)
        }))
    }

    fn assignment_remaining_required_count<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> Option<u64> {
        let solution = score_director.working_solution();
        let group =
            dynamic_assignment_group_binding(solution, &self.group_name, &self.scalar_slots)
                .unwrap_or_else(|error| panic!("dynamic assignment group binding failed: {error}"));
        let assignment = group.assignment().copied()?;
        Some(with_dynamic_assignment_group(&self.group_name, || {
            assignment.remaining_required_count(solution)
        }))
    }

    fn next_optional_assignment_move<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> Option<DynamicMove> {
        let solution = score_director.working_solution();
        let group =
            dynamic_assignment_group_binding(solution, &self.group_name, &self.scalar_slots)
                .unwrap_or_else(|error| panic!("dynamic assignment group binding failed: {error}"));
        group.assignment()?;
        let unassigned_before = self.assignment_unassigned_count(score_director)?;
        if unassigned_before == 0 {
            return None;
        }
        let required_before = self.assignment_remaining_required_count(score_director)?;
        if required_before > 0 {
            return None;
        }
        let mut baseline_preview = DynamicPreviewDirector::from_director(score_director);
        let baseline_score = baseline_preview.calculate_score();
        let selector = GroupedScalarSelector::new(
            group,
            self.value_candidate_limit,
            self.max_moves_per_step,
            false,
        );
        let mut cursor = with_dynamic_assignment_group(&self.group_name, || {
            selector.open_cursor_with_context(score_director, MoveStreamContext::default())
        });
        while let Some(inner_id) =
            with_dynamic_assignment_group(&self.group_name, || cursor.next_candidate())
        {
            let scalar_move =
                with_dynamic_assignment_group(&self.group_name, || cursor.take_candidate(inner_id));
            let mov = DynamicMove::Scalar(DynamicScalar::Native(scalar_move));
            if !mov.is_doable(score_director) {
                continue;
            }
            let mut preview = DynamicPreviewDirector::from_director(score_director);
            let _undo = mov.do_move(&mut preview);
            let required_after = self.assignment_remaining_required_count(&preview)?;
            let unassigned_after = self.assignment_unassigned_count(&preview)?;
            if required_after == 0
                && unassigned_after < unassigned_before
                && preview.calculate_score() >= baseline_score
            {
                return Some(mov);
            }
        }
        None
    }

    fn next_construction_assignment_placement<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> Option<Placement<PyDynamicSolution, CompoundScalarMove<PyDynamicSolution>>> {
        if self.required_only {
            return self.next_required_assignment_placement(score_director);
        }
        let solution = score_director.working_solution();
        let group =
            dynamic_assignment_group_binding(solution, &self.group_name, &self.scalar_slots)
                .unwrap_or_else(|error| panic!("dynamic assignment group binding failed: {error}"));
        let assignment = group.assignment().copied()?;
        let required_remaining = with_dynamic_assignment_group(&self.group_name, || {
            assignment.remaining_required_count(solution)
        });
        if required_remaining == 0
            && with_dynamic_assignment_group(&self.group_name, || {
                assignment.unassigned_count(solution)
            }) == 0
        {
            return None;
        }
        let selector = GroupedScalarSelector::new(
            group,
            self.value_candidate_limit,
            self.max_moves_per_step,
            false,
        );
        let mut cursor = with_dynamic_assignment_group(&self.group_name, || {
            selector.open_cursor_with_context(score_director, MoveStreamContext::default())
        });
        let inner_id = with_dynamic_assignment_group(&self.group_name, || cursor.next_candidate())?;
        let mov = match with_dynamic_assignment_group(&self.group_name, || {
            cursor.take_candidate(inner_id)
        }) {
            ScalarMoveUnion::CompoundScalar(mov) => mov,
            _ => return None,
        };
        let edit = mov.edits().first()?;
        Some(Placement::new(
            EntityReference::new(edit.descriptor_index, edit.entity_index),
            vec![mov],
        ))
    }

    fn next_required_assignment_placement<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> Option<Placement<PyDynamicSolution, CompoundScalarMove<PyDynamicSolution>>> {
        if !self.direct_required_cursor {
            return self.next_required_selector_placement(score_director);
        }
        let solution = score_director.working_solution();
        self.sync_required_stream(solution);
        let group =
            dynamic_assignment_group_binding(solution, &self.group_name, &self.scalar_slots)
                .unwrap_or_else(|error| panic!("dynamic assignment group binding failed: {error}"));
        let assignment = group.assignment().copied()?;
        let required_remaining = with_dynamic_assignment_group(&self.group_name, || {
            assignment.remaining_required_count(solution)
        });
        if required_remaining == 0 {
            let mut stream = self
                .required_stream
                .lock()
                .expect("dynamic assignment required stream mutex poisoned");
            stream.cursor = None;
            stream.pending = None;
            return None;
        }

        let options = self.required_construction_options(group, solution);
        let mut stream = self
            .required_stream
            .lock()
            .expect("dynamic assignment required stream mutex poisoned");
        let mut reopened_after_exhaustion = false;
        loop {
            if stream.cursor.is_none() {
                let cursor = with_dynamic_assignment_group(&self.group_name, || {
                    ScalarAssignmentRequiredStreamingCursor::new(assignment, solution, options)
                });
                stream.cursor = Some(cursor);
            }
            let Some(cursor) = &mut stream.cursor else {
                return None;
            };
            let Some(mov) =
                with_dynamic_assignment_group(&self.group_name, || cursor.next_move(solution))
            else {
                stream.cursor = None;
                stream.pending = None;
                if reopened_after_exhaustion {
                    return None;
                }
                reopened_after_exhaustion = true;
                continue;
            };
            if mov.is_doable(score_director) {
                let edit = mov
                    .edits()
                    .iter()
                    .find(|edit| edit.to_value.is_some())
                    .or_else(|| mov.edits().first())?;
                stream.pending = Some(mov.clone());
                return Some(Placement::new(
                    EntityReference::new(edit.descriptor_index, edit.entity_index),
                    vec![mov],
                ));
            }
        }
    }

    fn next_required_selector_placement<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> Option<Placement<PyDynamicSolution, CompoundScalarMove<PyDynamicSolution>>> {
        let solution = score_director.working_solution();
        let group =
            dynamic_assignment_group_binding(solution, &self.group_name, &self.scalar_slots)
                .unwrap_or_else(|error| panic!("dynamic assignment group binding failed: {error}"));
        let assignment = group.assignment().copied()?;
        let required_before = with_dynamic_assignment_group(&self.group_name, || {
            assignment.remaining_required_count(solution)
        });
        if required_before == 0 {
            return None;
        }
        let selector = GroupedScalarSelector::new(
            group,
            self.value_candidate_limit,
            self.max_moves_per_step,
            false,
        );
        let mut cursor = with_dynamic_assignment_group(&self.group_name, || {
            selector.open_cursor_with_context(score_director, MoveStreamContext::default())
        });
        while let Some(inner_id) =
            with_dynamic_assignment_group(&self.group_name, || cursor.next_candidate())
        {
            let mov = match with_dynamic_assignment_group(&self.group_name, || {
                cursor.take_candidate(inner_id)
            }) {
                ScalarMoveUnion::CompoundScalar(mov) => mov,
                _ => continue,
            };
            if !mov.is_doable(score_director) {
                continue;
            }
            let mut preview = DynamicPreviewDirector::from_director(score_director);
            let _undo = mov.do_move(&mut preview);
            let required_after = self.assignment_remaining_required_count(&preview)?;
            if required_after >= required_before {
                continue;
            }
            let edit = mov
                .edits()
                .iter()
                .find(|edit| edit.to_value.is_some())
                .or_else(|| mov.edits().first())?;
            return Some(Placement::new(
                EntityReference::new(edit.descriptor_index, edit.entity_index),
                vec![mov],
            ));
        }
        None
    }
}

type DynamicScalarConstructionMove = solverforge_solver::DynamicScalarChangeMove<PyDynamicSolution>;

type DynamicAssignmentConstructionMove = CompoundScalarMove<PyDynamicSolution>;
type DynamicAssignmentFirstFitConstruction = ConstructionHeuristicPhase<
    PyDynamicSolution,
    DynamicAssignmentConstructionMove,
    DynamicAssignmentConstructionPlacer,
    FirstFitForager<PyDynamicSolution, DynamicAssignmentConstructionMove>,
>;
type DynamicAssignmentBestFitConstruction = ConstructionHeuristicPhase<
    PyDynamicSolution,
    DynamicAssignmentConstructionMove,
    DynamicAssignmentConstructionPlacer,
    BestFitForager<PyDynamicSolution, DynamicAssignmentConstructionMove>,
>;

pub(crate) enum DynamicAssignmentMandatoryConstruction {
    FirstFit(DynamicAssignmentFirstFitConstruction),
    BestFit(DynamicAssignmentBestFitConstruction),
}

impl Debug for DynamicAssignmentMandatoryConstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FirstFit(phase) => f
                .debug_tuple("DynamicAssignmentMandatoryConstruction::FirstFit")
                .field(phase)
                .finish(),
            Self::BestFit(phase) => f
                .debug_tuple("DynamicAssignmentMandatoryConstruction::BestFit")
                .field(phase)
                .finish(),
        }
    }
}

impl DynamicAssignmentMandatoryConstruction {
    fn solve<D, ProgressCb>(
        &mut self,
        solver_scope: &mut solverforge_solver::SolverScope<'_, PyDynamicSolution, D, ProgressCb>,
    ) where
        D: Director<PyDynamicSolution>,
        ProgressCb: ProgressCallback<PyDynamicSolution>,
    {
        match self {
            Self::FirstFit(phase) => {
                solver_scope.mutate(|score_director| score_director.reset());
                phase.solve(solver_scope);
            }
            Self::BestFit(phase) => phase.solve(solver_scope),
        }
    }
}

pub(crate) struct DynamicAssignmentConstructionPhase {
    mandatory: DynamicAssignmentMandatoryConstruction,
    placer: DynamicAssignmentConstructionPlacer,
    optional_step_limit: Option<u64>,
}

impl Debug for DynamicAssignmentConstructionPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicAssignmentConstructionPhase")
            .field("mandatory", &self.mandatory)
            .field("placer", &self.placer)
            .field("optional_step_limit", &self.optional_step_limit)
            .finish()
    }
}

impl<D, ProgressCb> Phase<PyDynamicSolution, D, ProgressCb> for DynamicAssignmentConstructionPhase
where
    D: Director<PyDynamicSolution>,
    ProgressCb: ProgressCallback<PyDynamicSolution>,
{
    fn solve(
        &mut self,
        solver_scope: &mut solverforge_solver::SolverScope<'_, PyDynamicSolution, D, ProgressCb>,
    ) {
        self.mandatory.solve(solver_scope);
        let mut optional_steps = 0_u64;
        loop {
            if solver_scope.should_terminate() {
                break;
            }
            if self
                .optional_step_limit
                .is_some_and(|limit| optional_steps >= limit)
            {
                break;
            }
            let Some(mov) = self
                .placer
                .next_optional_assignment_move(solver_scope.score_director())
            else {
                break;
            };
            solver_scope.mutate(|score_director| {
                mov.do_move(score_director);
            });
            solver_scope.increment_step_count();
            optional_steps = optional_steps.saturating_add(1);
        }
        solver_scope.update_best_solution();
    }

    fn phase_type_name(&self) -> &'static str {
        "DynamicAssignmentConstruction"
    }
}

type DynamicScalarFirstFitConstruction = ConstructionHeuristicPhase<
    PyDynamicSolution,
    DynamicScalarConstructionMove,
    DynamicScalarEntityPlacer,
    FirstFitForager<PyDynamicSolution, DynamicScalarConstructionMove>,
>;
type DynamicScalarBestFitConstruction = ConstructionHeuristicPhase<
    PyDynamicSolution,
    DynamicScalarConstructionMove,
    DynamicScalarEntityPlacer,
    BestFitForager<PyDynamicSolution, DynamicScalarConstructionMove>,
>;

pub(crate) enum DynamicScalarConstructionPhase {
    FirstFit(DynamicScalarFirstFitConstruction),
    BestFit(DynamicScalarBestFitConstruction),
}

impl Debug for DynamicScalarConstructionPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FirstFit(phase) => f
                .debug_tuple("DynamicScalarConstructionPhase::FirstFit")
                .field(phase)
                .finish(),
            Self::BestFit(phase) => f
                .debug_tuple("DynamicScalarConstructionPhase::BestFit")
                .field(phase)
                .finish(),
        }
    }
}

impl<D, ProgressCb> Phase<PyDynamicSolution, D, ProgressCb> for DynamicScalarConstructionPhase
where
    D: Director<PyDynamicSolution>,
    ProgressCb: ProgressCallback<PyDynamicSolution>,
{
    fn solve(
        &mut self,
        solver_scope: &mut solverforge_solver::SolverScope<'_, PyDynamicSolution, D, ProgressCb>,
    ) {
        match self {
            Self::FirstFit(phase) => phase.solve(solver_scope),
            Self::BestFit(phase) => phase.solve(solver_scope),
        }
    }

    fn phase_type_name(&self) -> &'static str {
        "DynamicScalarConstruction"
    }
}

#[derive(Clone)]
pub struct DynamicCompoundScalarEdit {
    slot: DynamicScalarVariableSlot<PyDynamicSolution>,
    entity_index: usize,
    to_value: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub enum DynamicCompoundScalarKind {
    Grouped,
    ConflictRepair,
    CompoundConflictRepair,
}

#[derive(Clone)]
pub struct DynamicCompoundScalarMove {
    kind: DynamicCompoundScalarKind,
    reason: String,
    variable_label: String,
    edits: Vec<DynamicCompoundScalarEdit>,
    entity_indices: Vec<usize>,
    descriptor_index: usize,
    require_hard_improvement: bool,
}

impl DynamicCompoundScalarMove {
    fn new(
        kind: DynamicCompoundScalarKind,
        reason: String,
        variable_label: &'static str,
        edits: Vec<DynamicCompoundScalarEdit>,
        require_hard_improvement: bool,
    ) -> Self {
        let descriptor_index = edits
            .first()
            .map(|edit| edit.slot.descriptor_index())
            .unwrap_or(usize::MAX);
        let mut entity_indices = edits
            .iter()
            .map(|edit| edit.entity_index)
            .collect::<Vec<_>>();
        entity_indices.sort_unstable();
        entity_indices.dedup();
        Self {
            kind,
            reason,
            variable_label: variable_label.to_string(),
            edits,
            entity_indices,
            descriptor_index,
            require_hard_improvement,
        }
    }
}

impl Debug for DynamicCompoundScalarMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicCompoundScalarMove")
            .field("kind", &self.kind)
            .field("reason", &self.reason)
            .field("variable_label", &self.variable_label)
            .field("entity_indices", &self.entity_indices)
            .field("require_hard_improvement", &self.require_hard_improvement)
            .finish()
    }
}

pub type DynamicCompoundScalarUndo = Vec<Option<usize>>;

impl Move<PyDynamicSolution> for DynamicCompoundScalarMove {
    type Undo = DynamicCompoundScalarUndo;

    fn is_doable<D: Director<PyDynamicSolution>>(&self, score_director: &D) -> bool {
        if self.edits.is_empty() || compound_has_duplicate_targets(&self.edits) {
            return false;
        }
        let solution = score_director.working_solution();
        let mut changes_value = false;
        for edit in &self.edits {
            if edit.entity_index >= edit.slot.entity_count(solution) {
                return false;
            }
            if !edit
                .slot
                .value_is_legal(solution, edit.entity_index, edit.to_value)
            {
                return false;
            }
            let current = edit.slot.current_value(solution, edit.entity_index);
            changes_value |= current != edit.to_value;
        }
        changes_value
    }

    fn do_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D) -> Self::Undo {
        let mut undo = Vec::with_capacity(self.edits.len());
        let affected = unique_dynamic_compound_entities(&self.edits);
        for edit in &self.edits {
            undo.push(
                edit.slot
                    .current_value(score_director.working_solution(), edit.entity_index),
            );
        }
        for (descriptor_index, entity_index) in &affected {
            score_director.before_variable_changed(*descriptor_index, *entity_index);
        }
        for edit in &self.edits {
            edit.slot.set_value(
                score_director.working_solution_mut(),
                edit.entity_index,
                edit.to_value,
            );
        }
        for (descriptor_index, entity_index) in affected.iter().rev() {
            score_director.after_variable_changed(*descriptor_index, *entity_index);
        }
        undo
    }

    fn undo_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D, undo: Self::Undo) {
        let affected = unique_dynamic_compound_entities(&self.edits);
        for (descriptor_index, entity_index) in &affected {
            score_director.before_variable_changed(*descriptor_index, *entity_index);
        }
        for (edit, old_value) in self.edits.iter().zip(undo) {
            edit.slot.set_value(
                score_director.working_solution_mut(),
                edit.entity_index,
                old_value,
            );
        }
        for (descriptor_index, entity_index) in affected.iter().rev() {
            score_director.after_variable_changed(*descriptor_index, *entity_index);
        }
    }

    fn descriptor_index(&self) -> usize {
        self.descriptor_index
    }

    fn entity_indices(&self) -> &[usize] {
        &self.entity_indices
    }

    fn variable_name(&self) -> &str {
        &self.variable_label
    }

    fn telemetry_label(&self) -> &'static str {
        match self.kind {
            DynamicCompoundScalarKind::Grouped => "dynamic_scalar_grouped",
            DynamicCompoundScalarKind::ConflictRepair => "dynamic_scalar_conflict_repair",
            DynamicCompoundScalarKind::CompoundConflictRepair => {
                "dynamic_scalar_compound_conflict_repair"
            }
        }
    }

    fn requires_hard_improvement(&self) -> bool {
        self.require_hard_improvement
    }

    fn tabu_signature<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> MoveTabuSignature {
        let first = self
            .edits
            .first()
            .expect("compound dynamic scalar tabu signature requires at least one edit");
        let first_signature = edit_change_signature(first, score_director);
        let mut entity_tokens = first_signature.entity_tokens.clone();
        let mut destination_value_tokens = first_signature.destination_value_tokens.clone();
        let mut move_id = smallvec::smallvec![
            0xDCA7_0000_0000_1000
                ^ match self.kind {
                    DynamicCompoundScalarKind::Grouped => 1,
                    DynamicCompoundScalarKind::ConflictRepair => 2,
                    DynamicCompoundScalarKind::CompoundConflictRepair => 3,
                }
        ];
        let mut undo_move_id = move_id.clone();
        move_id.extend(first_signature.move_id.iter().copied());
        undo_move_id.extend(first_signature.undo_move_id.iter().copied());
        for edit in self.edits.iter().skip(1) {
            let signature = edit_change_signature(edit, score_director);
            for token in signature.entity_tokens {
                if !entity_tokens.contains(&token) {
                    entity_tokens.push(token);
                }
            }
            for token in signature.destination_value_tokens {
                if !destination_value_tokens.contains(&token) {
                    destination_value_tokens.push(token);
                }
            }
            move_id.extend(signature.move_id.iter().copied());
            undo_move_id.extend(signature.undo_move_id.iter().copied());
        }
        MoveTabuSignature {
            scope: first_signature.scope,
            entity_tokens,
            destination_value_tokens,
            move_id,
            undo_move_id,
        }
    }
}

fn edit_change_signature<D: Director<PyDynamicSolution>>(
    edit: &DynamicCompoundScalarEdit,
    score_director: &D,
) -> MoveTabuSignature {
    with_dynamic_scalar_slot(&edit.slot, || {
        solverforge_solver::DynamicScalarChangeMove::new(
            edit.slot.clone(),
            edit.entity_index,
            edit.to_value,
        )
        .tabu_signature(score_director)
    })
}

fn compound_has_duplicate_targets(edits: &[DynamicCompoundScalarEdit]) -> bool {
    let mut targets = HashSet::new();
    edits.iter().any(|edit| {
        !targets.insert((
            edit.slot.descriptor_index(),
            edit.entity_index,
            edit.slot.variable_name,
        ))
    })
}

fn unique_dynamic_compound_entities(edits: &[DynamicCompoundScalarEdit]) -> Vec<(usize, usize)> {
    let mut affected = Vec::new();
    for edit in edits {
        let entity = (edit.slot.descriptor_index(), edit.entity_index);
        if !affected.contains(&entity) {
            affected.push(entity);
        }
    }
    affected
}

#[derive(Clone)]
pub enum DynamicScalar {
    Native(ScalarMoveUnion<PyDynamicSolution, usize>),
    Change(solverforge_solver::DynamicScalarChangeMove<PyDynamicSolution>),
    Swap(DynamicScalarSwapMove),
    PillarChange {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        mov: PillarChangeMove<PyDynamicSolution, usize>,
    },
    PillarSwap {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        mov: PillarSwapMove<PyDynamicSolution, usize>,
    },
    RuinRecreate {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        mov: RuinRecreateMove<PyDynamicSolution>,
    },
    Grouped(DynamicCompoundScalarMove),
    ConflictRepair(DynamicCompoundScalarMove),
    CompoundConflictRepair(DynamicCompoundScalarMove),
}

impl Debug for DynamicScalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native(mov) => Debug::fmt(mov, f),
            Self::Change(change) => Debug::fmt(change, f),
            Self::Swap(swap) => Debug::fmt(swap, f),
            Self::PillarChange { mov, .. } => Debug::fmt(mov, f),
            Self::PillarSwap { mov, .. } => Debug::fmt(mov, f),
            Self::RuinRecreate { mov, .. } => Debug::fmt(mov, f),
            Self::Grouped(mov) | Self::ConflictRepair(mov) | Self::CompoundConflictRepair(mov) => {
                Debug::fmt(mov, f)
            }
        }
    }
}

pub enum DynamicScalarUndo {
    Native(<ScalarMoveUnion<PyDynamicSolution, usize> as solverforge_solver::Move<PyDynamicSolution>>::Undo),
    Change(<solverforge_solver::DynamicScalarChangeMove<PyDynamicSolution> as solverforge_solver::Move<PyDynamicSolution>>::Undo),
    Swap(<DynamicScalarSwapMove as solverforge_solver::Move<PyDynamicSolution>>::Undo),
    PillarChange(<PillarChangeMove<PyDynamicSolution, usize> as solverforge_solver::Move<PyDynamicSolution>>::Undo),
    PillarSwap(<PillarSwapMove<PyDynamicSolution, usize> as solverforge_solver::Move<PyDynamicSolution>>::Undo),
    RuinRecreate(<RuinRecreateMove<PyDynamicSolution> as solverforge_solver::Move<PyDynamicSolution>>::Undo),
    Grouped(DynamicCompoundScalarUndo),
    ConflictRepair(DynamicCompoundScalarUndo),
    CompoundConflictRepair(DynamicCompoundScalarUndo),
}

impl solverforge_solver::Move<PyDynamicSolution> for DynamicScalar {
    type Undo = DynamicScalarUndo;

    fn is_doable<D: Director<PyDynamicSolution>>(&self, score_director: &D) -> bool {
        match self {
            Self::Native(mov) => mov.is_doable(score_director),
            Self::Change(change) => change.is_doable(score_director),
            Self::Swap(swap) => swap.is_doable(score_director),
            Self::PillarChange { slot, mov } => {
                with_dynamic_scalar_slot(slot, || mov.is_doable(score_director))
            }
            Self::PillarSwap { slot, mov } => {
                with_dynamic_scalar_slot(slot, || mov.is_doable(score_director))
            }
            Self::RuinRecreate { slot, mov } => {
                with_dynamic_scalar_slot(slot, || mov.is_doable(score_director))
            }
            Self::Grouped(mov) | Self::ConflictRepair(mov) | Self::CompoundConflictRepair(mov) => {
                mov.is_doable(score_director)
            }
        }
    }

    fn do_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D) -> Self::Undo {
        match self {
            Self::Native(mov) => DynamicScalarUndo::Native(mov.do_move(score_director)),
            Self::Change(change) => DynamicScalarUndo::Change(change.do_move(score_director)),
            Self::Swap(swap) => DynamicScalarUndo::Swap(swap.do_move(score_director)),
            Self::PillarChange { slot, mov } => with_dynamic_scalar_slot(slot, || {
                DynamicScalarUndo::PillarChange(mov.do_move(score_director))
            }),
            Self::PillarSwap { slot, mov } => with_dynamic_scalar_slot(slot, || {
                DynamicScalarUndo::PillarSwap(mov.do_move(score_director))
            }),
            Self::RuinRecreate { slot, mov } => with_dynamic_scalar_slot(slot, || {
                DynamicScalarUndo::RuinRecreate(mov.do_move(score_director))
            }),
            Self::Grouped(mov) => DynamicScalarUndo::Grouped(mov.do_move(score_director)),
            Self::ConflictRepair(mov) => {
                DynamicScalarUndo::ConflictRepair(mov.do_move(score_director))
            }
            Self::CompoundConflictRepair(mov) => {
                DynamicScalarUndo::CompoundConflictRepair(mov.do_move(score_director))
            }
        }
    }

    fn undo_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D, undo: Self::Undo) {
        match (self, undo) {
            (Self::Native(mov), DynamicScalarUndo::Native(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::Change(change), DynamicScalarUndo::Change(undo)) => {
                change.undo_move(score_director, undo);
            }
            (Self::Swap(swap), DynamicScalarUndo::Swap(undo)) => {
                swap.undo_move(score_director, undo);
            }
            (Self::PillarChange { slot, mov }, DynamicScalarUndo::PillarChange(undo)) => {
                with_dynamic_scalar_slot(slot, || mov.undo_move(score_director, undo));
            }
            (Self::PillarSwap { slot, mov }, DynamicScalarUndo::PillarSwap(undo)) => {
                with_dynamic_scalar_slot(slot, || mov.undo_move(score_director, undo));
            }
            (Self::RuinRecreate { slot, mov }, DynamicScalarUndo::RuinRecreate(undo)) => {
                with_dynamic_scalar_slot(slot, || mov.undo_move(score_director, undo));
            }
            (Self::Grouped(mov), DynamicScalarUndo::Grouped(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::ConflictRepair(mov), DynamicScalarUndo::ConflictRepair(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (
                Self::CompoundConflictRepair(mov),
                DynamicScalarUndo::CompoundConflictRepair(undo),
            ) => {
                mov.undo_move(score_director, undo);
            }
            _ => panic!("dynamic scalar move undo variant mismatch"),
        }
    }

    fn descriptor_index(&self) -> usize {
        match self {
            Self::Native(mov) => mov.descriptor_index(),
            Self::Change(change) => change.descriptor_index(),
            Self::Swap(swap) => swap.descriptor_index(),
            Self::PillarChange { mov, .. } => mov.descriptor_index(),
            Self::PillarSwap { mov, .. } => mov.descriptor_index(),
            Self::RuinRecreate { mov, .. } => mov.descriptor_index(),
            Self::Grouped(mov) | Self::ConflictRepair(mov) | Self::CompoundConflictRepair(mov) => {
                mov.descriptor_index()
            }
        }
    }

    fn entity_indices(&self) -> &[usize] {
        match self {
            Self::Native(mov) => mov.entity_indices(),
            Self::Change(change) => change.entity_indices(),
            Self::Swap(swap) => swap.entity_indices(),
            Self::PillarChange { mov, .. } => mov.entity_indices(),
            Self::PillarSwap { mov, .. } => mov.entity_indices(),
            Self::RuinRecreate { mov, .. } => mov.entity_indices(),
            Self::Grouped(mov) | Self::ConflictRepair(mov) | Self::CompoundConflictRepair(mov) => {
                mov.entity_indices()
            }
        }
    }

    fn variable_name(&self) -> &str {
        match self {
            Self::Native(mov) => mov.variable_name(),
            Self::Change(change) => change.variable_name(),
            Self::Swap(swap) => swap.variable_name(),
            Self::PillarChange { mov, .. } => mov.variable_name(),
            Self::PillarSwap { mov, .. } => mov.variable_name(),
            Self::RuinRecreate { mov, .. } => mov.variable_name(),
            Self::Grouped(mov) | Self::ConflictRepair(mov) | Self::CompoundConflictRepair(mov) => {
                mov.variable_name()
            }
        }
    }

    fn telemetry_label(&self) -> &'static str {
        match self {
            Self::Native(mov) => mov.telemetry_label(),
            Self::Change(change) => change.telemetry_label(),
            Self::Swap(swap) => swap.telemetry_label(),
            Self::PillarChange { .. } => "dynamic_scalar_pillar_change",
            Self::PillarSwap { .. } => "dynamic_scalar_pillar_swap",
            Self::RuinRecreate { .. } => "dynamic_scalar_ruin_recreate",
            Self::Grouped(mov) | Self::ConflictRepair(mov) | Self::CompoundConflictRepair(mov) => {
                mov.telemetry_label()
            }
        }
    }

    fn tabu_signature<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> MoveTabuSignature {
        match self {
            Self::Native(mov) => mov.tabu_signature(score_director),
            Self::Change(change) => change.tabu_signature(score_director),
            Self::Swap(swap) => swap.tabu_signature(score_director),
            Self::PillarChange { slot, mov } => {
                with_dynamic_scalar_slot(slot, || mov.tabu_signature(score_director))
            }
            Self::PillarSwap { slot, mov } => {
                with_dynamic_scalar_slot(slot, || mov.tabu_signature(score_director))
            }
            Self::RuinRecreate { slot, mov } => {
                with_dynamic_scalar_slot(slot, || mov.tabu_signature(score_director))
            }
            Self::Grouped(mov) | Self::ConflictRepair(mov) | Self::CompoundConflictRepair(mov) => {
                mov.tabu_signature(score_director)
            }
        }
    }

    fn requires_hard_improvement(&self) -> bool {
        match self {
            Self::Native(mov) => mov.requires_hard_improvement(),
            Self::Change(change) => change.requires_hard_improvement(),
            Self::Swap(swap) => swap.requires_hard_improvement(),
            Self::PillarChange { mov, .. } => mov.requires_hard_improvement(),
            Self::PillarSwap { mov, .. } => mov.requires_hard_improvement(),
            Self::RuinRecreate { mov, .. } => mov.requires_hard_improvement(),
            Self::Grouped(mov) | Self::ConflictRepair(mov) | Self::CompoundConflictRepair(mov) => {
                mov.requires_hard_improvement()
            }
        }
    }
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum DynamicMove {
    Scalar(DynamicScalar),
    List(DynamicList),
    Cartesian(DynamicCartesianMove),
}

impl Debug for DynamicMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scalar(mov) => Debug::fmt(mov, f),
            Self::List(mov) => Debug::fmt(mov, f),
            Self::Cartesian(mov) => Debug::fmt(mov, f),
        }
    }
}

pub enum DynamicMoveUndo {
    Scalar(DynamicScalarUndo),
    List(DynamicListUndo),
    Cartesian(DynamicCartesianUndo),
}

impl solverforge_solver::Move<PyDynamicSolution> for DynamicMove {
    type Undo = DynamicMoveUndo;

    fn is_doable<D: Director<PyDynamicSolution>>(&self, score_director: &D) -> bool {
        match self {
            Self::Scalar(mov) => mov.is_doable(score_director),
            Self::List(mov) => mov.is_doable(score_director),
            Self::Cartesian(mov) => mov.is_doable(score_director),
        }
    }

    fn do_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D) -> Self::Undo {
        match self {
            Self::Scalar(mov) => DynamicMoveUndo::Scalar(mov.do_move(score_director)),
            Self::List(mov) => DynamicMoveUndo::List(mov.do_move(score_director)),
            Self::Cartesian(mov) => DynamicMoveUndo::Cartesian(mov.do_move(score_director)),
        }
    }

    fn undo_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D, undo: Self::Undo) {
        match (self, undo) {
            (Self::Scalar(mov), DynamicMoveUndo::Scalar(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::List(mov), DynamicMoveUndo::List(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::Cartesian(mov), DynamicMoveUndo::Cartesian(undo)) => {
                mov.undo_move(score_director, undo);
            }
            _ => panic!("dynamic move undo variant mismatch"),
        }
    }

    fn descriptor_index(&self) -> usize {
        match self {
            Self::Scalar(mov) => mov.descriptor_index(),
            Self::List(mov) => mov.descriptor_index(),
            Self::Cartesian(mov) => mov.descriptor_index(),
        }
    }

    fn entity_indices(&self) -> &[usize] {
        match self {
            Self::Scalar(mov) => mov.entity_indices(),
            Self::List(mov) => mov.entity_indices(),
            Self::Cartesian(mov) => mov.entity_indices(),
        }
    }

    fn variable_name(&self) -> &str {
        match self {
            Self::Scalar(mov) => mov.variable_name(),
            Self::List(mov) => mov.variable_name(),
            Self::Cartesian(mov) => mov.variable_name(),
        }
    }

    fn telemetry_label(&self) -> &'static str {
        match self {
            Self::Scalar(mov) => mov.telemetry_label(),
            Self::List(mov) => mov.telemetry_label(),
            Self::Cartesian(mov) => mov.telemetry_label(),
        }
    }

    fn tabu_signature<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> MoveTabuSignature {
        match self {
            Self::Scalar(mov) => mov.tabu_signature(score_director),
            Self::List(mov) => mov.tabu_signature(score_director),
            Self::Cartesian(mov) => mov.tabu_signature(score_director),
        }
    }

    fn requires_hard_improvement(&self) -> bool {
        match self {
            Self::Scalar(mov) => mov.requires_hard_improvement(),
            Self::List(mov) => mov.requires_hard_improvement(),
            Self::Cartesian(mov) => mov.requires_hard_improvement(),
        }
    }
}

#[derive(Clone)]
pub struct DynamicCartesianMove {
    first: Box<DynamicMove>,
    second: Box<DynamicMove>,
    descriptor_index: usize,
    entity_indices: Vec<usize>,
    variable_name: String,
    tabu_signature: MoveTabuSignature,
    require_hard_improvement: bool,
}

pub struct DynamicCartesianUndo {
    first: Box<DynamicMoveUndo>,
    second: Box<DynamicMoveUndo>,
}

impl DynamicCartesianMove {
    fn new(
        first: DynamicMove,
        second: DynamicMove,
        tabu_signature: MoveTabuSignature,
        require_hard_improvement: bool,
    ) -> Self {
        let descriptor_index = first.descriptor_index();
        let mut entity_indices = first.entity_indices().to_vec();
        entity_indices.extend(second.entity_indices().iter().copied());
        entity_indices.sort_unstable();
        entity_indices.dedup();
        Self {
            first: Box::new(first),
            second: Box::new(second),
            descriptor_index,
            entity_indices,
            variable_name: "dynamic_cartesian".to_string(),
            tabu_signature,
            require_hard_improvement,
        }
    }
}

impl Debug for DynamicCartesianMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicCartesianMove")
            .field("descriptor_index", &self.descriptor_index)
            .field("entity_indices", &self.entity_indices)
            .field("require_hard_improvement", &self.require_hard_improvement)
            .finish()
    }
}

impl Move<PyDynamicSolution> for DynamicCartesianMove {
    type Undo = DynamicCartesianUndo;

    fn is_doable<D: Director<PyDynamicSolution>>(&self, score_director: &D) -> bool {
        if !self.first.is_doable(score_director) {
            return false;
        }
        let mut preview = DynamicPreviewDirector::from_director(score_director);
        let _undo = self.first.do_move(&mut preview);
        self.second.is_doable(&preview)
    }

    fn do_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D) -> Self::Undo {
        let first = self.first.do_move(score_director);
        let second = self.second.do_move(score_director);
        DynamicCartesianUndo {
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    fn undo_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D, undo: Self::Undo) {
        self.second.undo_move(score_director, *undo.second);
        self.first.undo_move(score_director, *undo.first);
    }

    fn descriptor_index(&self) -> usize {
        self.descriptor_index
    }

    fn entity_indices(&self) -> &[usize] {
        &self.entity_indices
    }

    fn variable_name(&self) -> &str {
        &self.variable_name
    }

    fn telemetry_label(&self) -> &'static str {
        "dynamic_cartesian"
    }

    fn requires_hard_improvement(&self) -> bool {
        self.require_hard_improvement
            || self.first.requires_hard_improvement()
            || self.second.requires_hard_improvement()
    }

    fn tabu_signature<D: Director<PyDynamicSolution>>(
        &self,
        _score_director: &D,
    ) -> MoveTabuSignature {
        self.tabu_signature.clone()
    }
}

struct DynamicPreviewDirector<'a> {
    working_solution: PyDynamicSolution,
    descriptor: &'a SolutionDescriptor,
    entity_counts: Vec<Option<usize>>,
    total_entity_count: Option<usize>,
}

impl<'a> DynamicPreviewDirector<'a> {
    fn from_director<D: Director<PyDynamicSolution>>(score_director: &'a D) -> Self {
        let descriptor = score_director.solution_descriptor();
        let entity_counts = (0..descriptor.entity_descriptor_count())
            .map(|descriptor_index| score_director.entity_count(descriptor_index))
            .collect();
        Self {
            working_solution: score_director.clone_working_solution(),
            descriptor,
            entity_counts,
            total_entity_count: score_director.total_entity_count(),
        }
    }
}

impl Director<PyDynamicSolution> for DynamicPreviewDirector<'_> {
    fn working_solution(&self) -> &PyDynamicSolution {
        &self.working_solution
    }

    fn working_solution_mut(&mut self) -> &mut PyDynamicSolution {
        &mut self.working_solution
    }

    fn calculate_score(&mut self) -> DynamicScore {
        Python::attach(|py| {
            let constraints = self.working_solution.schema.constraints.clone_ref(py);
            PyDynamicConstraintSet::new(constraints).evaluate_all(&self.working_solution)
        })
    }

    fn solution_descriptor(&self) -> &SolutionDescriptor {
        self.descriptor
    }

    fn clone_working_solution(&self) -> PyDynamicSolution {
        self.working_solution.clone()
    }

    fn before_variable_changed(&mut self, _descriptor_index: usize, _entity_index: usize) {
        self.working_solution.set_score(None);
    }

    fn after_variable_changed(&mut self, descriptor_index: usize, entity_index: usize) {
        self.working_solution
            .update_entity_shadows(descriptor_index, entity_index);
        self.working_solution.set_score(None);
    }

    fn entity_count(&self, descriptor_index: usize) -> Option<usize> {
        self.entity_counts.get(descriptor_index).copied().flatten()
    }

    fn total_entity_count(&self) -> Option<usize> {
        self.total_entity_count
    }

    fn constraint_metadata(&self) -> Vec<ConstraintMetadata<'_>> {
        Vec::new()
    }

    fn snapshot_score_state(&self) -> DirectorScoreState<DynamicScore> {
        let solution_score = self.working_solution.score();
        DirectorScoreState {
            solution_score,
            committed_score: solution_score,
            initialized: solution_score.is_some(),
        }
    }

    fn restore_score_state(&mut self, state: DirectorScoreState<DynamicScore>) {
        self.working_solution.set_score(state.solution_score);
    }
}

#[derive(Clone)]
pub struct DynamicScalarSwapMove {
    slot: DynamicScalarVariableSlot<PyDynamicSolution>,
    left_entity_index: usize,
    right_entity_index: usize,
    entity_indices: [usize; 2],
}

impl DynamicScalarSwapMove {
    fn new(
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        left_entity_index: usize,
        right_entity_index: usize,
    ) -> Self {
        Self {
            slot,
            left_entity_index,
            right_entity_index,
            entity_indices: [left_entity_index, right_entity_index],
        }
    }
}

thread_local! {
    static ACTIVE_DYNAMIC_SCALAR_SLOT: RefCell<Vec<DynamicScalarVariableSlot<PyDynamicSolution>>> =
        const { RefCell::new(Vec::new()) };
    static ACTIVE_DYNAMIC_ASSIGNMENT_GROUP: RefCell<Vec<String>> =
        const { RefCell::new(Vec::new()) };
}

fn with_dynamic_scalar_slot<R>(
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    f: impl FnOnce() -> R,
) -> R {
    ACTIVE_DYNAMIC_SCALAR_SLOT.with(|stack| stack.borrow_mut().push(slot.clone()));
    let result = catch_unwind(AssertUnwindSafe(f));
    ACTIVE_DYNAMIC_SCALAR_SLOT.with(|stack| {
        let popped = stack.borrow_mut().pop();
        debug_assert!(popped.is_some(), "dynamic scalar slot stack underflow");
    });
    match result {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
}

fn active_dynamic_scalar_slot() -> DynamicScalarVariableSlot<PyDynamicSolution> {
    ACTIVE_DYNAMIC_SCALAR_SLOT.with(|stack| {
        stack
            .borrow()
            .last()
            .cloned()
            .expect("dynamic scalar operation used outside an active scalar slot")
    })
}

fn with_dynamic_assignment_group<T>(group_name: &str, f: impl FnOnce() -> T) -> T {
    ACTIVE_DYNAMIC_ASSIGNMENT_GROUP.with(|stack| stack.borrow_mut().push(group_name.to_string()));
    let result = catch_unwind(AssertUnwindSafe(f));
    ACTIVE_DYNAMIC_ASSIGNMENT_GROUP.with(|stack| {
        let popped = stack.borrow_mut().pop();
        assert!(
            popped.as_deref() == Some(group_name),
            "dynamic assignment group stack imbalance"
        );
    });
    match result {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
}

fn active_dynamic_assignment_group() -> String {
    ACTIVE_DYNAMIC_ASSIGNMENT_GROUP.with(|stack| {
        stack
            .borrow()
            .last()
            .cloned()
            .expect("dynamic assignment hook used outside an active assignment group")
    })
}

fn dynamic_scalar_get(
    solution: &PyDynamicSolution,
    entity_index: usize,
    _variable_index: usize,
) -> Option<usize> {
    active_dynamic_scalar_slot().current_value(solution, entity_index)
}

fn dynamic_scalar_set(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    _variable_index: usize,
    value: Option<usize>,
) {
    active_dynamic_scalar_slot().set_value(solution, entity_index, value);
}

fn dynamic_scalar_candidate_values(
    solution: &PyDynamicSolution,
    entity_index: usize,
    _variable_index: usize,
) -> &[usize] {
    active_dynamic_scalar_slot().candidate_values(solution, entity_index)
}

const DYNAMIC_SCALAR_KEY_SHIFT: usize = 32;
const DYNAMIC_SCALAR_KEY_MASK: usize = (1usize << DYNAMIC_SCALAR_KEY_SHIFT) - 1;

fn dynamic_scalar_key(entity: EntityClassId, variable: VariableId) -> usize {
    (entity.0 << DYNAMIC_SCALAR_KEY_SHIFT) | variable.0
}

fn decode_dynamic_scalar_key(key: usize) -> (EntityClassId, VariableId) {
    (
        EntityClassId(key >> DYNAMIC_SCALAR_KEY_SHIFT),
        VariableId(key & DYNAMIC_SCALAR_KEY_MASK),
    )
}

fn dynamic_scalar_get_by_key(
    solution: &PyDynamicSolution,
    entity_index: usize,
    variable_key: usize,
) -> Option<usize> {
    let (entity, variable) = decode_dynamic_scalar_key(variable_key);
    solution.get_scalar(entity, entity_index, variable)
}

fn dynamic_scalar_set_by_key(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    variable_key: usize,
    value: Option<usize>,
) {
    let (entity, variable) = decode_dynamic_scalar_key(variable_key);
    solution.set_scalar(entity, entity_index, variable, value);
}

fn dynamic_scalar_candidate_values_by_key(
    solution: &PyDynamicSolution,
    entity_index: usize,
    variable_key: usize,
) -> &[usize] {
    let (entity, variable) = decode_dynamic_scalar_key(variable_key);
    solution.candidate_values(entity, entity_index, variable)
}

fn dynamic_assignment_entity_count(solution: &PyDynamicSolution) -> usize {
    let group_name = active_dynamic_assignment_group();
    let Some(group) = solution.schema.assignment_scalar_group(&group_name) else {
        return 0;
    };
    solution
        .schema
        .entities
        .iter()
        .position(|entity| entity.type_name == group.entity_class)
        .map(|entity_index| solution.entity_count(entity_index))
        .unwrap_or(0)
}

fn dynamic_assignment_required_entity(solution: &PyDynamicSolution, entity_index: usize) -> bool {
    Python::attach(|py| -> PyResult<Option<bool>> {
        dynamic_assignment_bool_hook(py, solution, "required_entity", &[entity_index])
    })
    .unwrap_or_else(|error| panic!("dynamic assignment required_entity failed: {error}"))
    .unwrap_or(false)
}

fn dynamic_assignment_capacity_key(
    solution: &PyDynamicSolution,
    entity_index: usize,
    value: usize,
) -> Option<usize> {
    Python::attach(|py| -> PyResult<Option<usize>> {
        dynamic_assignment_optional_usize_hook(py, solution, "capacity_key", &[entity_index, value])
    })
    .unwrap_or_else(|error| panic!("dynamic assignment capacity_key failed: {error}"))
}

fn dynamic_assignment_position_key(solution: &PyDynamicSolution, entity_index: usize) -> i64 {
    Python::attach(|py| -> PyResult<Option<i64>> {
        dynamic_assignment_optional_i64_hook(py, solution, "position_key", &[entity_index])
    })
    .unwrap_or_else(|error| panic!("dynamic assignment position_key failed: {error}"))
    .unwrap_or(0)
}

fn dynamic_assignment_sequence_key(
    solution: &PyDynamicSolution,
    entity_index: usize,
    value: usize,
) -> Option<usize> {
    Python::attach(|py| -> PyResult<Option<usize>> {
        dynamic_assignment_optional_usize_hook(py, solution, "sequence_key", &[entity_index, value])
    })
    .unwrap_or_else(|error| panic!("dynamic assignment sequence_key failed: {error}"))
}

fn dynamic_assignment_entity_order(solution: &PyDynamicSolution, entity_index: usize) -> i64 {
    Python::attach(|py| -> PyResult<Option<i64>> {
        dynamic_assignment_optional_i64_hook(py, solution, "entity_order", &[entity_index])
    })
    .unwrap_or_else(|error| panic!("dynamic assignment entity_order failed: {error}"))
    .unwrap_or(0)
}

fn dynamic_assignment_value_order(
    solution: &PyDynamicSolution,
    entity_index: usize,
    value: usize,
) -> i64 {
    Python::attach(|py| -> PyResult<Option<i64>> {
        dynamic_assignment_optional_i64_hook(py, solution, "value_order", &[entity_index, value])
    })
    .unwrap_or_else(|error| panic!("dynamic assignment value_order failed: {error}"))
    .unwrap_or(0)
}

fn dynamic_assignment_rule(
    solution: &PyDynamicSolution,
    left_entity: usize,
    left_value: usize,
    right_entity: usize,
    right_value: usize,
) -> bool {
    Python::attach(|py| -> PyResult<Option<bool>> {
        dynamic_assignment_bool_hook(
            py,
            solution,
            "assignment_rule",
            &[left_entity, left_value, right_entity, right_value],
        )
    })
    .unwrap_or_else(|error| panic!("dynamic assignment assignment_rule failed: {error}"))
    .unwrap_or(true)
}

impl Debug for DynamicScalarSwapMove {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicScalarSwapMove")
            .field("descriptor_index", &self.slot.descriptor_index())
            .field("variable", &self.slot.variable)
            .field("left_entity_index", &self.left_entity_index)
            .field("right_entity_index", &self.right_entity_index)
            .finish()
    }
}

impl solverforge_solver::Move<PyDynamicSolution> for DynamicScalarSwapMove {
    type Undo = (Option<usize>, Option<usize>);

    fn is_doable<D: Director<PyDynamicSolution>>(&self, score_director: &D) -> bool {
        let solution = score_director.working_solution();
        let left = self.slot.current_value(solution, self.left_entity_index);
        let right = self.slot.current_value(solution, self.right_entity_index);
        left != right
            && self
                .slot
                .value_is_legal(solution, self.left_entity_index, right)
            && self
                .slot
                .value_is_legal(solution, self.right_entity_index, left)
    }

    fn do_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D) -> Self::Undo {
        let left = self
            .slot
            .current_value(score_director.working_solution(), self.left_entity_index);
        let right = self
            .slot
            .current_value(score_director.working_solution(), self.right_entity_index);
        let descriptor_index = self.slot.descriptor_index();
        score_director.before_variable_changed(descriptor_index, self.left_entity_index);
        score_director.before_variable_changed(descriptor_index, self.right_entity_index);
        self.slot.set_value(
            score_director.working_solution_mut(),
            self.left_entity_index,
            right,
        );
        self.slot.set_value(
            score_director.working_solution_mut(),
            self.right_entity_index,
            left,
        );
        score_director.after_variable_changed(descriptor_index, self.left_entity_index);
        score_director.after_variable_changed(descriptor_index, self.right_entity_index);
        (left, right)
    }

    fn undo_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D, undo: Self::Undo) {
        let descriptor_index = self.slot.descriptor_index();
        score_director.before_variable_changed(descriptor_index, self.left_entity_index);
        score_director.before_variable_changed(descriptor_index, self.right_entity_index);
        self.slot.set_value(
            score_director.working_solution_mut(),
            self.left_entity_index,
            undo.0,
        );
        self.slot.set_value(
            score_director.working_solution_mut(),
            self.right_entity_index,
            undo.1,
        );
        score_director.after_variable_changed(descriptor_index, self.left_entity_index);
        score_director.after_variable_changed(descriptor_index, self.right_entity_index);
    }

    fn descriptor_index(&self) -> usize {
        self.slot.descriptor_index()
    }

    fn entity_indices(&self) -> &[usize] {
        &self.entity_indices
    }

    fn variable_name(&self) -> &str {
        self.slot.variable_name
    }

    fn telemetry_label(&self) -> &'static str {
        "dynamic_scalar_swap"
    }

    fn tabu_signature<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> MoveTabuSignature {
        with_dynamic_scalar_slot(&self.slot, || {
            SwapMove::<PyDynamicSolution, usize>::new(
                self.left_entity_index,
                self.right_entity_index,
                dynamic_scalar_get,
                dynamic_scalar_set,
                self.slot.variable.0,
                self.slot.variable_name,
                self.slot.descriptor_index(),
            )
            .tabu_signature(score_director)
        })
    }
}

#[derive(Clone)]
pub struct DynamicScalarMoveSelector {
    selectors: Vec<DynamicScalarSelector>,
}

impl DynamicScalarMoveSelector {
    fn try_new(
        config: Option<&MoveSelectorConfig>,
        model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    ) -> Option<Self> {
        let mut selectors = Vec::new();
        if let Some(config) = config {
            collect_dynamic_selectors(config, model, &mut selectors);
        }
        (!selectors.is_empty()).then_some(Self { selectors })
    }

    fn moves(
        &self,
        solution: &PyDynamicSolution,
        context: MoveStreamContext,
    ) -> Vec<DynamicScalar> {
        self.selectors
            .iter()
            .flat_map(|selector| selector.moves(solution, context))
            .collect()
    }

    fn assignment_cursor<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
        context: MoveStreamContext,
    ) -> Option<DynamicAssignmentMoveCursor> {
        if self.selectors.len() != 1 {
            return None;
        }
        let DynamicScalarSelector::Grouped {
            group_name,
            scalar_slots,
            value_candidate_limit,
            max_moves_per_step,
            require_hard_improvement,
        } = &self.selectors[0]
        else {
            return None;
        };
        let solution = score_director.working_solution();
        solution.schema.assignment_scalar_group(group_name)?;
        DynamicAssignmentMoveCursor::try_new(
            score_director,
            context,
            group_name,
            scalar_slots,
            *value_candidate_limit,
            *max_moves_per_step,
            *require_hard_improvement,
        )
    }
}

impl Debug for DynamicScalarMoveSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicScalarMoveSelector")
            .field("selector_count", &self.selectors.len())
            .finish()
    }
}

#[derive(Clone)]
pub struct DynamicMoveSelector {
    selector: DynamicAnySelector,
}

impl DynamicMoveSelector {
    fn new(
        config: Option<&MoveSelectorConfig>,
        model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
        random_seed: Option<u64>,
    ) -> Self {
        let selector = DynamicAnySelector::try_new(config, model, random_seed)
            .expect("dynamic local search produced no bindable selectors");
        Self { selector }
    }
}

impl Debug for DynamicMoveSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicMoveSelector").finish()
    }
}

impl MoveSelector<PyDynamicSolution, DynamicMove> for DynamicMoveSelector {
    type Cursor<'a>
        = DynamicMoveCursor
    where
        Self: 'a;

    fn open_cursor<'a, D: Director<PyDynamicSolution>>(
        &'a self,
        score_director: &D,
    ) -> Self::Cursor<'a> {
        self.open_cursor_with_context(score_director, MoveStreamContext::default())
    }

    fn open_cursor_with_context<'a, D: Director<PyDynamicSolution>>(
        &'a self,
        score_director: &D,
        context: MoveStreamContext,
    ) -> Self::Cursor<'a> {
        if let Some(cursor) = self.selector.assignment_cursor(score_director, context) {
            return DynamicMoveCursor::Assignment(cursor);
        }
        DynamicMoveCursor::Arena(ArenaMoveCursor::from_moves(
            self.selector.moves(score_director, context),
        ))
    }

    fn size<D: Director<PyDynamicSolution>>(&self, score_director: &D) -> usize {
        let mut cursor = self.open_cursor(score_director);
        let mut count = 0;
        while cursor.next_candidate().is_some() {
            count += 1;
        }
        count
    }

    fn append_moves<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
        arena: &mut MoveArena<DynamicMove>,
    ) {
        let mut cursor = self.open_cursor(score_director);
        while let Some(id) = cursor.next_candidate() {
            arena.push(cursor.take_candidate(id));
        }
    }
}

#[derive(Debug)]
struct DynamicAssignmentGroupConfig {
    name: String,
    entity_class: String,
    variable_name: String,
    has_required_entity: bool,
    has_capacity_key: bool,
    has_assignment_rule: bool,
    has_position_key: bool,
    has_sequence_key: bool,
    has_entity_order: bool,
    has_value_order: bool,
    limits: ScalarGroupLimits,
}

fn dynamic_assignment_group_binding(
    solution: &PyDynamicSolution,
    group_name: &str,
    scalar_slots: &[DynamicScalarVariableSlot<PyDynamicSolution>],
) -> PyResult<ScalarGroupBinding<PyDynamicSolution>> {
    let config = load_dynamic_assignment_group_config(solution, group_name)?;
    if config.has_assignment_rule && !config.has_sequence_key {
        return Err(crate::error::py_err(format!(
            "assignment scalar group `{}` declares assignment_rule but no sequence_key",
            config.name
        )));
    }
    let slot = scalar_slots
        .iter()
        .find(|slot| {
            slot.matches_target(
                Some(config.entity_class.as_str()),
                Some(config.variable_name.as_str()),
            )
        })
        .ok_or_else(|| {
            crate::error::py_err(format!(
                "assignment scalar group `{}` targets unknown scalar variable `{}.{}`",
                config.name, config.entity_class, config.variable_name
            ))
        })?;
    if !slot.allows_unassigned {
        return Err(crate::error::py_err(format!(
            "assignment scalar group `{}` target `{}.{}` must allow unassigned values",
            config.name, config.entity_class, config.variable_name
        )));
    }

    let variable_key = dynamic_scalar_key(slot.entity, slot.variable);
    let member = ScalarGroupMemberBinding {
        descriptor_index: slot.descriptor_index(),
        variable_index: variable_key,
        entity_type_name: slot.entity_type_name,
        variable_name: slot.variable_name,
        getter: dynamic_scalar_get_by_key,
        setter: dynamic_scalar_set_by_key,
        value_source: ValueSource::EntitySlice {
            values_for_entity: dynamic_scalar_candidate_values_by_key,
        },
        entity_count: dynamic_assignment_entity_count,
        candidate_values: None,
        allows_unassigned: slot.allows_unassigned,
    };
    let assignment = ScalarAssignmentBinding {
        target: member,
        required_entity: config
            .has_required_entity
            .then_some(dynamic_assignment_required_entity as fn(&PyDynamicSolution, usize) -> bool),
        capacity_key: config.has_capacity_key.then_some(
            dynamic_assignment_capacity_key
                as fn(&PyDynamicSolution, usize, usize) -> Option<usize>,
        ),
        position_key: config
            .has_position_key
            .then_some(dynamic_assignment_position_key as fn(&PyDynamicSolution, usize) -> i64),
        sequence_key: config.has_sequence_key.then_some(
            dynamic_assignment_sequence_key
                as fn(&PyDynamicSolution, usize, usize) -> Option<usize>,
        ),
        entity_order: config
            .has_entity_order
            .then_some(dynamic_assignment_entity_order as fn(&PyDynamicSolution, usize) -> i64),
        value_order: config.has_value_order.then_some(
            dynamic_assignment_value_order as fn(&PyDynamicSolution, usize, usize) -> i64,
        ),
        assignment_rule: config.has_assignment_rule.then_some(
            dynamic_assignment_rule as fn(&PyDynamicSolution, usize, usize, usize, usize) -> bool,
        ),
    };
    Ok(ScalarGroupBinding {
        group_name: intern(config.name),
        members: vec![member],
        kind: ScalarGroupBindingKind::Assignment(assignment),
        limits: config.limits,
    })
}

fn load_dynamic_assignment_group_config(
    solution: &PyDynamicSolution,
    group_name: &str,
) -> PyResult<DynamicAssignmentGroupConfig> {
    let group = solution.schema.assignment_scalar_group(group_name).ok_or_else(|| {
        crate::error::py_err(format!(
            "grouped_scalar_move_selector configured for `{group_name}`, but no matching assignment scalar group was declared"
        ))
    })?;
    Ok(DynamicAssignmentGroupConfig {
        name: group.name.clone(),
        entity_class: group.entity_class.clone(),
        variable_name: group.variable_name.clone(),
        has_required_entity: group.required_entity.is_some(),
        has_capacity_key: group.capacity_key.is_some(),
        has_assignment_rule: group.assignment_rule.is_some(),
        has_position_key: group.position_key.is_some(),
        has_sequence_key: group.sequence_key.is_some(),
        has_entity_order: group.entity_order.is_some(),
        has_value_order: group.value_order.is_some(),
        limits: ScalarGroupLimits {
            value_candidate_limit: group.limits.value_candidate_limit,
            group_candidate_limit: group.limits.group_candidate_limit,
            max_moves_per_step: group.limits.max_moves_per_step,
            max_augmenting_depth: group.limits.max_augmenting_depth,
            max_rematch_size: group.limits.max_rematch_size,
        },
    })
}

pub struct DynamicAssignmentMoveCursor {
    group_name: String,
    inner: GroupedScalarCursor<PyDynamicSolution>,
    store: CandidateStore<PyDynamicSolution, DynamicMove>,
    next_index: usize,
}

impl DynamicAssignmentMoveCursor {
    #[allow(clippy::too_many_arguments)]
    fn try_new<D: Director<PyDynamicSolution>>(
        score_director: &D,
        context: MoveStreamContext,
        group_name: &str,
        scalar_slots: &[DynamicScalarVariableSlot<PyDynamicSolution>],
        value_candidate_limit: Option<usize>,
        max_moves_per_step: Option<usize>,
        require_hard_improvement: bool,
    ) -> Option<Self> {
        let solution = score_director.working_solution();
        let group = dynamic_assignment_group_binding(solution, group_name, scalar_slots)
            .unwrap_or_else(|error| panic!("dynamic assignment group binding failed: {error}"));
        let selector = GroupedScalarSelector::new(
            group,
            value_candidate_limit,
            max_moves_per_step,
            require_hard_improvement,
        );
        let inner = with_dynamic_assignment_group(group_name, || {
            selector.open_cursor_with_context(score_director, context)
        });
        Some(Self {
            group_name: group_name.to_string(),
            inner,
            store: CandidateStore::new(),
            next_index: 0,
        })
    }
}

impl MoveCursor<PyDynamicSolution, DynamicMove> for DynamicAssignmentMoveCursor {
    fn next_candidate(&mut self) -> Option<CandidateId> {
        if self.next_index < self.store.len() {
            let id = CandidateId::new(self.next_index);
            self.next_index += 1;
            return Some(id);
        }
        let inner_id =
            with_dynamic_assignment_group(&self.group_name, || self.inner.next_candidate())?;
        let scalar_move =
            with_dynamic_assignment_group(&self.group_name, || self.inner.take_candidate(inner_id));
        let id = self
            .store
            .push(DynamicMove::Scalar(DynamicScalar::Native(scalar_move)));
        self.next_index = id.index() + 1;
        Some(id)
    }

    fn candidate(
        &self,
        id: CandidateId,
    ) -> Option<MoveCandidateRef<'_, PyDynamicSolution, DynamicMove>> {
        self.store.candidate(id)
    }

    fn take_candidate(&mut self, id: CandidateId) -> DynamicMove {
        self.store.take_candidate(id)
    }
}

// Keep the concrete assignment cursor inline: this is local-search cursor state
// in the candidate loop, and the enum size tradeoff preserves zero-erasure flow.
#[allow(clippy::large_enum_variant)]
pub enum DynamicMoveCursor {
    Arena(ArenaMoveCursor<PyDynamicSolution, DynamicMove>),
    Assignment(DynamicAssignmentMoveCursor),
}

impl MoveCursor<PyDynamicSolution, DynamicMove> for DynamicMoveCursor {
    fn next_candidate(&mut self) -> Option<CandidateId> {
        match self {
            Self::Arena(cursor) => cursor.next_candidate(),
            Self::Assignment(cursor) => cursor.next_candidate(),
        }
    }

    fn candidate(
        &self,
        id: CandidateId,
    ) -> Option<MoveCandidateRef<'_, PyDynamicSolution, DynamicMove>> {
        match self {
            Self::Arena(cursor) => cursor.candidate(id),
            Self::Assignment(cursor) => cursor.candidate(id),
        }
    }

    fn take_candidate(&mut self, id: CandidateId) -> DynamicMove {
        match self {
            Self::Arena(cursor) => cursor.take_candidate(id),
            Self::Assignment(cursor) => cursor.take_candidate(id),
        }
    }
}

#[derive(Clone)]
enum DynamicAnySelector {
    Scalar(DynamicScalarMoveSelector),
    List(DynamicListMoveSelector),
    Union(Vec<DynamicAnySelector>),
    Limited {
        selected_count_limit: usize,
        selector: Box<DynamicAnySelector>,
    },
    Cartesian {
        left: Box<DynamicAnySelector>,
        right: Box<DynamicAnySelector>,
        require_hard_improvement: bool,
    },
}

impl DynamicAnySelector {
    fn try_new(
        config: Option<&MoveSelectorConfig>,
        model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
        random_seed: Option<u64>,
    ) -> Option<Self> {
        match config {
            Some(MoveSelectorConfig::LimitedNeighborhood(limit)) => {
                Self::try_new(Some(&limit.selector), model, random_seed).map(|selector| {
                    Self::Limited {
                        selected_count_limit: limit.selected_count_limit,
                        selector: Box::new(selector),
                    }
                })
            }
            Some(MoveSelectorConfig::UnionMoveSelector(union)) => Self::from_children(
                union
                    .selectors
                    .iter()
                    .filter_map(|child| Self::try_new(Some(child), model, random_seed))
                    .collect(),
            ),
            Some(MoveSelectorConfig::CartesianProductMoveSelector(cartesian)) => {
                assert_eq!(
                    cartesian.selectors.len(),
                    2,
                    "cartesian_product move selector requires exactly two child selectors"
                );
                let left = Self::try_new(Some(&cartesian.selectors[0]), model, random_seed)?;
                let right = Self::try_new(Some(&cartesian.selectors[1]), model, random_seed)?;
                Some(Self::Cartesian {
                    left: Box::new(left),
                    right: Box::new(right),
                    require_hard_improvement: cartesian.require_hard_improvement,
                })
            }
            other => {
                let mut children = Vec::new();
                if let Some(scalar) = DynamicScalarMoveSelector::try_new(other, model) {
                    children.push(Self::Scalar(scalar));
                }
                if let Some(list) = DynamicListMoveSelector::try_new(other, model, random_seed) {
                    children.push(Self::List(list));
                }
                Self::from_children(children)
            }
        }
    }

    fn from_children(children: Vec<Self>) -> Option<Self> {
        match children.len() {
            0 => None,
            1 => children.into_iter().next(),
            _ => Some(Self::Union(children)),
        }
    }

    fn moves<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
        context: MoveStreamContext,
    ) -> Vec<DynamicMove> {
        match self {
            Self::Scalar(selector) => {
                collect_dynamic_assignment_moves(selector, score_director, context).unwrap_or_else(
                    || {
                        selector
                            .moves(score_director.working_solution(), context)
                            .into_iter()
                            .map(DynamicMove::Scalar)
                            .collect()
                    },
                )
            }
            Self::List(selector) => selector
                .moves(score_director, context)
                .into_iter()
                .map(DynamicMove::List)
                .collect(),
            Self::Union(children) => children
                .iter()
                .flat_map(|child| child.moves(score_director, context))
                .collect(),
            Self::Limited {
                selected_count_limit,
                selector,
            } => {
                let mut moves = selector.moves(score_director, context);
                moves.truncate(*selected_count_limit);
                moves
            }
            Self::Cartesian {
                left,
                right,
                require_hard_improvement,
            } => cartesian_dynamic_moves(
                left,
                right,
                *require_hard_improvement,
                score_director,
                context,
            ),
        }
    }

    fn assignment_cursor<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
        context: MoveStreamContext,
    ) -> Option<DynamicAssignmentMoveCursor> {
        match self {
            Self::Scalar(selector) => selector.assignment_cursor(score_director, context),
            Self::List(_) | Self::Union(_) | Self::Limited { .. } | Self::Cartesian { .. } => None,
        }
    }
}

fn collect_dynamic_assignment_moves<D: Director<PyDynamicSolution>>(
    selector: &DynamicScalarMoveSelector,
    score_director: &D,
    context: MoveStreamContext,
) -> Option<Vec<DynamicMove>> {
    let mut cursor = selector.assignment_cursor(score_director, context)?;
    let mut moves = Vec::new();
    while let Some(id) = cursor.next_candidate() {
        moves.push(cursor.take_candidate(id));
    }
    Some(moves)
}

fn cartesian_dynamic_moves<D: Director<PyDynamicSolution>>(
    left: &DynamicAnySelector,
    right: &DynamicAnySelector,
    require_hard_improvement: bool,
    score_director: &D,
    context: MoveStreamContext,
) -> Vec<DynamicMove> {
    let left_moves = left.moves(score_director, context);
    let mut moves = Vec::new();
    for left_move in left_moves {
        if !left_move.is_doable(score_director) {
            continue;
        }
        let mut preview = DynamicPreviewDirector::from_director(score_director);
        let _undo = left_move.do_move(&mut preview);
        for right_move in right.moves(&preview, context) {
            if !right_move.is_doable(&preview) {
                continue;
            }
            let left_signature = left_move.tabu_signature(score_director);
            let right_signature = right_move.tabu_signature(&preview);
            let tabu_signature = compose_dynamic_cartesian_tabu(&left_signature, &right_signature);
            moves.push(DynamicMove::Cartesian(DynamicCartesianMove::new(
                left_move.clone(),
                right_move,
                tabu_signature,
                require_hard_improvement,
            )));
        }
    }
    moves
}

fn compose_dynamic_cartesian_tabu(
    left: &MoveTabuSignature,
    right: &MoveTabuSignature,
) -> MoveTabuSignature {
    let mut entity_tokens = left.entity_tokens.clone();
    for token in &right.entity_tokens {
        if !entity_tokens.contains(token) {
            entity_tokens.push(*token);
        }
    }

    let mut destination_value_tokens = left.destination_value_tokens.clone();
    for token in &right.destination_value_tokens {
        if !destination_value_tokens.contains(token) {
            destination_value_tokens.push(*token);
        }
    }

    let mut move_id = smallvec::smallvec![0xDCA7_0000_0000_0001];
    move_id.extend(left.move_id.iter().copied());
    move_id.extend(right.move_id.iter().copied());

    let mut undo_move_id = smallvec::smallvec![0xDCA7_0000_0000_0001];
    undo_move_id.extend(right.undo_move_id.iter().copied());
    undo_move_id.extend(left.undo_move_id.iter().copied());

    MoveTabuSignature {
        scope: left.scope,
        entity_tokens,
        destination_value_tokens,
        move_id,
        undo_move_id,
    }
}

type DynamicTypedListSlot =
    ListVariableSlot<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>;

#[derive(Clone, Copy)]
struct KOptMoveOptions {
    k: usize,
    min_segment_len: usize,
    max_nearby: usize,
}

#[derive(Clone, Copy)]
struct RuinRecreateOptions {
    min_ruin_count: usize,
    max_ruin_count: usize,
    moves_per_step: usize,
    value_candidate_limit: Option<usize>,
    recreate_heuristic_type: solverforge_config::RecreateHeuristicType,
}

thread_local! {
    static ACTIVE_DYNAMIC_LIST_SLOT: RefCell<Vec<DynamicListVariableSlot<PyDynamicSolution>>> =
        const { RefCell::new(Vec::new()) };
}

fn with_dynamic_list_slot<R>(
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    f: impl FnOnce() -> R,
) -> R {
    ACTIVE_DYNAMIC_LIST_SLOT.with(|stack| stack.borrow_mut().push(slot.clone()));
    let result = catch_unwind(AssertUnwindSafe(f));
    ACTIVE_DYNAMIC_LIST_SLOT.with(|stack| {
        let popped = stack.borrow_mut().pop();
        debug_assert!(popped.is_some(), "dynamic list slot stack underflow");
    });
    match result {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
}

fn publish_current_list_solution_if_not_worse<D, ProgressCb>(
    solver_scope: &mut solverforge_solver::SolverScope<'_, PyDynamicSolution, D, ProgressCb>,
) where
    D: Director<PyDynamicSolution>,
    ProgressCb: ProgressCallback<PyDynamicSolution>,
{
    let score = solver_scope.calculate_score();
    if solver_scope.best_score().is_none_or(|best| score >= *best) {
        let solution = solver_scope.working_solution().clone();
        solver_scope.set_best_solution(solution, score);
    }
}

fn active_dynamic_list_slot() -> DynamicListVariableSlot<PyDynamicSolution> {
    ACTIVE_DYNAMIC_LIST_SLOT.with(|stack| {
        stack
            .borrow()
            .last()
            .cloned()
            .expect("dynamic list operation used outside an active list slot")
    })
}

fn typed_dynamic_list_slot(
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    solution: &PyDynamicSolution,
) -> DynamicTypedListSlot {
    ListVariableSlot::new(
        slot.entity_type_name,
        dynamic_list_element_count,
        dynamic_list_assigned_elements,
        dynamic_list_len,
        dynamic_list_remove,
        dynamic_list_construction_remove,
        dynamic_list_insert,
        dynamic_list_get,
        dynamic_list_set,
        dynamic_list_reverse,
        dynamic_sublist_remove,
        dynamic_sublist_insert,
        dynamic_list_ruin_remove,
        dynamic_list_ruin_insert,
        dynamic_list_index_to_element,
        dynamic_list_entity_count,
        PyDistanceMeter,
        PyDistanceMeter,
        slot.variable_name,
        slot.descriptor_index(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .with_element_owner_fn(dynamic_element_owner_for_slot(
        slot,
        solution.schema.as_ref(),
    ))
    .with_construction_element_order_key(dynamic_construction_element_order_for_slot(
        slot,
        solution.schema.as_ref(),
    ))
    .with_precedence_hooks(
        dynamic_precedence_duration_for_slot(slot, solution.schema.as_ref()),
        dynamic_precedence_successors_for_slot(slot, solution.schema.as_ref()),
    )
}

fn dynamic_list_entity_count(solution: &PyDynamicSolution) -> usize {
    active_dynamic_list_slot().entity_count(solution)
}

fn dynamic_list_element_count(solution: &PyDynamicSolution) -> usize {
    active_dynamic_list_slot().element_count(solution)
}

fn dynamic_list_assigned_elements(solution: &PyDynamicSolution) -> Vec<usize> {
    active_dynamic_list_slot().assigned_elements(solution)
}

fn dynamic_list_len(solution: &PyDynamicSolution, entity_index: usize) -> usize {
    active_dynamic_list_slot().list_len(solution, entity_index)
}

fn dynamic_list_get(
    solution: &PyDynamicSolution,
    entity_index: usize,
    position: usize,
) -> Option<usize> {
    active_dynamic_list_slot().list_get(solution, entity_index, position)
}

fn dynamic_list_remove(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    position: usize,
) -> Option<usize> {
    active_dynamic_list_slot().list_remove(solution, entity_index, position)
}

fn dynamic_list_construction_remove(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    position: usize,
) -> usize {
    dynamic_list_remove(solution, entity_index, position)
        .expect("dynamic construction list remove position should be valid")
}

fn dynamic_list_insert(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    position: usize,
    value: usize,
) {
    active_dynamic_list_slot().list_insert(solution, entity_index, position, value);
}

fn dynamic_list_set(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    position: usize,
    value: usize,
) {
    let slot = active_dynamic_list_slot();
    if slot.list_remove(solution, entity_index, position).is_some() {
        slot.list_insert(solution, entity_index, position, value);
    }
}

fn dynamic_list_reverse(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    start: usize,
    end: usize,
) {
    let mut values = dynamic_sublist_remove(solution, entity_index, start, end);
    values.reverse();
    dynamic_sublist_insert(solution, entity_index, start, values);
}

fn dynamic_sublist_remove(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    start: usize,
    end: usize,
) -> Vec<usize> {
    let slot = active_dynamic_list_slot();
    let len = slot.list_len(solution, entity_index);
    let stop = end.min(len);
    if start >= stop {
        return Vec::new();
    }
    let mut removed = Vec::with_capacity(stop - start);
    for _ in start..stop {
        if let Some(value) = slot.list_remove(solution, entity_index, start) {
            removed.push(value);
        }
    }
    removed
}

fn dynamic_sublist_insert(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    position: usize,
    values: Vec<usize>,
) {
    let slot = active_dynamic_list_slot();
    for (offset, value) in values.into_iter().enumerate() {
        slot.list_insert(solution, entity_index, position + offset, value);
    }
}

fn dynamic_list_ruin_remove(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    position: usize,
) -> usize {
    dynamic_list_remove(solution, entity_index, position)
        .expect("dynamic list ruin remove position should be valid")
}

fn dynamic_list_ruin_insert(
    solution: &mut PyDynamicSolution,
    entity_index: usize,
    position: usize,
    value: usize,
) {
    dynamic_list_insert(solution, entity_index, position, value);
}

fn dynamic_list_index_to_element(solution: &PyDynamicSolution, element_index: usize) -> usize {
    active_dynamic_list_slot()
        .element(solution, element_index)
        .unwrap_or(element_index)
}

fn dynamic_route_get(solution: &PyDynamicSolution, entity_index: usize) -> Vec<usize> {
    let slot = active_dynamic_list_slot();
    solution
        .state
        .entities
        .get(slot.entity.0)
        .and_then(|rows| rows.get(entity_index))
        .and_then(|row| row.lists.get(slot.variable_name))
        .cloned()
        .unwrap_or_default()
}

fn dynamic_route_set(solution: &mut PyDynamicSolution, entity_index: usize, route: Vec<usize>) {
    let slot = active_dynamic_list_slot();
    if let Some(row) = solution
        .state
        .entities
        .get_mut(slot.entity.0)
        .and_then(|rows| rows.get_mut(entity_index))
    {
        row.lists.insert(slot.variable_name.to_string(), route);
    }
}

fn dynamic_route_depot(solution: &PyDynamicSolution, entity_index: usize) -> usize {
    let slot = active_dynamic_list_slot();
    if let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) {
        if let Some(field_name) = variable.route_depot_field.as_deref() {
            if let Some(value) =
                dynamic_route_usize_field(solution, &slot, entity_index, field_name)
            {
                return value;
            }
        }
    }
    Python::attach(|py| -> PyResult<Option<usize>> {
        let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) else {
            return Ok(None);
        };
        if let Some(callback) = variable.route_depot_entity.as_ref() {
            let route = dynamic_route_entity_view(py, solution, &slot, entity_index)?;
            return callback
                .bind(py)
                .call1((route,))?
                .extract::<usize>()
                .map(Some);
        }
        if let Some(callback) = variable.route_depot.as_ref() {
            let snapshot = solution.to_python_callback_view(py)?;
            return callback
                .bind(py)
                .call1((snapshot, entity_index))?
                .extract::<usize>()
                .map(Some);
        }
        Ok(None)
    })
    .unwrap_or_else(|error| panic!("dynamic route depot callback failed: {error}"))
    .unwrap_or_else(|| active_dynamic_list_slot().element_count(solution))
}

fn dynamic_route_metric_class(solution: &PyDynamicSolution, entity_index: usize) -> usize {
    let slot = active_dynamic_list_slot();
    if let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) {
        if let Some(field_name) = variable.route_metric_class_field.as_deref() {
            if let Some(value) =
                dynamic_route_usize_field(solution, &slot, entity_index, field_name)
            {
                return value;
            }
        }
    }
    Python::attach(|py| -> PyResult<Option<usize>> {
        let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) else {
            return Ok(None);
        };
        if let Some(callback) = variable.route_metric_class_entity.as_ref() {
            let route = dynamic_route_entity_view(py, solution, &slot, entity_index)?;
            return callback
                .bind(py)
                .call1((route,))?
                .extract::<usize>()
                .map(Some);
        }
        if let Some(callback) = variable.route_metric_class.as_ref() {
            let snapshot = solution.to_python_callback_view(py)?;
            return callback
                .bind(py)
                .call1((snapshot, entity_index))?
                .extract::<usize>()
                .map(Some);
        }
        Ok(None)
    })
    .unwrap_or_else(|error| panic!("dynamic route metric class callback failed: {error}"))
    .unwrap_or(entity_index)
}

fn dynamic_route_distance(
    solution: &PyDynamicSolution,
    entity_index: usize,
    from: usize,
    to: usize,
) -> i64 {
    let slot = active_dynamic_list_slot();
    if let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) {
        if let Some(field_name) = variable.route_distance_matrix_field.as_deref() {
            if let Some(value) =
                dynamic_route_matrix_distance(solution, &slot, entity_index, field_name, from, to)
            {
                return value;
            }
        }
    }
    Python::attach(|py| -> PyResult<Option<i64>> {
        let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) else {
            return Ok(None);
        };
        if let Some(callback) = variable.route_distance_entity.as_ref() {
            let route = dynamic_route_entity_view(py, solution, &slot, entity_index)?;
            return callback
                .bind(py)
                .call1((route, from, to))?
                .extract::<i64>()
                .map(Some);
        }
        if let Some(callback) = variable.route_distance.as_ref() {
            let snapshot = solution.to_python_callback_view(py)?;
            return callback
                .bind(py)
                .call1((snapshot, entity_index, from, to))?
                .extract::<i64>()
                .map(Some);
        }
        Ok(None)
    })
    .unwrap_or_else(|error| panic!("dynamic route distance callback failed: {error}"))
    .unwrap_or_else(|| from.abs_diff(to) as i64)
}

fn dynamic_route_feasible(
    solution: &PyDynamicSolution,
    entity_index: usize,
    route: &[usize],
) -> bool {
    let slot = active_dynamic_list_slot();
    if let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) {
        if let Some(value) =
            dynamic_route_capacity_feasible(solution, &slot, variable, entity_index, route)
        {
            return value;
        }
    }
    Python::attach(|py| -> PyResult<Option<bool>> {
        let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) else {
            return Ok(None);
        };
        if let Some(callback) = variable.route_feasible_entity.as_ref() {
            let entity = dynamic_route_entity_view(py, solution, &slot, entity_index)?;
            return callback
                .bind(py)
                .call1((entity, route.to_vec()))?
                .extract::<bool>()
                .map(Some);
        }
        if let Some(callback) = variable.route_feasible.as_ref() {
            let snapshot = solution.to_python_callback_view(py)?;
            return callback
                .bind(py)
                .call1((snapshot, entity_index, route.to_vec()))?
                .extract::<bool>()
                .map(Some);
        }
        Ok(None)
    })
    .unwrap_or_else(|error| panic!("dynamic route feasible callback failed: {error}"))
    .unwrap_or(true)
}

fn dynamic_route_row<'a>(
    solution: &'a PyDynamicSolution,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    entity_index: usize,
) -> Option<&'a DynamicEntityRow> {
    solution
        .state
        .entities
        .get(slot.entity.0)
        .and_then(|rows| rows.get(entity_index))
}

fn dynamic_route_usize_field(
    solution: &PyDynamicSolution,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    entity_index: usize,
    field_name: &str,
) -> Option<usize> {
    dynamic_route_i64_field(solution, slot, entity_index, field_name)
        .and_then(|value| usize::try_from(value).ok())
}

fn dynamic_route_i64_field(
    solution: &PyDynamicSolution,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    entity_index: usize,
    field_name: &str,
) -> Option<i64> {
    let row = dynamic_route_row(solution, slot, entity_index)?;
    dynamic_route_value(solution, row, field_name).and_then(dynamic_value_i64)
}

fn dynamic_route_matrix_distance(
    solution: &PyDynamicSolution,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    entity_index: usize,
    field_name: &str,
    from: usize,
    to: usize,
) -> Option<i64> {
    let row = dynamic_route_row(solution, slot, entity_index)?;
    dynamic_nested_list_i64(dynamic_route_value(solution, row, field_name)?, &[from, to])
}

fn dynamic_route_capacity_feasible(
    solution: &PyDynamicSolution,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    variable: &VariableSchema,
    entity_index: usize,
    route: &[usize],
) -> Option<bool> {
    let capacity_field = variable.route_capacity_field.as_deref()?;
    let demand_field = variable.route_demand_field.as_deref()?;
    let row = dynamic_route_row(solution, slot, entity_index)?;
    let capacity = dynamic_route_i64_field(solution, slot, entity_index, capacity_field)?;
    let demands = dynamic_route_value(solution, row, demand_field)?;
    let mut load = 0_i64;
    for element in route {
        load = load.checked_add(dynamic_nested_list_i64(demands, &[*element])?)?;
    }
    Some(load <= capacity)
}

fn dynamic_route_value<'a>(
    solution: &'a PyDynamicSolution,
    row: &'a DynamicEntityRow,
    field_name: &str,
) -> Option<&'a DynamicValue> {
    row.fields
        .get(field_name)
        .or_else(|| solution.state.solution_fields.get(field_name))
}

fn dynamic_nested_list_i64(value: &DynamicValue, path: &[usize]) -> Option<i64> {
    if path.is_empty() {
        return dynamic_value_i64(value);
    }
    let DynamicValue::List(values) = value else {
        return None;
    };
    dynamic_nested_list_i64(values.get(path[0])?, &path[1..])
}

fn dynamic_value_i64(value: &DynamicValue) -> Option<i64> {
    match value {
        DynamicValue::Int(value) => Some(*value),
        _ => None,
    }
}

fn dynamic_route_entity_view(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    entity_index: usize,
) -> PyResult<Py<PyAny>> {
    solution.entity_callback_view(py, slot.entity.0, entity_index)
}

fn dynamic_element_owner(solution: &PyDynamicSolution, element: &usize) -> Option<usize> {
    Python::attach(|py| -> PyResult<Option<usize>> {
        let slot = active_dynamic_list_slot();
        let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) else {
            return Ok(None);
        };
        let Some(callback) = variable.element_owner.as_ref() else {
            return Ok(None);
        };
        let snapshot = solution.to_python_callback_view(py)?;
        let result = callback.bind(py).call1((snapshot, *element))?;
        if result.is_none() {
            Ok(None)
        } else {
            result.extract::<usize>().map(Some)
        }
    })
    .unwrap_or_else(|error| panic!("dynamic element owner callback failed: {error}"))
}

fn dynamic_construction_element_order(solution: &PyDynamicSolution, element: usize) -> i64 {
    Python::attach(|py| -> PyResult<Option<i64>> {
        let slot = active_dynamic_list_slot();
        let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) else {
            return Ok(None);
        };
        let Some(callback) = variable.construction_element_order_key.as_ref() else {
            return Ok(None);
        };
        let snapshot = solution.to_python_callback_view(py)?;
        callback
            .bind(py)
            .call1((snapshot, element))?
            .extract::<i64>()
            .map(Some)
    })
    .unwrap_or_else(|error| panic!("dynamic list element order callback failed: {error}"))
    .unwrap_or(0)
}

fn dynamic_precedence_duration(solution: &PyDynamicSolution, element: usize) -> usize {
    Python::attach(|py| -> PyResult<Option<usize>> {
        let slot = active_dynamic_list_slot();
        let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) else {
            return Ok(None);
        };
        let Some(callback) = variable.precedence_duration.as_ref() else {
            return Ok(None);
        };
        let snapshot = solution.to_python_callback_view(py)?;
        callback
            .bind(py)
            .call1((snapshot, element))?
            .extract::<usize>()
            .map(Some)
    })
    .unwrap_or_else(|error| panic!("dynamic precedence duration callback failed: {error}"))
    .unwrap_or(0)
}

fn dynamic_precedence_successors(
    solution: &PyDynamicSolution,
    element: usize,
    successors: &mut Vec<usize>,
) {
    Python::attach(|py| -> PyResult<()> {
        let slot = active_dynamic_list_slot();
        let Some(variable) = schema_variable_for_slot(&solution.schema, &slot) else {
            return Ok(());
        };
        let Some(callback) = variable.precedence_successors.as_ref() else {
            return Ok(());
        };
        let snapshot = solution.to_python_callback_view(py)?;
        let result = callback.bind(py).call1((snapshot, element))?;
        if result.is_none() {
            return Ok(());
        }
        successors.extend(result.extract::<Vec<usize>>()?);
        Ok(())
    })
    .unwrap_or_else(|error| panic!("dynamic precedence successors callback failed: {error}"));
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum DynamicList {
    Change {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: ListChangeMove<PyDynamicSolution, usize>,
    },
    Swap {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: ListSwapMove<PyDynamicSolution, usize>,
    },
    MultiSwap {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: ListMultiSwapMove<PyDynamicSolution, usize>,
    },
    Permute {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: ListPermuteMove<PyDynamicSolution, usize>,
    },
    SublistChange {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: SublistChangeMove<PyDynamicSolution, usize>,
    },
    SublistSwap {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: SublistSwapMove<PyDynamicSolution, usize>,
    },
    Reverse {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: ListReverseMove<PyDynamicSolution, usize>,
    },
    KOpt {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: KOptMove<PyDynamicSolution, usize>,
    },
    Ruin {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: ListRuinMove<PyDynamicSolution, usize>,
    },
}

impl DynamicList {
    fn from_union(
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        mov: ListMoveUnion<PyDynamicSolution, usize>,
    ) -> Self {
        match mov {
            ListMoveUnion::ListChange(mov) => Self::Change { slot, mov },
            ListMoveUnion::ListSwap(mov) => Self::Swap { slot, mov },
            ListMoveUnion::ListMultiSwap(mov) => Self::MultiSwap { slot, mov },
            ListMoveUnion::ListPermute(mov) => Self::Permute { slot, mov },
            ListMoveUnion::SublistChange(mov) => Self::SublistChange { slot, mov },
            ListMoveUnion::SublistSwap(mov) => Self::SublistSwap { slot, mov },
            ListMoveUnion::ListReverse(mov) => Self::Reverse { slot, mov },
            ListMoveUnion::KOpt(mov) => Self::KOpt { slot, mov },
            ListMoveUnion::ListRuin(mov) => Self::Ruin { slot, mov },
        }
    }

    fn slot(&self) -> &DynamicListVariableSlot<PyDynamicSolution> {
        match self {
            Self::Change { slot, .. }
            | Self::Swap { slot, .. }
            | Self::MultiSwap { slot, .. }
            | Self::Permute { slot, .. }
            | Self::SublistChange { slot, .. }
            | Self::SublistSwap { slot, .. }
            | Self::Reverse { slot, .. }
            | Self::KOpt { slot, .. }
            | Self::Ruin { slot, .. } => slot,
        }
    }
}

impl Debug for DynamicList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Change { mov, .. } => Debug::fmt(mov, f),
            Self::Swap { mov, .. } => Debug::fmt(mov, f),
            Self::MultiSwap { mov, .. } => Debug::fmt(mov, f),
            Self::Permute { mov, .. } => Debug::fmt(mov, f),
            Self::SublistChange { mov, .. } => Debug::fmt(mov, f),
            Self::SublistSwap { mov, .. } => Debug::fmt(mov, f),
            Self::Reverse { mov, .. } => Debug::fmt(mov, f),
            Self::KOpt { mov, .. } => Debug::fmt(mov, f),
            Self::Ruin { mov, .. } => Debug::fmt(mov, f),
        }
    }
}

pub enum DynamicListUndo {
    Change(()),
    Swap(()),
    MultiSwap(()),
    Permute(Vec<usize>),
    SublistChange(()),
    SublistSwap(()),
    Reverse(()),
    KOpt(Vec<usize>),
    Ruin(smallvec::SmallVec<[(usize, usize, usize); 8]>),
}

impl Move<PyDynamicSolution> for DynamicList {
    type Undo = DynamicListUndo;

    fn is_doable<D: Director<PyDynamicSolution>>(&self, score_director: &D) -> bool {
        with_dynamic_list_slot(self.slot(), || match self {
            Self::Change { mov, .. } => mov.is_doable(score_director),
            Self::Swap { mov, .. } => mov.is_doable(score_director),
            Self::MultiSwap { mov, .. } => mov.is_doable(score_director),
            Self::Permute { mov, .. } => mov.is_doable(score_director),
            Self::SublistChange { mov, .. } => mov.is_doable(score_director),
            Self::SublistSwap { mov, .. } => mov.is_doable(score_director),
            Self::Reverse { mov, .. } => mov.is_doable(score_director),
            Self::KOpt { mov, .. } => mov.is_doable(score_director),
            Self::Ruin { mov, .. } => mov.is_doable(score_director),
        })
    }

    fn do_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D) -> Self::Undo {
        with_dynamic_list_slot(self.slot(), || match self {
            Self::Change { mov, .. } => {
                mov.do_move(score_director);
                DynamicListUndo::Change(())
            }
            Self::Swap { mov, .. } => {
                mov.do_move(score_director);
                DynamicListUndo::Swap(())
            }
            Self::MultiSwap { mov, .. } => {
                mov.do_move(score_director);
                DynamicListUndo::MultiSwap(())
            }
            Self::Permute { mov, .. } => DynamicListUndo::Permute(mov.do_move(score_director)),
            Self::SublistChange { mov, .. } => {
                mov.do_move(score_director);
                DynamicListUndo::SublistChange(())
            }
            Self::SublistSwap { mov, .. } => {
                mov.do_move(score_director);
                DynamicListUndo::SublistSwap(())
            }
            Self::Reverse { mov, .. } => {
                mov.do_move(score_director);
                DynamicListUndo::Reverse(())
            }
            Self::KOpt { mov, .. } => DynamicListUndo::KOpt(mov.do_move(score_director)),
            Self::Ruin { mov, .. } => DynamicListUndo::Ruin(mov.do_move(score_director)),
        })
    }

    fn undo_move<D: Director<PyDynamicSolution>>(&self, score_director: &mut D, undo: Self::Undo) {
        with_dynamic_list_slot(self.slot(), || match (self, undo) {
            (Self::Change { mov, .. }, DynamicListUndo::Change(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::Swap { mov, .. }, DynamicListUndo::Swap(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::MultiSwap { mov, .. }, DynamicListUndo::MultiSwap(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::Permute { mov, .. }, DynamicListUndo::Permute(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::SublistChange { mov, .. }, DynamicListUndo::SublistChange(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::SublistSwap { mov, .. }, DynamicListUndo::SublistSwap(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::Reverse { mov, .. }, DynamicListUndo::Reverse(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::KOpt { mov, .. }, DynamicListUndo::KOpt(undo)) => {
                mov.undo_move(score_director, undo);
            }
            (Self::Ruin { mov, .. }, DynamicListUndo::Ruin(undo)) => {
                mov.undo_move(score_director, undo);
            }
            _ => panic!("dynamic list undo variant mismatch"),
        });
    }

    fn descriptor_index(&self) -> usize {
        match self {
            Self::Change { mov, .. } => mov.descriptor_index(),
            Self::Swap { mov, .. } => mov.descriptor_index(),
            Self::MultiSwap { mov, .. } => mov.descriptor_index(),
            Self::Permute { mov, .. } => mov.descriptor_index(),
            Self::SublistChange { mov, .. } => mov.descriptor_index(),
            Self::SublistSwap { mov, .. } => mov.descriptor_index(),
            Self::Reverse { mov, .. } => mov.descriptor_index(),
            Self::KOpt { mov, .. } => mov.descriptor_index(),
            Self::Ruin { mov, .. } => mov.descriptor_index(),
        }
    }

    fn entity_indices(&self) -> &[usize] {
        match self {
            Self::Change { mov, .. } => mov.entity_indices(),
            Self::Swap { mov, .. } => mov.entity_indices(),
            Self::MultiSwap { mov, .. } => mov.entity_indices(),
            Self::Permute { mov, .. } => mov.entity_indices(),
            Self::SublistChange { mov, .. } => mov.entity_indices(),
            Self::SublistSwap { mov, .. } => mov.entity_indices(),
            Self::Reverse { mov, .. } => mov.entity_indices(),
            Self::KOpt { mov, .. } => mov.entity_indices(),
            Self::Ruin { mov, .. } => mov.entity_indices(),
        }
    }

    fn variable_name(&self) -> &str {
        match self {
            Self::Change { mov, .. } => mov.variable_name(),
            Self::Swap { mov, .. } => mov.variable_name(),
            Self::MultiSwap { mov, .. } => mov.variable_name(),
            Self::Permute { mov, .. } => mov.variable_name(),
            Self::SublistChange { mov, .. } => mov.variable_name(),
            Self::SublistSwap { mov, .. } => mov.variable_name(),
            Self::Reverse { mov, .. } => mov.variable_name(),
            Self::KOpt { mov, .. } => mov.variable_name(),
            Self::Ruin { mov, .. } => mov.variable_name(),
        }
    }

    fn telemetry_label(&self) -> &'static str {
        match self {
            Self::Change { .. } => "dynamic_list_change",
            Self::Swap { .. } => "dynamic_list_swap",
            Self::MultiSwap { .. } => "dynamic_list_multi_swap",
            Self::Permute { .. } => "dynamic_list_permute",
            Self::SublistChange { .. } => "dynamic_list_sublist_change",
            Self::SublistSwap { .. } => "dynamic_list_sublist_swap",
            Self::Reverse { .. } => "dynamic_list_reverse",
            Self::KOpt { .. } => "dynamic_list_k_opt",
            Self::Ruin { .. } => "dynamic_list_ruin",
        }
    }

    fn tabu_signature<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
    ) -> MoveTabuSignature {
        with_dynamic_list_slot(self.slot(), || match self {
            Self::Change { mov, .. } => mov.tabu_signature(score_director),
            Self::Swap { mov, .. } => mov.tabu_signature(score_director),
            Self::MultiSwap { mov, .. } => mov.tabu_signature(score_director),
            Self::Permute { mov, .. } => mov.tabu_signature(score_director),
            Self::SublistChange { mov, .. } => mov.tabu_signature(score_director),
            Self::SublistSwap { mov, .. } => mov.tabu_signature(score_director),
            Self::Reverse { mov, .. } => mov.tabu_signature(score_director),
            Self::KOpt { mov, .. } => mov.tabu_signature(score_director),
            Self::Ruin { mov, .. } => mov.tabu_signature(score_director),
        })
    }
}

#[derive(Clone)]
pub struct DynamicListMoveSelector {
    selector: DynamicListSelector,
}

impl DynamicListMoveSelector {
    fn try_new(
        config: Option<&MoveSelectorConfig>,
        model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
        random_seed: Option<u64>,
    ) -> Option<Self> {
        match config {
            Some(config) => build_dynamic_list_selector(config, model, random_seed)
                .map(|selector| Self { selector }),
            None => {
                let leaves = model
                    .dynamic_list_variables()
                    .cloned()
                    .map(|slot| DynamicListSelector::Leaf {
                        slot,
                        config: None,
                        random_seed,
                    })
                    .collect::<Vec<_>>();
                DynamicListSelector::from_children(leaves).map(|selector| Self { selector })
            }
        }
    }

    fn moves<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
        context: MoveStreamContext,
    ) -> Vec<DynamicList> {
        self.selector.moves(score_director, context)
    }
}

impl Debug for DynamicListMoveSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicListMoveSelector").finish()
    }
}

#[derive(Clone)]
enum DynamicListSelector {
    Leaf {
        slot: DynamicListVariableSlot<PyDynamicSolution>,
        config: Option<MoveSelectorConfig>,
        random_seed: Option<u64>,
    },
    Union(Vec<DynamicListSelector>),
    Limited {
        selected_count_limit: usize,
        selector: Box<DynamicListSelector>,
    },
}

impl DynamicListSelector {
    fn from_children(children: Vec<Self>) -> Option<Self> {
        match children.len() {
            0 => None,
            1 => children.into_iter().next(),
            _ => Some(Self::Union(children)),
        }
    }

    fn moves<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
        context: MoveStreamContext,
    ) -> Vec<DynamicList> {
        match self {
            Self::Leaf {
                slot,
                config,
                random_seed,
            } => dynamic_list_leaf_moves(
                slot,
                config.as_ref(),
                *random_seed,
                score_director,
                context,
            ),
            Self::Union(children) => children
                .iter()
                .flat_map(|child| child.moves(score_director, context))
                .collect(),
            Self::Limited {
                selected_count_limit,
                selector,
            } => {
                let mut moves = selector.moves(score_director, context);
                moves.truncate(*selected_count_limit);
                moves
            }
        }
    }
}

fn dynamic_list_leaf_moves<D: Director<PyDynamicSolution>>(
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    config: Option<&MoveSelectorConfig>,
    random_seed: Option<u64>,
    score_director: &D,
    context: MoveStreamContext,
) -> Vec<DynamicList> {
    with_dynamic_list_slot(slot, || {
        let typed_slot = typed_dynamic_list_slot(slot, score_director.working_solution());
        let mut moves = Vec::new();

        match config {
            None => {
                append_nearby_list_change_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    20,
                    score_director,
                    context,
                );
                append_nearby_list_swap_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    20,
                    score_director,
                    context,
                );
                append_list_reverse_moves(&mut moves, slot, &typed_slot, score_director, context);
            }
            Some(MoveSelectorConfig::ListChangeMoveSelector(_)) => {
                append_list_change_moves(&mut moves, slot, &typed_slot, score_director, context);
            }
            Some(MoveSelectorConfig::NearbyListChangeMoveSelector(config)) => {
                append_nearby_list_change_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    config.max_nearby,
                    score_director,
                    context,
                );
            }
            Some(MoveSelectorConfig::ListSwapMoveSelector(_)) => {
                append_list_swap_moves(&mut moves, slot, &typed_slot, score_director, context);
            }
            Some(MoveSelectorConfig::NearbyListSwapMoveSelector(config)) => {
                append_nearby_list_swap_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    config.max_nearby,
                    score_director,
                    context,
                );
            }
            Some(MoveSelectorConfig::SublistChangeMoveSelector(config)) => {
                append_sublist_change_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    config.min_sublist_size,
                    config.max_sublist_size,
                    score_director,
                    context,
                );
            }
            Some(MoveSelectorConfig::SublistSwapMoveSelector(config)) => {
                append_sublist_swap_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    config.min_sublist_size,
                    config.max_sublist_size,
                    score_director,
                    context,
                );
            }
            Some(MoveSelectorConfig::ListReverseMoveSelector(_)) => {
                append_list_reverse_moves(&mut moves, slot, &typed_slot, score_director, context);
            }
            Some(MoveSelectorConfig::ListPermuteMoveSelector(config)) => {
                append_list_permute_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    config.min_window_size,
                    config.max_window_size,
                    score_director,
                    context,
                );
            }
            Some(MoveSelectorConfig::ListPrecedenceMoveSelector(_)) => {
                append_list_precedence_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    score_director,
                    context,
                );
            }
            Some(MoveSelectorConfig::KOptMoveSelector(config)) => {
                append_k_opt_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    KOptMoveOptions {
                        k: config.k,
                        min_segment_len: config.min_segment_len,
                        max_nearby: config.max_nearby,
                    },
                    score_director,
                    context,
                );
            }
            Some(MoveSelectorConfig::ListRuinMoveSelector(config)) => {
                append_list_ruin_moves(
                    &mut moves,
                    slot,
                    &typed_slot,
                    config.min_ruin_count,
                    config.max_ruin_count,
                    config.moves_per_step,
                    config.max_source_list_len,
                    config.skip_empty_destinations,
                    random_seed,
                    score_director,
                    context,
                );
            }
            Some(_) => {}
        }
        moves
    })
}

fn collect_wrapped_list_moves<M, S, D>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    selector: &S,
    score_director: &D,
    context: MoveStreamContext,
    wrap: impl Fn(M) -> ListMoveUnion<PyDynamicSolution, usize>,
) where
    M: Move<PyDynamicSolution>,
    S: MoveSelector<PyDynamicSolution, M>,
    D: Director<PyDynamicSolution>,
{
    let mut cursor = selector.open_cursor_with_context(score_director, context);
    while let Some(id) = cursor.next_candidate() {
        out.push(DynamicList::from_union(
            slot.clone(),
            wrap(cursor.take_candidate(id)),
        ));
    }
}

fn append_nearby_list_change_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    max_nearby: usize,
    score_director: &D,
    context: MoveStreamContext,
) {
    let selector = NearbyListChangeMoveSelector::new(
        FromSolutionEntitySelector::new(ctx.descriptor_index),
        ctx.cross_distance_meter,
        max_nearby,
        ctx.list_len,
        ctx.list_get,
        ctx.list_remove,
        ctx.list_insert,
        ctx.variable_name,
        ctx.descriptor_index,
    )
    .with_element_owner_fn(ctx.element_owner_fn);
    collect_wrapped_list_moves(
        out,
        slot,
        &selector,
        score_director,
        context,
        ListMoveUnion::ListChange,
    );
}

fn append_nearby_list_swap_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    max_nearby: usize,
    score_director: &D,
    context: MoveStreamContext,
) {
    let selector = NearbyListSwapMoveSelector::new(
        FromSolutionEntitySelector::new(ctx.descriptor_index),
        ctx.cross_distance_meter,
        max_nearby,
        ctx.list_len,
        ctx.list_get,
        ctx.list_set,
        ctx.variable_name,
        ctx.descriptor_index,
    )
    .with_element_owner_fn(ctx.element_owner_fn);
    collect_wrapped_list_moves(
        out,
        slot,
        &selector,
        score_director,
        context,
        ListMoveUnion::ListSwap,
    );
}

fn append_list_reverse_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    score_director: &D,
    context: MoveStreamContext,
) {
    let selector = ListReverseMoveSelector::new(
        FromSolutionEntitySelector::new(ctx.descriptor_index),
        ctx.list_len,
        ctx.list_get,
        ctx.list_reverse,
        ctx.variable_name,
        ctx.descriptor_index,
    );
    collect_wrapped_list_moves(
        out,
        slot,
        &selector,
        score_director,
        context,
        ListMoveUnion::ListReverse,
    );
}

fn append_list_permute_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    min_window_size: usize,
    max_window_size: usize,
    score_director: &D,
    context: MoveStreamContext,
) {
    let selector = ListPermuteMoveSelector::new(
        FromSolutionEntitySelector::new(ctx.descriptor_index),
        min_window_size,
        max_window_size,
        ctx.list_len,
        ctx.list_get,
        ctx.sublist_remove,
        ctx.sublist_insert,
        ctx.variable_name,
        ctx.descriptor_index,
    )
    .with_element_owner_fn(ctx.element_owner_fn);
    collect_wrapped_list_moves(
        out,
        slot,
        &selector,
        score_director,
        context,
        ListMoveUnion::ListPermute,
    );
}

fn append_list_precedence_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    score_director: &D,
    context: MoveStreamContext,
) {
    let (Some(duration_fn), Some(successors_fn)) =
        (ctx.precedence_duration_fn, ctx.precedence_successors_fn)
    else {
        return;
    };
    let selector = ListPrecedenceMoveSelector::new(
        FromSolutionEntitySelector::new(ctx.descriptor_index),
        ctx.element_count,
        ctx.index_to_element,
        duration_fn,
        successors_fn,
        ctx.entity_count,
        ctx.list_len,
        ctx.list_get,
        ctx.list_remove,
        ctx.list_insert,
        ctx.list_set,
        ctx.list_reverse,
        ctx.ruin_remove,
        ctx.ruin_insert,
        ctx.element_owner_fn,
        ctx.sublist_remove,
        ctx.sublist_insert,
        ctx.variable_name,
        ctx.descriptor_index,
    );
    collect_wrapped_list_moves(out, slot, &selector, score_director, context, |mov| mov);
}

fn append_sublist_change_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    min_sublist_size: usize,
    max_sublist_size: usize,
    score_director: &D,
    context: MoveStreamContext,
) {
    let selector = SublistChangeMoveSelector::new(
        FromSolutionEntitySelector::new(ctx.descriptor_index),
        min_sublist_size,
        max_sublist_size,
        ctx.list_len,
        ctx.list_get,
        ctx.sublist_remove,
        ctx.sublist_insert,
        ctx.variable_name,
        ctx.descriptor_index,
    )
    .with_element_owner_fn(ctx.element_owner_fn);
    collect_wrapped_list_moves(
        out,
        slot,
        &selector,
        score_director,
        context,
        ListMoveUnion::SublistChange,
    );
}

fn append_sublist_swap_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    min_sublist_size: usize,
    max_sublist_size: usize,
    score_director: &D,
    context: MoveStreamContext,
) {
    let selector = SublistSwapMoveSelector::new(
        FromSolutionEntitySelector::new(ctx.descriptor_index),
        min_sublist_size,
        max_sublist_size,
        ctx.list_len,
        ctx.list_get,
        ctx.sublist_remove,
        ctx.sublist_insert,
        ctx.variable_name,
        ctx.descriptor_index,
    )
    .with_element_owner_fn(ctx.element_owner_fn);
    collect_wrapped_list_moves(
        out,
        slot,
        &selector,
        score_director,
        context,
        ListMoveUnion::SublistSwap,
    );
}

fn append_k_opt_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    options: KOptMoveOptions,
    score_director: &D,
    context: MoveStreamContext,
) {
    let config =
        KOptConfig::new(options.k.clamp(2, 5)).with_min_segment_len(options.min_segment_len);
    if options.max_nearby > 0 {
        let selector = NearbyKOptMoveSelector::new(
            FromSolutionEntitySelector::new(ctx.descriptor_index),
            IntraDistanceAdapter(ctx.intra_distance_meter),
            options.max_nearby,
            config,
            ctx.list_len,
            ctx.list_get,
            ctx.sublist_remove,
            ctx.sublist_insert,
            ctx.variable_name,
            ctx.descriptor_index,
        );
        collect_wrapped_list_moves(
            out,
            slot,
            &selector,
            score_director,
            context,
            ListMoveUnion::KOpt,
        );
    } else {
        let selector = KOptMoveSelector::new(
            FromSolutionEntitySelector::new(ctx.descriptor_index),
            config,
            ctx.list_len,
            ctx.list_get,
            ctx.sublist_remove,
            ctx.sublist_insert,
            ctx.variable_name,
            ctx.descriptor_index,
        );
        collect_wrapped_list_moves(
            out,
            slot,
            &selector,
            score_director,
            context,
            ListMoveUnion::KOpt,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn append_list_ruin_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    min_ruin_count: usize,
    max_ruin_count: usize,
    moves_per_step: Option<usize>,
    max_source_list_len: Option<usize>,
    skip_empty_destinations: bool,
    random_seed: Option<u64>,
    score_director: &D,
    context: MoveStreamContext,
) {
    let selector = ListRuinMoveSelector::new(
        min_ruin_count,
        max_ruin_count,
        ctx.entity_count,
        ctx.list_len,
        ctx.list_get,
        ctx.ruin_remove,
        ctx.ruin_insert,
        ctx.variable_name,
        ctx.descriptor_index,
    )
    .with_element_owner_fn(ctx.element_owner_fn)
    .with_moves_per_step(moves_per_step.unwrap_or(10).max(1))
    .with_max_source_list_len(max_source_list_len)
    .with_skip_empty_destinations(skip_empty_destinations);
    let selector = if let Some(seed) = random_seed {
        selector.with_seed(seed)
    } else {
        selector
    };
    collect_wrapped_list_moves(
        out,
        slot,
        &selector,
        score_director,
        context,
        ListMoveUnion::ListRuin,
    );
}

fn append_list_change_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    score_director: &D,
    context: MoveStreamContext,
) {
    let selector = ListChangeMoveSelector::new(
        FromSolutionEntitySelector::new(ctx.descriptor_index),
        ctx.list_len,
        ctx.list_get,
        ctx.list_remove,
        ctx.list_insert,
        ctx.variable_name,
        ctx.descriptor_index,
    )
    .with_element_owner_fn(ctx.element_owner_fn);
    collect_wrapped_list_moves(
        out,
        slot,
        &selector,
        score_director,
        context,
        ListMoveUnion::ListChange,
    );
}

fn append_list_swap_moves<D: Director<PyDynamicSolution>>(
    out: &mut Vec<DynamicList>,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    ctx: &DynamicTypedListSlot,
    score_director: &D,
    context: MoveStreamContext,
) {
    let selector = ListSwapMoveSelector::new(
        FromSolutionEntitySelector::new(ctx.descriptor_index),
        ctx.list_len,
        ctx.list_get,
        ctx.list_set,
        ctx.variable_name,
        ctx.descriptor_index,
    )
    .with_element_owner_fn(ctx.element_owner_fn);
    collect_wrapped_list_moves(
        out,
        slot,
        &selector,
        score_director,
        context,
        ListMoveUnion::ListSwap,
    );
}

fn build_dynamic_list_selector(
    config: &MoveSelectorConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    random_seed: Option<u64>,
) -> Option<DynamicListSelector> {
    match config {
        MoveSelectorConfig::LimitedNeighborhood(limit) => {
            build_dynamic_list_selector(&limit.selector, model, random_seed).map(|selector| {
                DynamicListSelector::Limited {
                    selected_count_limit: limit.selected_count_limit,
                    selector: Box::new(selector),
                }
            })
        }
        MoveSelectorConfig::UnionMoveSelector(union) => DynamicListSelector::from_children(
            union
                .selectors
                .iter()
                .filter_map(|child| build_dynamic_list_selector(child, model, random_seed))
                .collect(),
        ),
        MoveSelectorConfig::CartesianProductMoveSelector(_) => {
            let leaves = model
                .dynamic_list_variables()
                .filter(|slot| list_selector_matches_slot(config, slot))
                .cloned()
                .map(|slot| DynamicListSelector::Leaf {
                    slot,
                    config: Some(config.clone()),
                    random_seed,
                })
                .collect::<Vec<_>>();
            DynamicListSelector::from_children(leaves)
        }
        _ if list_selector_target(config).is_some() => {
            let target = list_selector_target(config).expect("list selector target checked");
            let leaves = matching_list_slots(model, target)
                .into_iter()
                .map(|slot| DynamicListSelector::Leaf {
                    slot,
                    config: Some(config.clone()),
                    random_seed,
                })
                .collect::<Vec<_>>();
            DynamicListSelector::from_children(leaves)
        }
        _ => None,
    }
}

fn list_selector_target(config: &MoveSelectorConfig) -> Option<&VariableTargetConfig> {
    match config {
        MoveSelectorConfig::ListChangeMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::NearbyListChangeMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::ListSwapMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::NearbyListSwapMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::SublistChangeMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::SublistSwapMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::ListReverseMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::ListPermuteMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::ListPrecedenceMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::KOptMoveSelector(config) => Some(&config.target),
        MoveSelectorConfig::ListRuinMoveSelector(config) => Some(&config.target),
        _ => None,
    }
}

fn list_selector_matches_slot(
    config: &MoveSelectorConfig,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
) -> bool {
    match config {
        MoveSelectorConfig::LimitedNeighborhood(limit) => {
            list_selector_matches_slot(&limit.selector, slot)
        }
        MoveSelectorConfig::UnionMoveSelector(union) => union
            .selectors
            .iter()
            .all(|child| list_selector_matches_slot(child, slot)),
        MoveSelectorConfig::CartesianProductMoveSelector(cartesian) => cartesian
            .selectors
            .iter()
            .all(|child| list_selector_matches_slot(child, slot)),
        _ => list_selector_target(config).is_some_and(|target| {
            slot.matches_target(
                target.entity_class.as_deref(),
                target.variable_name.as_deref(),
            )
        }),
    }
}

fn matching_list_slots(
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    target: &VariableTargetConfig,
) -> Vec<DynamicListVariableSlot<PyDynamicSolution>> {
    model
        .dynamic_list_variables()
        .filter(|slot| {
            slot.matches_target(
                target.entity_class.as_deref(),
                target.variable_name.as_deref(),
            )
        })
        .cloned()
        .collect()
}

fn matching_route_list_slots(
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    target: &VariableTargetConfig,
    schema: &DynamicSchema,
) -> Vec<DynamicListVariableSlot<PyDynamicSolution>> {
    matching_list_slots(model, target)
        .into_iter()
        .filter(|slot| {
            schema_variable_for_slot(schema, slot).is_some_and(|variable| {
                variable.route_distance.is_some()
                    || variable.route_distance_entity.is_some()
                    || variable.route_distance_matrix_field.is_some()
            })
        })
        .collect()
}

fn dynamic_element_owner_for_slot(
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    schema: &DynamicSchema,
) -> Option<fn(&PyDynamicSolution, &usize) -> Option<usize>> {
    schema_variable_for_slot(schema, slot)
        .and_then(|variable| variable.element_owner.as_ref())
        .map(|_| dynamic_element_owner as fn(&PyDynamicSolution, &usize) -> Option<usize>)
}

fn dynamic_construction_element_order_for_slot(
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    schema: &DynamicSchema,
) -> Option<fn(&PyDynamicSolution, usize) -> i64> {
    schema_variable_for_slot(schema, slot)
        .and_then(|variable| variable.construction_element_order_key.as_ref())
        .map(|_| dynamic_construction_element_order as fn(&PyDynamicSolution, usize) -> i64)
}

fn dynamic_precedence_duration_for_slot(
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    schema: &DynamicSchema,
) -> Option<fn(&PyDynamicSolution, usize) -> usize> {
    schema_variable_for_slot(schema, slot)
        .and_then(|variable| variable.precedence_duration.as_ref())
        .map(|_| dynamic_precedence_duration as fn(&PyDynamicSolution, usize) -> usize)
}

fn dynamic_precedence_successors_for_slot(
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
    schema: &DynamicSchema,
) -> Option<fn(&PyDynamicSolution, usize, &mut Vec<usize>)> {
    schema_variable_for_slot(schema, slot)
        .and_then(|variable| variable.precedence_successors.as_ref())
        .map(|_| dynamic_precedence_successors as fn(&PyDynamicSolution, usize, &mut Vec<usize>))
}

fn schema_variable_for_slot<'a>(
    schema: &'a DynamicSchema,
    slot: &DynamicListVariableSlot<PyDynamicSolution>,
) -> Option<&'a VariableSchema> {
    schema
        .entities
        .get(slot.entity.0)?
        .variables
        .get(slot.variable.0)
}

impl MoveSelector<PyDynamicSolution, DynamicScalar> for DynamicScalarMoveSelector {
    type Cursor<'a>
        = ArenaMoveCursor<PyDynamicSolution, DynamicScalar>
    where
        Self: 'a;

    fn open_cursor<'a, D: Director<PyDynamicSolution>>(
        &'a self,
        score_director: &D,
    ) -> Self::Cursor<'a> {
        self.open_cursor_with_context(score_director, MoveStreamContext::default())
    }

    fn open_cursor_with_context<'a, D: Director<PyDynamicSolution>>(
        &'a self,
        score_director: &D,
        context: MoveStreamContext,
    ) -> Self::Cursor<'a> {
        let solution = score_director.working_solution();
        ArenaMoveCursor::from_moves(
            self.selectors
                .iter()
                .flat_map(|selector| selector.moves(solution, context)),
        )
    }

    fn size<D: Director<PyDynamicSolution>>(&self, score_director: &D) -> usize {
        self.open_cursor(score_director).count()
    }

    fn append_moves<D: Director<PyDynamicSolution>>(
        &self,
        score_director: &D,
        arena: &mut MoveArena<DynamicScalar>,
    ) {
        let mut cursor = self.open_cursor(score_director);
        while let Some(id) = cursor.next_candidate() {
            arena.push(cursor.take_candidate(id));
        }
    }
}

#[derive(Clone)]
enum DynamicScalarSelector {
    Change {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        value_candidate_limit: Option<usize>,
    },
    NearbyChange {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        max_nearby: usize,
        value_candidate_limit: Option<usize>,
    },
    Swap {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
    },
    NearbySwap {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        max_nearby: usize,
    },
    PillarChange {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        minimum_sub_pillar_size: usize,
        maximum_sub_pillar_size: usize,
        value_candidate_limit: Option<usize>,
    },
    PillarSwap {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        minimum_sub_pillar_size: usize,
        maximum_sub_pillar_size: usize,
    },
    RuinRecreate {
        slot: DynamicScalarVariableSlot<PyDynamicSolution>,
        min_ruin_count: usize,
        max_ruin_count: usize,
        moves_per_step: usize,
        value_candidate_limit: Option<usize>,
        recreate_heuristic_type: solverforge_config::RecreateHeuristicType,
    },
    Grouped {
        group_name: String,
        scalar_slots: Vec<DynamicScalarVariableSlot<PyDynamicSolution>>,
        value_candidate_limit: Option<usize>,
        max_moves_per_step: Option<usize>,
        require_hard_improvement: bool,
    },
    ConflictRepair {
        constraint_names: Vec<String>,
        scalar_slots: Vec<DynamicScalarVariableSlot<PyDynamicSolution>>,
        max_matches_per_step: usize,
        max_repairs_per_match: usize,
        max_moves_per_step: usize,
        require_hard_improvement: bool,
        include_soft_matches: bool,
    },
    CompoundConflictRepair {
        constraint_names: Vec<String>,
        scalar_slots: Vec<DynamicScalarVariableSlot<PyDynamicSolution>>,
        max_matches_per_step: usize,
        max_repairs_per_match: usize,
        max_moves_per_step: usize,
        require_hard_improvement: bool,
        include_soft_matches: bool,
    },
}

impl DynamicScalarSelector {
    fn moves(
        &self,
        solution: &PyDynamicSolution,
        context: MoveStreamContext,
    ) -> Vec<DynamicScalar> {
        match self {
            Self::Change {
                slot,
                value_candidate_limit,
            } => change_moves(solution, slot, *value_candidate_limit, context),
            Self::NearbyChange {
                slot,
                max_nearby,
                value_candidate_limit,
            } => nearby_change_moves(solution, slot, *max_nearby, *value_candidate_limit, context),
            Self::Swap { slot } => swap_moves(solution, slot, None, context),
            Self::NearbySwap { slot, max_nearby } => {
                swap_moves(solution, slot, Some(*max_nearby), context)
            }
            Self::PillarChange {
                slot,
                minimum_sub_pillar_size,
                maximum_sub_pillar_size,
                value_candidate_limit,
            } => pillar_change_moves(
                solution,
                slot,
                *minimum_sub_pillar_size,
                *maximum_sub_pillar_size,
                *value_candidate_limit,
                context,
            ),
            Self::PillarSwap {
                slot,
                minimum_sub_pillar_size,
                maximum_sub_pillar_size,
            } => pillar_swap_moves(
                solution,
                slot,
                *minimum_sub_pillar_size,
                *maximum_sub_pillar_size,
                context,
            ),
            Self::RuinRecreate {
                slot,
                min_ruin_count,
                max_ruin_count,
                moves_per_step,
                value_candidate_limit,
                recreate_heuristic_type,
            } => ruin_recreate_moves(
                solution,
                slot,
                RuinRecreateOptions {
                    min_ruin_count: *min_ruin_count,
                    max_ruin_count: *max_ruin_count,
                    moves_per_step: *moves_per_step,
                    value_candidate_limit: *value_candidate_limit,
                    recreate_heuristic_type: *recreate_heuristic_type,
                },
                context,
            ),
            Self::Grouped {
                group_name,
                scalar_slots,
                value_candidate_limit,
                max_moves_per_step,
                require_hard_improvement,
            } => grouped_scalar_moves(
                solution,
                group_name,
                scalar_slots,
                *value_candidate_limit,
                *max_moves_per_step,
                *require_hard_improvement,
                context,
            ),
            Self::ConflictRepair {
                constraint_names,
                scalar_slots,
                max_matches_per_step,
                max_repairs_per_match,
                max_moves_per_step,
                require_hard_improvement,
                include_soft_matches,
            } => conflict_repair_moves(
                solution,
                constraint_names,
                scalar_slots,
                *max_matches_per_step,
                *max_repairs_per_match,
                *max_moves_per_step,
                *require_hard_improvement,
                *include_soft_matches,
                DynamicCompoundScalarKind::ConflictRepair,
                context,
            ),
            Self::CompoundConflictRepair {
                constraint_names,
                scalar_slots,
                max_matches_per_step,
                max_repairs_per_match,
                max_moves_per_step,
                require_hard_improvement,
                include_soft_matches,
            } => conflict_repair_moves(
                solution,
                constraint_names,
                scalar_slots,
                *max_matches_per_step,
                *max_repairs_per_match,
                *max_moves_per_step,
                *require_hard_improvement,
                *include_soft_matches,
                DynamicCompoundScalarKind::CompoundConflictRepair,
                context,
            ),
        }
    }
}

fn collect_dynamic_selectors(
    config: &MoveSelectorConfig,
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    out: &mut Vec<DynamicScalarSelector>,
) {
    match config {
        MoveSelectorConfig::ChangeMoveSelector(change) => {
            for slot in matching_slots(model, &change.target) {
                out.push(DynamicScalarSelector::Change {
                    slot,
                    value_candidate_limit: change.value_candidate_limit,
                });
            }
        }
        MoveSelectorConfig::NearbyChangeMoveSelector(change) => {
            for slot in matching_slots(model, &change.target) {
                out.push(DynamicScalarSelector::NearbyChange {
                    slot,
                    max_nearby: change.max_nearby,
                    value_candidate_limit: change.value_candidate_limit,
                });
            }
        }
        MoveSelectorConfig::SwapMoveSelector(swap) => {
            for slot in matching_slots(model, &swap.target) {
                out.push(DynamicScalarSelector::Swap { slot });
            }
        }
        MoveSelectorConfig::NearbySwapMoveSelector(swap) => {
            for slot in matching_slots(model, &swap.target) {
                out.push(DynamicScalarSelector::NearbySwap {
                    slot,
                    max_nearby: swap.max_nearby,
                });
            }
        }
        MoveSelectorConfig::PillarChangeMoveSelector(pillar_change) => {
            for slot in matching_slots(model, &pillar_change.target) {
                out.push(DynamicScalarSelector::PillarChange {
                    slot,
                    minimum_sub_pillar_size: pillar_change.minimum_sub_pillar_size,
                    maximum_sub_pillar_size: pillar_change.maximum_sub_pillar_size,
                    value_candidate_limit: pillar_change.value_candidate_limit,
                });
            }
        }
        MoveSelectorConfig::PillarSwapMoveSelector(pillar_swap) => {
            for slot in matching_slots(model, &pillar_swap.target) {
                out.push(DynamicScalarSelector::PillarSwap {
                    slot,
                    minimum_sub_pillar_size: pillar_swap.minimum_sub_pillar_size,
                    maximum_sub_pillar_size: pillar_swap.maximum_sub_pillar_size,
                });
            }
        }
        MoveSelectorConfig::RuinRecreateMoveSelector(ruin_recreate) => {
            for slot in matching_slots(model, &ruin_recreate.target) {
                out.push(DynamicScalarSelector::RuinRecreate {
                    slot,
                    min_ruin_count: ruin_recreate.min_ruin_count,
                    max_ruin_count: ruin_recreate.max_ruin_count,
                    moves_per_step: ruin_recreate.moves_per_step.unwrap_or(10).max(1),
                    value_candidate_limit: ruin_recreate.value_candidate_limit,
                    recreate_heuristic_type: ruin_recreate.recreate_heuristic_type,
                });
            }
        }
        MoveSelectorConfig::GroupedScalarMoveSelector(grouped) => {
            let scalar_slots = all_dynamic_scalar_slots(model);
            if !scalar_slots.is_empty() {
                out.push(DynamicScalarSelector::Grouped {
                    group_name: grouped.group_name.clone(),
                    scalar_slots,
                    value_candidate_limit: grouped.value_candidate_limit,
                    max_moves_per_step: grouped.max_moves_per_step,
                    require_hard_improvement: grouped.require_hard_improvement,
                });
            }
        }
        MoveSelectorConfig::ConflictRepairMoveSelector(repair) => {
            let scalar_slots = all_dynamic_scalar_slots(model);
            if !scalar_slots.is_empty() {
                out.push(DynamicScalarSelector::ConflictRepair {
                    constraint_names: repair.constraints.clone(),
                    scalar_slots,
                    max_matches_per_step: repair.max_matches_per_step,
                    max_repairs_per_match: repair.max_repairs_per_match,
                    max_moves_per_step: repair.max_moves_per_step,
                    require_hard_improvement: repair.require_hard_improvement,
                    include_soft_matches: repair.include_soft_matches,
                });
            }
        }
        MoveSelectorConfig::CompoundConflictRepairMoveSelector(repair) => {
            let scalar_slots = all_dynamic_scalar_slots(model);
            if !scalar_slots.is_empty() {
                out.push(DynamicScalarSelector::CompoundConflictRepair {
                    constraint_names: repair.constraints.clone(),
                    scalar_slots,
                    max_matches_per_step: repair.max_matches_per_step,
                    max_repairs_per_match: repair.max_repairs_per_match,
                    max_moves_per_step: repair.max_moves_per_step,
                    require_hard_improvement: repair.require_hard_improvement,
                    include_soft_matches: repair.include_soft_matches,
                });
            }
        }
        MoveSelectorConfig::UnionMoveSelector(union) => {
            for child in &union.selectors {
                collect_dynamic_selectors(child, model, out);
            }
        }
        _ => {}
    }
}

fn matching_slots(
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
    target: &solverforge_config::VariableTargetConfig,
) -> Vec<DynamicScalarVariableSlot<PyDynamicSolution>> {
    model
        .dynamic_scalar_variables()
        .filter(|slot| {
            slot.matches_target(
                target.entity_class.as_deref(),
                target.variable_name.as_deref(),
            )
        })
        .cloned()
        .collect()
}

fn all_dynamic_scalar_slots(
    model: &RuntimeModel<PyDynamicSolution, usize, PyDistanceMeter, PyDistanceMeter>,
) -> Vec<DynamicScalarVariableSlot<PyDynamicSolution>> {
    model.dynamic_scalar_variables().cloned().collect()
}

fn change_moves(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    value_candidate_limit: Option<usize>,
    context: MoveStreamContext,
) -> Vec<DynamicScalar> {
    let mut moves = Vec::new();
    let limit = value_candidate_limit.unwrap_or(usize::MAX);
    let entity_count = slot.entity_count(solution);
    for entity_offset in 0..entity_count {
        let entity_index = ordered_entity_index(
            entity_count,
            entity_offset,
            context,
            0xC4A4_6E00_CCCC_0001,
            0xC4A4_6E00_CCCC_0002,
            slot,
        );
        let current = slot.current_value(solution, entity_index);
        for value in slot
            .candidate_values(solution, entity_index)
            .iter()
            .copied()
            .take(limit)
        {
            if current != Some(value) {
                moves.push(DynamicScalar::Change(
                    solverforge_solver::DynamicScalarChangeMove::new(
                        slot.clone(),
                        entity_index,
                        Some(value),
                    ),
                ));
            }
        }
        if slot.allows_unassigned && current.is_some() {
            moves.push(DynamicScalar::Change(
                solverforge_solver::DynamicScalarChangeMove::new(slot.clone(), entity_index, None),
            ));
        }
    }
    moves
}

fn nearby_change_moves(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    max_nearby: usize,
    value_candidate_limit: Option<usize>,
    context: MoveStreamContext,
) -> Vec<DynamicScalar> {
    let mut moves = Vec::new();
    let limit = value_candidate_limit.unwrap_or(usize::MAX);
    let entity_count = slot.entity_count(solution);
    for entity_offset in 0..entity_count {
        let entity_index = ordered_entity_index(
            entity_count,
            entity_offset,
            context,
            0xC4A4_6E00_AAAA_0001,
            0xC4A4_6E00_AAAA_0002,
            slot,
        );
        let current = slot.current_value(solution, entity_index);
        let mut candidates = nearby_value_candidates(solution, slot, entity_index)
            .iter()
            .copied()
            .take(limit)
            .enumerate()
            .filter_map(|(order, value)| {
                if current == Some(value) {
                    return None;
                }
                if !slot.value_is_legal(solution, entity_index, Some(value)) {
                    return None;
                }
                let distance = nearby_value_distance(solution, slot, entity_index, value, order);
                distance.is_finite().then_some((value, distance, order))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.0.cmp(&right.0))
        });
        candidates.truncate(max_nearby);
        for (value, _, _) in candidates {
            moves.push(DynamicScalar::Change(
                solverforge_solver::DynamicScalarChangeMove::new(
                    slot.clone(),
                    entity_index,
                    Some(value),
                ),
            ));
        }
        if slot.allows_unassigned && current.is_some() {
            moves.push(DynamicScalar::Change(
                solverforge_solver::DynamicScalarChangeMove::new(slot.clone(), entity_index, None),
            ));
        }
    }
    moves
}

fn swap_moves(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    max_nearby: Option<usize>,
    context: MoveStreamContext,
) -> Vec<DynamicScalar> {
    let mut moves = Vec::new();
    let count = slot.entity_count(solution);
    for left_offset in 0..count {
        let left = ordered_entity_index(
            count,
            left_offset,
            context,
            if max_nearby.is_some() {
                0x5A09_5CA1_AAAA_0001
            } else {
                0x5A09_5CA1_AA00_0001
            },
            if max_nearby.is_some() {
                0x5A09_5CA1_AAAA_0002
            } else {
                0x5A09_5CA1_AA00_0002
            },
            slot,
        );
        let right_indices = if max_nearby.is_some() {
            nearby_entity_candidates(solution, slot, left, count)
        } else {
            (0..count)
                .map(|right_offset| {
                    ordered_index(
                        count,
                        right_offset,
                        context,
                        0x5A09_5CA1_AA00_0002 ^ left as u64 ^ slot.variable.0 as u64,
                        0x5A09_5CA1_AA00_0002
                            ^ left as u64
                            ^ slot.variable.0 as u64
                            ^ 0xD1B5_4A32_D192_ED03,
                    )
                })
                .collect::<Vec<_>>()
        };
        let mut candidates = right_indices
            .into_iter()
            .enumerate()
            .filter_map(|(order, right)| {
                if (max_nearby.is_some() && right == left)
                    || (max_nearby.is_none() && right <= left)
                {
                    return None;
                }
                if !can_swap(solution, slot, left, right) {
                    return None;
                }
                let distance = max_nearby
                    .map(|_| nearby_entity_distance(solution, slot, left, right, order))
                    .unwrap_or(order as f64);
                distance.is_finite().then_some((right, distance, order))
            })
            .collect::<Vec<_>>();
        if let Some(max_nearby) = max_nearby {
            candidates.sort_by(|left, right| {
                left.1
                    .total_cmp(&right.1)
                    .then_with(|| left.2.cmp(&right.2))
                    .then_with(|| left.0.cmp(&right.0))
            });
            candidates.truncate(max_nearby);
        }
        for (right, _, _) in candidates {
            moves.push(DynamicScalar::Swap(DynamicScalarSwapMove::new(
                slot.clone(),
                left,
                right,
            )));
        }
    }
    moves
}

fn pillar_change_moves(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    minimum_sub_pillar_size: usize,
    maximum_sub_pillar_size: usize,
    value_candidate_limit: Option<usize>,
    context: MoveStreamContext,
) -> Vec<DynamicScalar> {
    let groups = scalar_pillars(
        solution,
        slot,
        minimum_sub_pillar_size,
        maximum_sub_pillar_size,
    );
    let mut moves = Vec::new();
    for group_offset in 0..groups.len() {
        let group_index = ordered_index(
            groups.len(),
            group_offset,
            context,
            0xC4A4_6E00_DD00_0001 ^ slot.variable.0 as u64,
            0xC4A4_6E00_DD00_0002 ^ slot.variable.0 as u64,
        );
        let (current_value, entity_indices) = &groups[group_index];
        for value in intersect_scalar_values(solution, slot, entity_indices, value_candidate_limit)
        {
            if value == *current_value {
                continue;
            }
            moves.push(DynamicScalar::PillarChange {
                slot: slot.clone(),
                mov: PillarChangeMove::new(
                    entity_indices.clone(),
                    Some(value),
                    dynamic_scalar_get,
                    dynamic_scalar_set,
                    slot.variable.0,
                    slot.variable_name,
                    slot.descriptor_index(),
                ),
            });
        }
    }
    moves
}

fn pillar_swap_moves(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    minimum_sub_pillar_size: usize,
    maximum_sub_pillar_size: usize,
    context: MoveStreamContext,
) -> Vec<DynamicScalar> {
    let groups = scalar_pillars(
        solution,
        slot,
        minimum_sub_pillar_size,
        maximum_sub_pillar_size,
    );
    let mut moves = Vec::new();
    for left_offset in 0..groups.len() {
        let left_index = ordered_index(
            groups.len(),
            left_offset,
            context,
            0xC4A4_6E00_EE00_0001 ^ slot.variable.0 as u64,
            0xC4A4_6E00_EE00_0002 ^ slot.variable.0 as u64,
        );
        let (left_value, left_entities) = &groups[left_index];
        for (right_index, (right_value, right_entities)) in groups.iter().enumerate() {
            if right_index <= left_index || left_value == right_value {
                continue;
            }
            let left_accepts_right = left_entities
                .iter()
                .all(|&entity| slot.value_is_legal(solution, entity, Some(*right_value)));
            let right_accepts_left = right_entities
                .iter()
                .all(|&entity| slot.value_is_legal(solution, entity, Some(*left_value)));
            if !left_accepts_right || !right_accepts_left {
                continue;
            }
            moves.push(DynamicScalar::PillarSwap {
                slot: slot.clone(),
                mov: PillarSwapMove::new(
                    left_entities.clone(),
                    right_entities.clone(),
                    dynamic_scalar_get,
                    dynamic_scalar_set,
                    slot.variable.0,
                    slot.variable_name,
                    slot.descriptor_index(),
                ),
            });
        }
    }
    moves
}

fn ruin_recreate_moves(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    options: RuinRecreateOptions,
    context: MoveStreamContext,
) -> Vec<DynamicScalar> {
    let entity_count = slot.entity_count(solution);
    if entity_count == 0 || options.max_ruin_count == 0 {
        return Vec::new();
    }
    let min = options.min_ruin_count.max(1).min(entity_count);
    let max = options.max_ruin_count.max(min).min(entity_count);
    let ruin_span = max - min + 1;
    let mut moves = Vec::new();
    for move_offset in 0..options.moves_per_step {
        let ruin_count = min
            + ordered_index(
                ruin_span,
                move_offset % ruin_span,
                context,
                0xC4A4_6E00_FF00_0001 ^ slot.variable.0 as u64 ^ move_offset as u64,
                0xC4A4_6E00_FF00_0002 ^ slot.variable.0 as u64,
            );
        let mut entity_indices = Vec::with_capacity(ruin_count);
        for entity_offset in 0..entity_count {
            let entity_index = ordered_entity_index(
                entity_count,
                entity_offset,
                context,
                0xC4A4_6E00_FF00_0003 ^ move_offset as u64,
                0xC4A4_6E00_FF00_0004 ^ move_offset as u64,
                slot,
            );
            if !entity_indices.contains(&entity_index) {
                entity_indices.push(entity_index);
            }
            if entity_indices.len() == ruin_count {
                break;
            }
        }
        moves.push(DynamicScalar::RuinRecreate {
            slot: slot.clone(),
            mov: RuinRecreateMove::new(
                &entity_indices,
                dynamic_scalar_get,
                dynamic_scalar_set,
                slot.descriptor_index(),
                slot.variable.0,
                slot.variable_name,
                ScalarRecreateValueSource::CandidateSlice {
                    candidate_values: dynamic_scalar_candidate_values,
                    variable_index: slot.variable.0,
                    value_candidate_limit: options.value_candidate_limit,
                },
                options.recreate_heuristic_type,
                slot.allows_unassigned,
            ),
        });
    }
    moves
}

fn grouped_scalar_moves(
    solution: &PyDynamicSolution,
    group_name: &str,
    scalar_slots: &[DynamicScalarVariableSlot<PyDynamicSolution>],
    value_candidate_limit: Option<usize>,
    max_moves_per_step: Option<usize>,
    require_hard_improvement: bool,
    context: MoveStreamContext,
) -> Vec<DynamicScalar> {
    Python::attach(|py| -> PyResult<Vec<DynamicScalar>> {
        let groups = solution
            .schema
            .scalar_groups
            .bind(py)
            .cast::<PyList>()?;
        let mut moves = Vec::new();
        let limit = max_moves_per_step.unwrap_or(256).max(1);
        let mut matched_group = false;
        for group_any in groups.iter() {
            let group = group_any.cast::<PyDict>()?;
            let name = required_dict_str(group, "name")?;
            if name != group_name {
                continue;
            }
            matched_group = true;
            let callback = group
                .get_item("callback")?
                .ok_or_else(|| crate::error::py_err("scalar group missing `callback`"))?;
            let limits = dynamic_limits_dict(py)?;
            set_optional_usize(&limits, "value_candidate_limit", value_candidate_limit)?;
            set_optional_usize(&limits, "max_moves_per_step", max_moves_per_step)?;
            let result = callback.call1((solution.to_python_callback_view(py)?, limits))?;
            let parsed = parse_dynamic_compound_candidates(
                py,
                &result,
                solution,
                scalar_slots,
                DynamicCompoundScalarKind::Grouped,
                "dynamic_grouped_scalar",
                require_hard_improvement,
            )?;
            for mov in parsed {
                moves.push(DynamicScalar::Grouped(mov));
                if moves.len() >= limit {
                    break;
                }
            }
        }
        if !matched_group {
            return Err(crate::error::py_err(format!(
                "grouped_scalar_move_selector configured for `{group_name}`, but no matching Python scalar group was declared"
            )));
        }
        if !moves.is_empty() {
            let offset =
                context.start_offset(moves.len(), 0xC0A1_E5CE_DA7A_0001 ^ group_name.len() as u64);
            moves.rotate_left(offset);
        }
        moves.truncate(limit);
        Ok(moves)
    })
    .unwrap_or_else(|err| panic!("python scalar group callback failed: {err:?}"))
}

#[allow(clippy::too_many_arguments)]
fn conflict_repair_moves(
    solution: &PyDynamicSolution,
    constraint_names: &[String],
    scalar_slots: &[DynamicScalarVariableSlot<PyDynamicSolution>],
    max_matches_per_step: usize,
    max_repairs_per_match: usize,
    max_moves_per_step: usize,
    require_hard_improvement: bool,
    include_soft_matches: bool,
    kind: DynamicCompoundScalarKind,
    context: MoveStreamContext,
) -> Vec<DynamicScalar> {
    if constraint_names.is_empty()
        || max_matches_per_step == 0
        || max_repairs_per_match == 0
        || max_moves_per_step == 0
    {
        return Vec::new();
    }
    Python::attach(|py| -> PyResult<Vec<DynamicScalar>> {
        validate_dynamic_conflict_constraints(
            py,
            solution,
            constraint_names,
            include_soft_matches,
        )?;
        let repairs = solution
            .schema
            .conflict_repairs
            .bind(py)
            .cast::<PyList>()?;
        let mut moves = Vec::new();
        let mut provider_invocations = 0usize;
        let mut matched_repair = false;
        let mut repair_indices = (0..repairs.len()).collect::<Vec<_>>();
        let offset = context.start_offset(
            repair_indices.len(),
            0xC0AF_11C7_DA7A_0001 ^ max_moves_per_step as u64,
        );
        repair_indices.rotate_left(offset);
        for repair_index in repair_indices {
            if moves.len() >= max_moves_per_step || provider_invocations >= max_matches_per_step {
                break;
            }
            let repair_any = repairs.get_item(repair_index)?;
            let repair = repair_any.cast::<PyDict>()?;
            let declared = string_list_from_dict(repair, "constraints")?;
            if !declared
                .iter()
                .any(|declared_name| constraint_names.iter().any(|name| name == declared_name))
            {
                continue;
            }
            matched_repair = true;
            provider_invocations += 1;
            let callback = repair
                .get_item("callback")?
                .ok_or_else(|| crate::error::py_err("conflict repair missing `callback`"))?;
            let limits = dynamic_limits_dict(py)?;
            limits.set_item("constraints", constraint_names.to_vec())?;
            limits.set_item("max_matches_per_step", max_matches_per_step)?;
            limits.set_item("max_repairs_per_match", max_repairs_per_match)?;
            limits.set_item("max_moves_per_step", max_moves_per_step)?;
            limits.set_item("include_soft_matches", include_soft_matches)?;
            let result = callback.call1((solution.to_python_callback_view(py)?, limits))?;
            let mut parsed = parse_dynamic_compound_candidates(
                py,
                &result,
                solution,
                scalar_slots,
                kind,
                match kind {
                    DynamicCompoundScalarKind::ConflictRepair => "dynamic_conflict_repair",
                    DynamicCompoundScalarKind::CompoundConflictRepair => {
                        "dynamic_compound_conflict_repair"
                    }
                    DynamicCompoundScalarKind::Grouped => "dynamic_grouped_scalar",
                },
                require_hard_improvement,
            )?;
            parsed.truncate(max_repairs_per_match);
            for mov in parsed {
                moves.push(match kind {
                    DynamicCompoundScalarKind::ConflictRepair => DynamicScalar::ConflictRepair(mov),
                    DynamicCompoundScalarKind::CompoundConflictRepair => {
                        DynamicScalar::CompoundConflictRepair(mov)
                    }
                    DynamicCompoundScalarKind::Grouped => DynamicScalar::Grouped(mov),
                });
                if moves.len() >= max_moves_per_step {
                    break;
                }
            }
        }
        if !matched_repair {
            return Err(crate::error::py_err(format!(
                "conflict repair selector configured for {:?}, but no matching Python conflict repair was declared",
                constraint_names
            )));
        }
        Ok(moves)
    })
    .unwrap_or_else(|err| panic!("python conflict repair callback failed: {err:?}"))
}

#[allow(clippy::too_many_arguments)]
fn parse_dynamic_compound_candidates(
    py: Python<'_>,
    result: &Bound<'_, PyAny>,
    solution: &PyDynamicSolution,
    scalar_slots: &[DynamicScalarVariableSlot<PyDynamicSolution>],
    kind: DynamicCompoundScalarKind,
    default_reason: &'static str,
    require_hard_improvement: bool,
) -> PyResult<Vec<DynamicCompoundScalarMove>> {
    if result.is_none() {
        return Ok(Vec::new());
    }
    let mut moves = Vec::new();
    let mut seen = HashSet::new();
    if let Ok(dict) = result.cast::<PyDict>() {
        append_dynamic_compound_candidate(
            py,
            &mut moves,
            &mut seen,
            dict.as_any(),
            solution,
            scalar_slots,
            kind,
            default_reason,
            require_hard_improvement,
        )?;
        return Ok(moves);
    }
    if let Ok(list) = result.cast::<PyList>() {
        for item in list.iter() {
            append_dynamic_compound_candidate(
                py,
                &mut moves,
                &mut seen,
                &item,
                solution,
                scalar_slots,
                kind,
                default_reason,
                require_hard_improvement,
            )?;
        }
        return Ok(moves);
    }
    if let Ok(tuple) = result.cast::<PyTuple>() {
        for item in tuple.iter() {
            append_dynamic_compound_candidate(
                py,
                &mut moves,
                &mut seen,
                &item,
                solution,
                scalar_slots,
                kind,
                default_reason,
                require_hard_improvement,
            )?;
        }
        return Ok(moves);
    }
    Err(crate::error::py_err(format!(
        "dynamic compound callback returned unsupported candidate container {result:?}"
    )))
}

#[allow(clippy::too_many_arguments)]
fn append_dynamic_compound_candidate(
    _py: Python<'_>,
    moves: &mut Vec<DynamicCompoundScalarMove>,
    seen: &mut HashSet<String>,
    candidate_any: &Bound<'_, PyAny>,
    solution: &PyDynamicSolution,
    scalar_slots: &[DynamicScalarVariableSlot<PyDynamicSolution>],
    kind: DynamicCompoundScalarKind,
    default_reason: &'static str,
    require_hard_improvement: bool,
) -> PyResult<()> {
    if candidate_any.is_none() {
        return Ok(());
    }
    let (reason, edits_any) = if let Ok(candidate) = candidate_any.cast::<PyDict>() {
        let reason = optional_dict_str(candidate, "reason")?.unwrap_or(default_reason.to_string());
        let edits = candidate
            .get_item("edits")?
            .ok_or_else(|| crate::error::py_err("dynamic compound candidate missing `edits`"))?;
        (reason, edits)
    } else {
        (default_reason.to_string(), candidate_any.clone())
    };
    let edits = parse_dynamic_compound_edits(&edits_any, solution, scalar_slots)?;
    if edits.is_empty() || compound_has_duplicate_targets(&edits) {
        return Ok(());
    }
    let mut key = reason.clone();
    for edit in &edits {
        key.push_str(&format!(
            "|{}:{}:{}:{:?}",
            edit.slot.descriptor_index(),
            edit.entity_index,
            edit.slot.variable_name,
            edit.to_value
        ));
    }
    if !seen.insert(key) {
        return Ok(());
    }
    moves.push(DynamicCompoundScalarMove::new(
        kind,
        reason,
        match kind {
            DynamicCompoundScalarKind::Grouped => "dynamic_grouped_scalar",
            DynamicCompoundScalarKind::ConflictRepair => "dynamic_conflict_repair",
            DynamicCompoundScalarKind::CompoundConflictRepair => "dynamic_compound_conflict_repair",
        },
        edits,
        require_hard_improvement,
    ));
    Ok(())
}

fn parse_dynamic_compound_edits(
    edits_any: &Bound<'_, PyAny>,
    solution: &PyDynamicSolution,
    scalar_slots: &[DynamicScalarVariableSlot<PyDynamicSolution>],
) -> PyResult<Vec<DynamicCompoundScalarEdit>> {
    let mut edits = Vec::new();
    if let Ok(list) = edits_any.cast::<PyList>() {
        for item in list.iter() {
            edits.push(parse_dynamic_compound_edit(&item, solution, scalar_slots)?);
        }
        return Ok(edits);
    }
    if let Ok(tuple) = edits_any.cast::<PyTuple>() {
        for item in tuple.iter() {
            edits.push(parse_dynamic_compound_edit(&item, solution, scalar_slots)?);
        }
        return Ok(edits);
    }
    Err(crate::error::py_err(format!(
        "dynamic compound candidate edits must be a list or tuple, got {edits_any:?}"
    )))
}

fn parse_dynamic_compound_edit(
    edit_any: &Bound<'_, PyAny>,
    solution: &PyDynamicSolution,
    scalar_slots: &[DynamicScalarVariableSlot<PyDynamicSolution>],
) -> PyResult<DynamicCompoundScalarEdit> {
    let edit = edit_any.cast::<PyDict>()?;
    let entity_object = edit.get_item("entity")?;
    let entity_class = optional_dict_str(edit, "entity_class")?
        .or(optional_dict_str(edit, "entity_type")?)
        .or_else(|| {
            entity_object
                .as_ref()
                .and_then(|entity| entity.getattr("_solverforge_entity_class").ok())
                .and_then(|value| value.extract::<String>().ok())
        });
    let variable_name = required_dict_str(edit, "variable_name")?;
    let entity_index = optional_dict_usize(edit, "entity_index")?
        .or_else(|| {
            entity_object
                .as_ref()
                .and_then(|entity| entity.getattr("_solverforge_entity_index").ok())
                .and_then(|value| value.extract::<usize>().ok())
        })
        .ok_or_else(|| {
            crate::error::py_err("dynamic compound edit missing `entity_index` or `entity`")
        })?;
    let to_value_any = edit
        .get_item("to_value")?
        .or(edit.get_item("value")?)
        .ok_or_else(|| crate::error::py_err("dynamic compound edit missing `to_value`"))?;
    let to_value = if to_value_any.is_none() {
        None
    } else {
        Some(to_value_any.extract::<usize>()?)
    };
    let slot = scalar_slots
        .iter()
        .find(|slot| slot.matches_target(entity_class.as_deref(), Some(variable_name.as_str())))
        .ok_or_else(|| {
            crate::error::py_err(format!(
                "dynamic compound edit targets unknown scalar variable `{}`{}",
                variable_name,
                entity_class
                    .as_ref()
                    .map(|name| format!(" on `{name}`"))
                    .unwrap_or_default()
            ))
        })?;
    if entity_index >= slot.entity_count(solution) {
        return Err(crate::error::py_err(format!(
            "dynamic compound edit entity_index `{entity_index}` is out of bounds for `{}`",
            slot.entity_type_name
        )));
    }
    if !slot.value_is_legal(solution, entity_index, to_value) {
        return Err(crate::error::py_err(format!(
            "dynamic compound edit value `{to_value:?}` is not legal for `{}.{}` row `{entity_index}`",
            slot.entity_type_name, slot.variable_name
        )));
    }
    Ok(DynamicCompoundScalarEdit {
        slot: slot.clone(),
        entity_index,
        to_value,
    })
}

fn validate_dynamic_conflict_constraints(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    constraint_names: &[String],
    include_soft_matches: bool,
) -> PyResult<()> {
    let constraints = solution.schema.constraints.bind(py).cast::<PyList>()?;
    for constraint_name in constraint_names {
        let mut found = false;
        for plan_any in constraints.iter() {
            let plan = plan_any.cast::<PyDict>()?;
            if required_dict_str(plan, "name")? != *constraint_name {
                continue;
            }
            found = true;
            if !include_soft_matches && !constraint_plan_is_hard(plan)? {
                return Err(crate::error::py_err(format!(
                    "conflict_repair_move_selector configured for non-hard constraint `{constraint_name}` while include_soft_matches is false"
                )));
            }
        }
        if !found {
            return Err(crate::error::py_err(format!(
                "conflict_repair_move_selector configured for `{constraint_name}`, but no matching dynamic constraint was found"
            )));
        }
    }
    Ok(())
}

fn constraint_plan_is_hard(plan: &Bound<'_, PyDict>) -> PyResult<bool> {
    let weight_any = plan
        .get_item("weight")?
        .ok_or_else(|| crate::error::py_err("constraint plan missing `weight`"))?;
    let weight = weight_any.cast::<PyDict>()?;
    let family = required_dict_str(weight, "family")?;
    if family == "soft" {
        return Ok(false);
    }
    let levels_any = weight
        .get_item("levels")?
        .ok_or_else(|| crate::error::py_err("constraint weight missing `levels`"))?;
    let levels = levels_any.cast::<PyList>()?;
    let Some(first) = levels.get_item(0).ok() else {
        return Ok(false);
    };
    if let Ok(value) = first.extract::<i64>() {
        return Ok(value != 0);
    }
    if let Ok(value) = first.extract::<f64>() {
        return Ok(value != 0.0);
    }
    Ok(false)
}

fn dynamic_limits_dict(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    Ok(PyDict::new(py))
}

fn set_optional_usize(dict: &Bound<'_, PyDict>, key: &str, value: Option<usize>) -> PyResult<()> {
    match value {
        Some(value) => dict.set_item(key, value),
        None => dict.set_item(key, dict.py().None()),
    }
}

fn dynamic_assignment_bool_hook(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    hook_name: &str,
    args: &[usize],
) -> PyResult<Option<bool>> {
    dynamic_assignment_extract_hook(py, solution, hook_name, args, |value| {
        value.extract::<bool>()
    })
}

fn dynamic_assignment_optional_usize_hook(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    hook_name: &str,
    args: &[usize],
) -> PyResult<Option<usize>> {
    dynamic_assignment_extract_hook(py, solution, hook_name, args, |value| {
        value.extract::<usize>()
    })
}

fn dynamic_assignment_optional_i64_hook(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    hook_name: &str,
    args: &[usize],
) -> PyResult<Option<i64>> {
    dynamic_assignment_extract_hook(py, solution, hook_name, args, |value| {
        value.extract::<i64>()
    })
}

fn dynamic_assignment_extract_hook<T>(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    hook_name: &str,
    args: &[usize],
    extract: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<T>,
) -> PyResult<Option<T>> {
    let group_name = active_dynamic_assignment_group();
    let group = solution.schema.assignment_scalar_group(&group_name).ok_or_else(|| {
        crate::error::py_err(format!(
            "grouped_scalar_move_selector configured for `{group_name}`, but no matching assignment scalar group was declared"
        ))
    })?;
    let Some(callback) = dynamic_assignment_hook_callback(group, hook_name) else {
        return Ok(None);
    };
    let snapshot = dynamic_assignment_callback_view(py, solution, group)?;
    let result = call_assignment_callback(callback.bind(py), snapshot, args)?;
    if result.is_none() {
        Ok(None)
    } else {
        extract(&result).map(Some)
    }
}

fn dynamic_assignment_hook_callback<'a>(
    group: &'a crate::schema::types::AssignmentScalarGroupSchema,
    hook_name: &str,
) -> Option<&'a Py<PyAny>> {
    match hook_name {
        "required_entity" => group.required_entity.as_ref(),
        "capacity_key" => group.capacity_key.as_ref(),
        "assignment_rule" => group.assignment_rule.as_ref(),
        "position_key" => group.position_key.as_ref(),
        "sequence_key" => group.sequence_key.as_ref(),
        "entity_order" => group.entity_order.as_ref(),
        "value_order" => group.value_order.as_ref(),
        _ => None,
    }
}

fn dynamic_assignment_callback_view(
    py: Python<'_>,
    solution: &PyDynamicSolution,
    group: &crate::schema::types::AssignmentScalarGroupSchema,
) -> PyResult<Py<PyAny>> {
    if group.sync_solution_before_callbacks {
        return solution.to_python_callback_view(py);
    }
    solution.to_python_unsynced_callback_view(py)
}

fn call_assignment_callback<'py>(
    callback: &Bound<'py, PyAny>,
    snapshot: Py<PyAny>,
    args: &[usize],
) -> PyResult<Bound<'py, PyAny>> {
    match args {
        [a] => callback.call1((snapshot, *a)),
        [a, b] => callback.call1((snapshot, *a, *b)),
        [a, b, c, d] => callback.call1((snapshot, *a, *b, *c, *d)),
        _ => Err(crate::error::py_err(format!(
            "unsupported assignment hook arity {}",
            args.len()
        ))),
    }
}

fn required_dict_str(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
    dict.get_item(key)?
        .ok_or_else(|| crate::error::py_err(format!("dynamic callback dict missing `{key}`")))?
        .extract::<String>()
}

fn optional_dict_str(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<String>> {
    dict.get_item(key)?
        .map(|value| {
            if value.is_none() {
                Ok(None)
            } else {
                value.extract::<String>().map(Some)
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn optional_dict_usize(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<usize>> {
    dict.get_item(key)?
        .map(|value| {
            if value.is_none() {
                Ok(None)
            } else {
                value.extract::<usize>().map(Some)
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn string_list_from_dict(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Vec<String>> {
    let values_any = dict
        .get_item(key)?
        .ok_or_else(|| crate::error::py_err(format!("dynamic callback dict missing `{key}`")))?;
    let values = values_any.cast::<PyList>()?;
    values.iter().map(|item| item.extract::<String>()).collect()
}

fn scalar_pillars(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    minimum_sub_pillar_size: usize,
    maximum_sub_pillar_size: usize,
) -> Vec<(usize, Vec<usize>)> {
    let mut grouped = std::collections::BTreeMap::<usize, Vec<usize>>::new();
    for entity_index in 0..slot.entity_count(solution) {
        if let Some(value) = slot.current_value(solution, entity_index) {
            grouped.entry(value).or_default().push(entity_index);
        }
    }

    let mut pillars = Vec::new();
    for (value, mut entities) in grouped {
        entities.sort_unstable();
        if entities.len() < 2 {
            continue;
        }
        if minimum_sub_pillar_size == 0 || maximum_sub_pillar_size == 0 {
            pillars.push((value, entities));
            continue;
        }

        let minimum_size = minimum_sub_pillar_size.max(2);
        let maximum_size = maximum_sub_pillar_size
            .max(minimum_size)
            .min(entities.len());
        if minimum_size > maximum_size {
            continue;
        }
        for window_size in minimum_size..=maximum_size {
            for start in 0..=entities.len() - window_size {
                pillars.push((value, entities[start..start + window_size].to_vec()));
            }
        }
    }
    pillars.sort_by_key(|(_, entities)| entities.first().copied().unwrap_or(usize::MAX));
    pillars
}

fn intersect_scalar_values(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    entity_indices: &[usize],
    value_candidate_limit: Option<usize>,
) -> Vec<usize> {
    let Some((&first_entity, rest)) = entity_indices.split_first() else {
        return Vec::new();
    };
    let limit = value_candidate_limit.unwrap_or(usize::MAX);
    let mut intersection = slot
        .candidate_values(solution, first_entity)
        .iter()
        .copied()
        .take(limit)
        .collect::<Vec<_>>();
    intersection.dedup();

    for &entity_index in rest {
        let legal_values = slot
            .candidate_values(solution, entity_index)
            .iter()
            .copied()
            .take(limit)
            .collect::<Vec<_>>();
        intersection.retain(|value| legal_values.contains(value));
        if intersection.is_empty() {
            break;
        }
    }
    intersection
}

fn ordered_entity_index(
    len: usize,
    offset: usize,
    context: MoveStreamContext,
    start_salt: u64,
    stride_salt: u64,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
) -> usize {
    ordered_index(
        len,
        offset,
        context,
        start_salt ^ ((slot.entity.0 as u64) << 32) ^ slot.variable.0 as u64,
        stride_salt ^ ((slot.entity.0 as u64) << 32) ^ slot.variable.0 as u64,
    )
}

fn ordered_index(
    len: usize,
    offset: usize,
    context: MoveStreamContext,
    start_salt: u64,
    stride_salt: u64,
) -> usize {
    if len <= 1 {
        return 0;
    }
    let start = context.start_offset(len, start_salt);
    let stride = context.stride(len, stride_salt);
    (start + offset * stride) % len
}

fn can_swap(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    left: usize,
    right: usize,
) -> bool {
    let left_value = slot.current_value(solution, left);
    let right_value = slot.current_value(solution, right);
    left_value != right_value
        && slot.value_is_legal(solution, left, right_value)
        && slot.value_is_legal(solution, right, left_value)
}

fn schema_variable_for_scalar_slot<'a>(
    schema: &'a DynamicSchema,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
) -> Option<&'a VariableSchema> {
    schema
        .entities
        .get(slot.entity.0)?
        .variables
        .get(slot.variable.0)
}

fn nearby_value_candidates(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    entity_index: usize,
) -> Vec<usize> {
    let values = Python::attach(|py| -> PyResult<Option<Vec<usize>>> {
        let Some(variable) = schema_variable_for_scalar_slot(&solution.schema, slot) else {
            return Ok(None);
        };
        let Some(callback) = variable.nearby_value_candidates.as_ref() else {
            return Ok(None);
        };
        let entity = solution.entity_callback_view(py, slot.entity.0, entity_index)?;
        let result = callback.bind(py).call1((entity,))?;
        if result.is_none() {
            Ok(None)
        } else {
            result.extract::<Vec<usize>>().map(Some)
        }
    })
    .unwrap_or_else(|error| panic!("dynamic nearby value candidates callback failed: {error}"));

    values.unwrap_or_else(|| slot.candidate_values(solution, entity_index).to_vec())
}

fn nearby_entity_candidates(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    entity_index: usize,
    entity_count: usize,
) -> Vec<usize> {
    let candidates = Python::attach(|py| -> PyResult<Option<Vec<usize>>> {
        let Some(variable) = schema_variable_for_scalar_slot(&solution.schema, slot) else {
            return Ok(None);
        };
        let Some(callback) = variable.nearby_entity_candidates.as_ref() else {
            return Ok(None);
        };
        let entity = solution.entity_callback_view(py, slot.entity.0, entity_index)?;
        let result = callback.bind(py).call1((entity,))?;
        if result.is_none() {
            Ok(None)
        } else {
            result.extract::<Vec<usize>>().map(Some)
        }
    })
    .unwrap_or_else(|error| panic!("dynamic nearby entity candidates callback failed: {error}"));

    candidates.unwrap_or_else(|| (0..entity_count).collect())
}

fn nearby_value_distance(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    entity_index: usize,
    value: usize,
    order: usize,
) -> f64 {
    Python::attach(|py| -> PyResult<Option<f64>> {
        let Some(variable) = schema_variable_for_scalar_slot(&solution.schema, slot) else {
            return Ok(None);
        };
        let Some(callback) = variable.nearby_value_distance_meter.as_ref() else {
            return Ok(None);
        };
        let entity = solution.entity_callback_view(py, slot.entity.0, entity_index)?;
        let result = callback.bind(py).call1((entity, value))?;
        if result.is_none() {
            Ok(None)
        } else {
            result.extract::<f64>().map(Some)
        }
    })
    .unwrap_or_else(|error| panic!("dynamic nearby value distance callback failed: {error}"))
    .or_else(|| nearby_value_distance_field(solution, slot, entity_index, value))
    .unwrap_or(order as f64)
}

fn nearby_entity_distance(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    left: usize,
    right: usize,
    order: usize,
) -> f64 {
    Python::attach(|py| -> PyResult<Option<f64>> {
        let Some(variable) = schema_variable_for_scalar_slot(&solution.schema, slot) else {
            return Ok(None);
        };
        let Some(callback) = variable.nearby_entity_distance_meter.as_ref() else {
            return Ok(None);
        };
        let left_entity = solution.entity_callback_view(py, slot.entity.0, left)?;
        let right_entity = solution.entity_callback_view(py, slot.entity.0, right)?;
        let result = callback.bind(py).call1((left_entity, right_entity))?;
        if result.is_none() {
            Ok(None)
        } else {
            result.extract::<f64>().map(Some)
        }
    })
    .unwrap_or_else(|error| panic!("dynamic nearby entity distance callback failed: {error}"))
    .or_else(|| nearby_entity_distance_fields(solution, slot, left, right))
    .unwrap_or(order as f64)
}

fn nearby_value_distance_field(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    entity_index: usize,
    value: usize,
) -> Option<f64> {
    let row = solution
        .state
        .entities
        .get(slot.entity.0)?
        .get(entity_index)?;
    list_number(row.fields.get("employee_nearby_distance")?, value)
}

fn nearby_entity_distance_fields(
    solution: &PyDynamicSolution,
    slot: &DynamicScalarVariableSlot<PyDynamicSolution>,
    left: usize,
    right: usize,
) -> Option<f64> {
    let rows = solution.state.entities.get(slot.entity.0)?;
    let left_row = rows.get(left)?;
    let right_row = rows.get(right)?;
    let hub_distance = care_hub_distance(
        string_field(left_row, "care_hub")?,
        string_field(right_row, "care_hub")?,
    )?;
    let start_distance = start_band_distance(start_hour(left_row)?, start_hour(right_row)?);
    Some(10.0 * hub_distance + start_distance)
}

fn list_number(value: &DynamicValue, index: usize) -> Option<f64> {
    let DynamicValue::List(values) = value else {
        return None;
    };
    dynamic_number(values.get(index)?)
}

fn dynamic_number(value: &DynamicValue) -> Option<f64> {
    match value {
        DynamicValue::Int(value) => Some(*value as f64),
        DynamicValue::Float(value) => Some(*value),
        _ => None,
    }
}

fn string_field<'a>(row: &'a DynamicEntityRow, field_name: &str) -> Option<&'a str> {
    match row.fields.get(field_name)? {
        DynamicValue::String(value) => Some(value.as_str()),
        _ => None,
    }
}

fn start_hour(row: &DynamicEntityRow) -> Option<i64> {
    let start = string_field(row, "start")?;
    let time = start
        .split('T')
        .nth(1)
        .or_else(|| start.split(' ').nth(1))?;
    let hour = time.split(':').next()?;
    hour.parse().ok()
}

fn start_band_distance(left_hour: i64, right_hour: i64) -> f64 {
    let distance = (start_band_index(left_hour) - start_band_index(right_hour)).abs();
    distance.min(2) as f64
}

fn start_band_index(hour: i64) -> i64 {
    if hour <= 7 {
        0
    } else if hour <= 12 {
        1
    } else if hour <= 17 {
        2
    } else {
        3
    }
}

fn care_hub_distance(left: &str, right: &str) -> Option<f64> {
    let (left_x, left_y) = care_hub_position(left)?;
    let (right_x, right_y) = care_hub_position(right)?;
    Some((left_x - right_x).abs() as f64 + (left_y - right_y).abs() as f64)
}

fn care_hub_position(hub: &str) -> Option<(i64, i64)> {
    Some(match hub {
        "ambulatory" => (0, 0),
        "outpatient" => (1, 0),
        "pediatric_care" => (0, 1),
        "neurology" => (1, 1),
        "critical_care" => (2, 1),
        "surgery" => (2, 2),
        "radiology" => (3, 2),
        "unknown" => (4, 4),
        _ => return None,
    })
}
