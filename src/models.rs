use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde::Serialize;
use std::collections::HashMap;

#[pyclass]
#[doc = "Instructions and their counts.

This class contains all instructions found in the Dockerfile along with their 
counts. It also incudes the total count.
"]
#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct InstructionStats {
    #[pyo3(get)]
    pub total_count: u32,
    #[pyo3(get)]
    pub by_type: HashMap<String, u32>,
}

#[pymethods]
impl InstructionStats {
    fn __repr__(&self) -> String {
        format!(
            "InstructionStats(total_count={}, by_type={:?})",
            self.total_count, self.by_type
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("total_count", self.total_count)?;
        dict.set_item("by_type", &self.by_type)?;
        Ok(dict.into())
    }
}

#[pyclass]
#[doc = "Parsed components of a Docker image reference.

Attributes:
    registry (str | None): The registry hostname (e.g., 'docker.io')
    name (str): The image name (e.g., 'ubuntu')
    tag (str | None): The image tag (e.g., '20.04')
    digest (str | None): The image digest if specified
"]
#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct ImageComponents {
    #[pyo3(get)]
    pub registry: Option<String>,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub tag: Option<String>,
    #[pyo3(get)]
    pub digest: Option<String>,
}

#[pymethods]
impl ImageComponents {
    fn __repr__(&self) -> String {
        format!(
            "ImageComponents(registry={:?}, name={:?}, tag={:?}, digest={:?})",
            self.registry, self.name, self.tag, self.digest
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("registry", &self.registry)?;
        dict.set_item("name", &self.name)?;
        dict.set_item("tag", &self.tag)?;
        dict.set_item("digest", &self.digest)?;
        Ok(dict.into())
    }
}

#[pyclass]
#[doc = "Information about a Docker image used in a Dockerfile.

Attributes:
    full (str): The complete image reference as it appears in the Dockerfile
    components (ImageComponents | None): Parsed components of the image reference
"]
#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct Image {
    #[pyo3(get)]
    pub full: String,
    #[pyo3(get)]
    pub components: Option<ImageComponents>,
}

#[pymethods]
impl Image {
    fn __repr__(&self) -> String {
        format!(
            "Image(full={:?}, components={:?})",
            self.full,
            match &self.components {
                Some(comp) => comp.__repr__().to_string(),
                None => "None".to_string(),
            }
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("full", &self.full)?;

        let components = match &self.components {
            Some(comp) => Some(comp.to_dict(py)?),
            None => None,
        };
        dict.set_item("components", components)?;
        Ok(dict.into())
    }
}

#[pyclass]
#[doc = "Information about multistage characteristics.

This class contains an is_multistage bool along with information
about specific stages in the Dockerfile.
"]
#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct MultistageAnalysis {
    #[pyo3(get)]
    pub is_multistage: bool,
    #[pyo3(get)]
    pub stages_used_as_base_images: Vec<String>,
    #[pyo3(get)]
    pub stages_copied_from: Vec<String>,
    #[pyo3(get)]
    pub stages_added_from: Vec<String>,
    #[pyo3(get)]
    pub unused_stages: Vec<String>,
}

#[pymethods]
impl MultistageAnalysis {
    fn __repr__(&self) -> String {
        format!(
            "MultistageAnalysis(is_multistage={}, stages_used_as_base_images={:?}, stages_copied_from={:?}, stages_added_from={:?}, unused_stages={:?})",
            self.is_multistage,
            self.stages_used_as_base_images,
            self.stages_copied_from,
            self.stages_added_from,
            self.unused_stages
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("is_multistage", self.is_multistage)?;
        dict.set_item(
            "stages_used_as_base_images",
            &self.stages_used_as_base_images,
        )?;
        dict.set_item("stages_copied_from", &self.stages_copied_from)?;
        dict.set_item("stages_added_from", &self.stages_added_from)?;
        dict.set_item("unused_stages", &self.unused_stages)?;
        Ok(dict.into())
    }
}

#[pyclass]
#[doc = "Represents comprehensive analysis results of a Dockerfile.

This class contains all the extracted information from a Dockerfile including
stages, images, instructions, environment variables, and multistage analysis.
"]
#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct Analysis {
    #[pyo3(get)]
    pub num_stages: usize,
    #[pyo3(get)]
    pub images: Vec<Image>,
    #[pyo3(get)]
    pub stage_names: Vec<String>,
    #[pyo3(get)]
    pub copy_from_stages: Vec<String>,
    #[pyo3(get)]
    pub add_from_stages: Vec<String>,
    #[pyo3(get)]
    pub multistage_analysis: MultistageAnalysis,
    #[pyo3(get)]
    pub exposed_ports: Vec<String>,
    #[pyo3(get)]
    pub instructions: InstructionStats,
    #[pyo3(get)]
    pub args: HashMap<String, Option<String>>,
    #[pyo3(get)]
    pub labels: HashMap<String, String>,
    #[pyo3(get)]
    pub env_vars: HashMap<String, String>,
}

#[pymethods]
impl Analysis {
    fn __repr__(&self) -> String {
        let images_repr: Vec<String> = self.images.iter().map(|img| img.__repr__()).collect();

        format!(
            "Analysis(num_stages={}, images=[{}], stage_names={:?}, copy_from_stages={:?}, add_from_stages={:?}, multistage_analysis={}, exposed_ports={:?}, instructions={}, args={:?}, labels={:?}, env_vars={:?})",
            self.num_stages,
            images_repr.join(", "),
            self.stage_names,
            self.copy_from_stages,
            self.add_from_stages,
            self.multistage_analysis.__repr__(),
            self.exposed_ports,
            self.instructions.__repr__(),
            self.args,
            self.labels,
            self.env_vars
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("num_stages", self.num_stages)?;

        // Convert Vec<Image> to Vec<PyObject>
        let images: PyResult<Vec<PyObject>> =
            self.images.iter().map(|img| img.to_dict(py)).collect();
        dict.set_item("images", images?)?;

        dict.set_item("stage_names", &self.stage_names)?;
        dict.set_item("copy_from_stages", &self.copy_from_stages)?;
        dict.set_item("add_from_stages", &self.add_from_stages)?;
        dict.set_item("multistage_analysis", self.multistage_analysis.to_dict(py)?)?;
        dict.set_item("exposed_ports", &self.exposed_ports)?;
        dict.set_item("instructions", self.instructions.to_dict(py)?)?;
        dict.set_item("args", &self.args)?;
        dict.set_item("labels", &self.labels)?;
        dict.set_item("env_vars", &self.env_vars)?;
        Ok(dict.into())
    }
}

#[pyclass]
#[doc = "Key/Value Pairs found in ARG, ENV, and LABEL instructions.
"]
#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct KeyValueInstr {
    #[pyo3(get)]
    pub args: HashMap<String, Option<String>>,
    #[pyo3(get)]
    pub labels: HashMap<String, String>,
    #[pyo3(get)]
    pub env_vars: HashMap<String, String>,
}

#[pymethods]
impl KeyValueInstr {
    fn __repr__(&self) -> String {
        format!(
            "KeyValueInstr(args={:?}, labels={:?}, env_vars={:?})",
            self.args, self.labels, self.env_vars
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("args", &self.args)?;
        dict.set_item("labels", &self.labels)?;
        dict.set_item("env_vars", &self.env_vars)?;
        Ok(dict.into())
    }
}
