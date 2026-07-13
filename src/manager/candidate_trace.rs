//! Explicit Python conversion for bounded core candidate-trace diagnostics.
//!
//! This module is intentionally separate from ordinary status/event conversion:
//! the trace is returned only from the manager's atomic detail endpoint and is
//! never materialized in progress traffic.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use solverforge_solver::stats::{
    CandidatePullTelemetry, CandidateTraceCoordinate, CandidateTraceDigest,
    CandidateTraceDisposition, CandidateTraceExecutionPolicy, CandidateTraceExternalDigest,
    CandidateTraceHeader, CandidateTraceIdentity, CandidateTraceInputProvenance,
    CandidateTraceInputProvenanceStatus, CandidateTracePhasePlan, CandidateTraceProvenanceStatus,
    CandidateTraceQualificationStatus, CandidateTraceSource, CandidateTraceTelemetry,
    QualifiedCandidateTraceRunProvenance,
};

/// Converts the one bounded, core-owned trace returned by the retained
/// manager. This function does not recompute a digest, inspect a solution, or
/// invoke a callback: it only projects already retained core detail.
pub(super) fn candidate_trace_to_python(
    py: Python<'_>,
    trace: &CandidateTraceTelemetry,
) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("header", header_to_python(py, &trace.header)?)?;
    dict.set_item("max_entries", trace.max_entries)?;
    dict.set_item("total_pulls", trace.total_pulls)?;
    dict.set_item(
        "pulls",
        PyList::new(
            py,
            trace
                .pulls
                .iter()
                .map(|pull| pull_to_python(py, pull))
                .collect::<PyResult<Vec<_>>>()?,
        )?,
    )?;
    dict.set_item("truncated", trace.truncated)?;
    dict.set_item("prefix_digest", digest_to_python(py, trace.prefix_digest)?)?;
    dict.set_item("unencoded_identity_count", trace.unencoded_identity_count)?;
    dict.set_item("trace_complete", trace.is_complete())?;
    dict.set_item(
        "execution_provenance_complete",
        trace.has_complete_execution_provenance(),
    )?;
    dict.set_item(
        "provenance_status",
        provenance_status_to_python(py, trace.provenance_status())?,
    )?;
    // `candidate_index` is intentionally retained for single-engine diagnosis,
    // but global ordinal plus identity is the cross-representation key.
    dict.set_item("candidate_index_scope", "source_local_only")?;
    Ok(dict.into_any().unbind())
}

fn header_to_python(py: Python<'_>, header: &CandidateTraceHeader) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("format_version", header.format_version)?;
    dict.set_item("configured_input", &header.configured_input)?;
    dict.set_item(
        "configured_input_digest",
        digest_to_python(py, header.configured_input_digest)?,
    )?;
    dict.set_item(
        "execution_policy",
        execution_policy_to_python(py, &header.execution_policy)?,
    )?;
    dict.set_item(
        "execution_policy_digest",
        digest_to_python(py, header.execution_policy_digest)?,
    )?;
    dict.set_item(
        "execution_policy_complete",
        header.execution_policy_complete,
    )?;
    match &header.input_provenance {
        Some(provenance) => {
            dict.set_item(
                "input_provenance",
                input_provenance_to_python(py, provenance)?,
            )?;
        }
        None => dict.set_item("input_provenance", py.None())?,
    }
    match header.input_provenance_digest {
        Some(digest) => dict.set_item("input_provenance_digest", digest_to_python(py, digest)?)?,
        None => dict.set_item("input_provenance_digest", py.None())?,
    }
    match &header.qualified_run_provenance {
        Some(provenance) => dict.set_item(
            "qualified_run_provenance",
            qualified_run_provenance_to_python(py, provenance)?,
        )?,
        None => dict.set_item("qualified_run_provenance", py.None())?,
    }
    dict.set_item(
        "resolved_phase_plan",
        phase_plan_to_python(py, &header.resolved_phase_plan)?,
    )?;
    dict.set_item(
        "resolved_phase_plan_digest",
        digest_to_python(py, header.resolved_phase_plan_digest)?,
    )?;
    dict.set_item(
        "resolved_phase_plan_complete",
        header.resolved_phase_plan_complete,
    )?;
    Ok(dict.into_any().unbind())
}

fn execution_policy_to_python(
    py: Python<'_>,
    policy: &CandidateTraceExecutionPolicy,
) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("kind", &policy.kind)?;
    dict.set_item("attributes", attributes_to_python(py, &policy.attributes)?)?;
    dict.set_item("opaque", policy.opaque)?;
    Ok(dict.into_any().unbind())
}

