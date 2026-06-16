// SPDX-License-Identifier: Apache-2.0

//! Python bindings for code2graph.
//!
//! Exposes the extraction and resolution API to Python. Results are returned as
//! native Python objects (dicts/lists) produced from the crate's serde
//! representation, so `SymbolId`s appear as their stable SCIP strings.

use code2graph_core::extract_path;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pythonize::pythonize;

/// Extract symbols and references from a single source file.
///
/// `file` is a project-relative path used to infer the language; `source` is its
/// contents. Returns a dict mirroring `FileFacts` (symbols, references, scopes,
/// bindings, ffi_exports).
#[pyfunction]
fn extract<'py>(py: Python<'py>, file: &str, source: &str) -> PyResult<Bound<'py, PyAny>> {
    let facts = extract_path(file, source).map_err(|e| PyValueError::new_err(e.to_string()))?;
    pythonize(py, &facts).map_err(Into::into)
}

#[pymodule]
fn code2graph(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    Ok(())
}
