use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
mod analyzer;
mod constants;
mod models;
mod parse_utils;

#[pyfunction]
#[doc = "Analyzes a Dockerfile and returns detailed analysis information.

Args:
    dockerfile_content (str): The content of the Dockerfile to analyze

Returns:
    Analysis: A comprehensive analysis object containing information about:
        - Number of stages and stage names
        - Base images used
        - Multistage analysis (if applicable)
        - Instructions statistics
        - Environment variables, labels, and arguments
        - Exposed ports

Raises:
    ValueError: If the dockerfile content is empty or invalid

Example:
    >>> analysis = analyze_dockerfile('FROM ubuntu:20.04\\nRUN echo hello')
    >>> print(analysis.num_stages)
    1
"]
fn analyze_dockerfile(body: &str) -> PyResult<models::Analysis> {
    let res = analyzer::analyze_dockerfile(body);
    match res {
        Ok(res) => Ok(res),
        Err(e) => Err(PyValueError::new_err(e.to_string())),
    }
}

/// A Python module implemented in Rust.
#[pymodule]
fn dockerfile_analyzer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(analyze_dockerfile, m)?)?;
    m.add_class::<models::Analysis>()?;
    m.add_class::<models::MultistageAnalysis>()?;
    m.add_class::<models::Image>()?;
    m.add_class::<models::ImageComponents>()?;
    m.add_class::<models::InstructionStats>()?;
    Ok(())
}