fn input_provenance_to_python(
    py: Python<'_>,
    provenance: &CandidateTraceInputProvenance,
) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item(
        "schema_digest",
        external_digest_to_python(provenance.schema_digest),
    )?;
    dict.set_item(
        "instance_digest",
        external_digest_to_python(provenance.instance_digest),
    )?;
    dict.set_item(
        "initial_state_digest",
        external_digest_to_python(provenance.initial_state_digest),
    )?;
    match provenance.core_tree_digest {
        Some(digest) => dict.set_item("core_tree_digest", external_digest_to_python(digest))?,
        None => dict.set_item("core_tree_digest", py.None())?,
    }
    match provenance.build_digest {
        Some(digest) => dict.set_item("build_digest", external_digest_to_python(digest))?,
        None => dict.set_item("build_digest", py.None())?,
    }
    let attestation = PyDict::new(py);
    attestation.set_item("kind", "external")?;
    attestation.set_item("producer", provenance.attestation.external_producer())?;
    dict.set_item("attestation", attestation)?;
    Ok(dict.into_any().unbind())
}

/// Projects the explicit, fail-closed run qualification carried by core.
///
/// Qualification is an explicit core value, so consumers never infer it from
/// the presence of digest fields in an unqualified trace.
fn qualified_run_provenance_to_python(
    py: Python<'_>,
    provenance: &QualifiedCandidateTraceRunProvenance,
) -> PyResult<Py<PyAny>> {
    input_provenance_to_python(py, provenance.input_provenance())
}

fn phase_plan_to_python(py: Python<'_>, plan: &CandidateTracePhasePlan) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("kind", &plan.kind)?;
    dict.set_item("attributes", attributes_to_python(py, &plan.attributes)?)?;
    dict.set_item("opaque", plan.opaque)?;
    dict.set_item(
        "children",
        PyList::new(
            py,
            plan.children
                .iter()
                .map(|child| phase_plan_to_python(py, child))
                .collect::<PyResult<Vec<_>>>()?,
        )?,
    )?;
    Ok(dict.into_any().unbind())
}

fn attributes_to_python(
    py: Python<'_>,
    attributes: &[solverforge_solver::stats::CandidateTracePhaseAttribute],
) -> PyResult<Py<PyAny>> {
    Ok(PyList::new(
        py,
        attributes
            .iter()
            .map(|attribute| {
                let dict = PyDict::new(py);
                dict.set_item("key", &attribute.key)?;
                dict.set_item("value", &attribute.value)?;
                Ok(dict.into_any().unbind())
            })
            .collect::<PyResult<Vec<_>>>()?,
    )?
    .into_any()
    .unbind())
}

fn pull_to_python(py: Python<'_>, pull: &CandidatePullTelemetry) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("ordinal", pull.ordinal)?;
    dict.set_item("source", source_name(pull.source))?;
    dict.set_item("phase_index", pull.phase_index)?;
    dict.set_item("phase_type", &pull.phase_type)?;
    dict.set_item("step_index", pull.step_index)?;
    dict.set_item("selector_index", pull.selector_index)?;
    dict.set_item("candidate_index", pull.candidate_index)?;
    match pull.construction_target {
        Some(target) => {
            let target_dict = PyDict::new(py);
            target_dict.set_item("descriptor_index", target.descriptor_index)?;
            target_dict.set_item("entity_index", target.entity_index)?;
            dict.set_item("construction_target", target_dict)?;
        }
        None => dict.set_item("construction_target", py.None())?,
    }
    match &pull.identity {
        Some(identity) => dict.set_item("identity", identity_to_python(py, identity)?)?,
        None => dict.set_item("identity", py.None())?,
    }
    dict.set_item(
        "dispositions",
        PyList::new(
            py,
            pull.dispositions
                .iter()
                .copied()
                .map(disposition_name)
                .collect::<Vec<_>>(),
        )?,
    )?;
    Ok(dict.into_any().unbind())
}

fn identity_to_python(py: Python<'_>, identity: &CandidateTraceIdentity) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    match identity {
        CandidateTraceIdentity::Operation(operation) => {
            dict.set_item("kind", "operation")?;
            dict.set_item("descriptor_index", operation.descriptor_index)?;
            dict.set_item("variable_name", &operation.variable_name)?;
            dict.set_item("operation", &operation.operation)?;
            dict.set_item(
                "components",
                PyList::new(
                    py,
                    operation
                        .components
                        .iter()
                        .map(|component| coordinate_to_python(py, component))
                        .collect::<PyResult<Vec<_>>>()?,
                )?,
            )?;
        }
        CandidateTraceIdentity::Composite(composite) => {
            dict.set_item("kind", "composite")?;
            dict.set_item("operation", &composite.operation)?;
            dict.set_item(
                "children",
                PyList::new(
                    py,
                    composite
                        .children
                        .iter()
                        .map(|child| identity_to_python(py, child))
                        .collect::<PyResult<Vec<_>>>()?,
                )?,
            )?;
        }
    }
    Ok(dict.into_any().unbind())
}

