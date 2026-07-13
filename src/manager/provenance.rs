//! Explicit, externally-attested provenance for a qualified candidate trace.
//!
//! This is deliberately an inert Python value.  It validates only the six
//! values supplied by its caller and transports the resulting core value to a
//! retained job later; it never discovers digests from environment, callbacks,
//! files, or a working solution.

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyString;
use solverforge_solver::stats::{
    CandidateTraceExternalDigest, QualifiedCandidateTraceRunProvenance,
};

/// Immutable, externally-attested inputs for one qualified candidate-trace
/// diagnostic run.
///
/// The Python-facing fields deliberately say `*_sha256`: the header uses the
/// established `*_digest` names, but this constructor accepts only a SHA-256
/// hexadecimal representation.
#[pyclass(name = "QualifiedCandidateTraceProvenance", frozen)]
pub struct PyQualifiedCandidateTraceProvenance {
    inner: QualifiedCandidateTraceRunProvenance,
}

impl PyQualifiedCandidateTraceProvenance {
    /// Clones the already validated core value for one retained job.
    ///
    /// The wrapper must keep this at the explicit manager-to-runner handoff;
    /// callers cannot supply a partially parsed or inferred provenance value.
    pub(crate) fn clone_inner(&self) -> QualifiedCandidateTraceRunProvenance {
        self.inner.clone()
    }
}

#[pymethods]
impl PyQualifiedCandidateTraceProvenance {
    #[new]
    #[pyo3(signature = (*, schema_sha256, instance_sha256, initial_state_sha256, core_tree_sha256, build_sha256, producer))]
    fn new(
        schema_sha256: &Bound<'_, PyAny>,
        instance_sha256: &Bound<'_, PyAny>,
        initial_state_sha256: &Bound<'_, PyAny>,
        core_tree_sha256: &Bound<'_, PyAny>,
        build_sha256: &Bound<'_, PyAny>,
        producer: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let schema_digest = parse_lowercase_sha256("schema_sha256", schema_sha256)?;
        let instance_digest = parse_lowercase_sha256("instance_sha256", instance_sha256)?;
        let initial_state_digest =
            parse_lowercase_sha256("initial_state_sha256", initial_state_sha256)?;
        let core_tree_digest = parse_lowercase_sha256("core_tree_sha256", core_tree_sha256)?;
        let build_digest = parse_lowercase_sha256("build_sha256", build_sha256)?;
        let producer = required_python_string("producer", producer)?;
        let inner = QualifiedCandidateTraceRunProvenance::externally_attested(
            schema_digest,
            instance_digest,
            initial_state_digest,
            core_tree_digest,
            build_digest,
            producer,
        )
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self { inner })
    }

    #[getter]
    fn schema_sha256(&self) -> String {
        external_digest_hex(self.inner.input_provenance().schema_digest)
    }

    #[getter]
    fn instance_sha256(&self) -> String {
        external_digest_hex(self.inner.input_provenance().instance_digest)
    }

    #[getter]
    fn initial_state_sha256(&self) -> String {
        external_digest_hex(self.inner.input_provenance().initial_state_digest)
    }

    #[getter]
    fn core_tree_sha256(&self) -> String {
        let digest = self
            .inner
            .input_provenance()
            .core_tree_digest
            .expect("qualified provenance always has a core-tree digest");
        external_digest_hex(digest)
    }

    #[getter]
    fn build_sha256(&self) -> String {
        let digest = self
            .inner
            .input_provenance()
            .build_digest
            .expect("qualified provenance always has a build digest");
        external_digest_hex(digest)
    }

    #[getter]
    fn producer(&self) -> String {
        self.inner
            .input_provenance()
            .attestation
            .external_producer()
            .to_string()
    }
}

fn parse_lowercase_sha256(
    field: &str,
    value: &Bound<'_, PyAny>,
) -> PyResult<CandidateTraceExternalDigest> {
    let value = required_python_string(field, value)?;
    let bytes = value.as_bytes();
    if bytes.len() != 64 {
        return Err(invalid_sha256(field));
    }

    let mut digest = [0_u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        let Some(high) = lowercase_hex_nibble(bytes[index * 2]) else {
            return Err(invalid_sha256(field));
        };
        let Some(low) = lowercase_hex_nibble(bytes[index * 2 + 1]) else {
            return Err(invalid_sha256(field));
        };
        *byte = (high << 4) | low;
    }
    Ok(CandidateTraceExternalDigest::sha256(digest))
}

fn required_python_string(field: &str, value: &Bound<'_, PyAny>) -> PyResult<String> {
    let value = value
        .cast::<PyString>()
        .map_err(|_| PyTypeError::new_err(format!("{field} must be a str")))?;
    value.extract::<String>()
}

fn lowercase_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn invalid_sha256(field: &str) -> PyErr {
    PyValueError::new_err(format!(
        "{field} must be exactly 64 lowercase hexadecimal characters encoding SHA-256"
    ))
}

fn external_digest_hex(digest: CandidateTraceExternalDigest) -> String {
    let mut value = String::with_capacity(64);
    for byte in digest.bytes {
        use std::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    value
}

#[cfg(test)]
mod tests {
    use pyo3::IntoPyObject;

    use super::{external_digest_hex, parse_lowercase_sha256};

    #[test]
    fn lowercase_sha256_parser_preserves_all_bytes() {
        pyo3::Python::initialize();
        pyo3::Python::attach(|py| {
            let value = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
                .into_pyobject(py)
                .expect("string converts to Python")
                .into_any();
            let digest = parse_lowercase_sha256("schema_sha256", &value)
                .expect("lowercase SHA-256 digest parses");
            assert_eq!(
                external_digest_hex(digest),
                "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
            );
        });
    }

    #[test]
    fn lowercase_sha256_parser_rejects_uppercase_and_wrong_length() {
        pyo3::Python::initialize();
        pyo3::Python::attach(|py| {
            let uppercase = "AA112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
                .into_pyobject(py)
                .expect("string converts to Python")
                .into_any();
            assert!(parse_lowercase_sha256("schema_sha256", &uppercase).is_err());

            let short = "0"
                .into_pyobject(py)
                .expect("string converts to Python")
                .into_any();
            assert!(parse_lowercase_sha256("schema_sha256", &short).is_err());
        });
    }
}