fn coordinate_to_python(
    py: Python<'_>,
    coordinate: &CandidateTraceCoordinate,
) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    match coordinate {
        CandidateTraceCoordinate::Unsigned(value) => {
            dict.set_item("kind", "unsigned")?;
            dict.set_item("value", value)?;
        }
        CandidateTraceCoordinate::Absent => {
            dict.set_item("kind", "absent")?;
            dict.set_item("value", py.None())?;
        }
        CandidateTraceCoordinate::Text(value) => {
            dict.set_item("kind", "text")?;
            dict.set_item("value", value)?;
        }
        CandidateTraceCoordinate::Bytes(value) => {
            dict.set_item("kind", "bytes")?;
            dict.set_item("value", hex_bytes(value))?;
        }
    }
    Ok(dict.into_any().unbind())
}

fn provenance_status_to_python(
    py: Python<'_>,
    status: CandidateTraceProvenanceStatus,
) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item(
        "execution_policy_complete",
        status.execution_policy_complete,
    )?;
    dict.set_item(
        "resolved_phase_plan_complete",
        status.resolved_phase_plan_complete,
    )?;
    dict.set_item(
        "input_provenance",
        match status.input_provenance {
            CandidateTraceInputProvenanceStatus::Absent => "absent",
            CandidateTraceInputProvenanceStatus::ExternallyAttested => "externally_attested",
        },
    )?;
    dict.set_item(
        "qualification",
        match status.qualification {
            CandidateTraceQualificationStatus::NotRequested => "not_requested",
            CandidateTraceQualificationStatus::Qualified => "qualified",
        },
    )?;
    Ok(dict.into_any().unbind())
}

fn digest_to_python(py: Python<'_>, digest: CandidateTraceDigest) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("algorithm", "solverforge_dual64")?;
    dict.set_item("first", digest.first)?;
    dict.set_item("second", digest.second)?;
    Ok(dict.into_any().unbind())
}

fn external_digest_to_python(digest: CandidateTraceExternalDigest) -> String {
    hex_bytes(&digest.bytes)
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    value
}

fn source_name(source: CandidateTraceSource) -> &'static str {
    match source {
        CandidateTraceSource::Construction => "construction",
        CandidateTraceSource::LocalSearch => "local_search",
        CandidateTraceSource::VariableNeighborhoodDescent => "variable_neighborhood_descent",
        CandidateTraceSource::KOpt => "k_opt",
        CandidateTraceSource::ListRoundRobinConstruction => "list_round_robin_construction",
        CandidateTraceSource::ListCheapestInsertionTrial => "list_cheapest_insertion_trial",
        CandidateTraceSource::ListRegretInsertionTrial => "list_regret_insertion_trial",
        CandidateTraceSource::ListClarkeWrightSavings => "list_clarke_wright_savings",
        CandidateTraceSource::ListClarkeWrightMerge => "list_clarke_wright_merge",
        CandidateTraceSource::ListClarkeWrightCompletionInsertion => {
            "list_clarke_wright_completion_insertion"
        }
        CandidateTraceSource::ListKOptReconnection => "list_k_opt_reconnection",
        CandidateTraceSource::ListRegretOwnerAppend => "list_regret_owner_append",
    }
}

fn disposition_name(disposition: CandidateTraceDisposition) -> &'static str {
    match disposition {
        CandidateTraceDisposition::InterruptedBeforeEvaluation => "interrupted_before_evaluation",
        CandidateTraceDisposition::Evaluated => "evaluated",
        CandidateTraceDisposition::NotDoable => "not_doable",
        CandidateTraceDisposition::RejectedByHardImprovement => "rejected_by_hard_improvement",
        CandidateTraceDisposition::RejectedByScoreImprovement => "rejected_by_score_improvement",
        CandidateTraceDisposition::AcceptorRejected => "acceptor_rejected",
        CandidateTraceDisposition::ForagerIgnored => "forager_ignored",
        CandidateTraceDisposition::Selected => "selected",
        CandidateTraceDisposition::Applied => "applied",
    }
}
