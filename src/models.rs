use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::BTreeMap;

fn fmt_opt(value: &Option<String>) -> String {
    match value {
        Some(text) => format!("{text:?}"),
        None => "None".to_string(),
    }
}

fn fmt_opt_map(map: &BTreeMap<String, Option<String>>) -> String {
    let body = map
        .iter()
        .map(|(key, value)| format!("{key:?}: {}", fmt_opt(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{body}}}")
}

#[pyclass(skip_from_py_object)]
#[doc = "Instructions and their counts.

Counts are keyed by the Dockerfile instruction name (`FROM`, `RUN`, ...).
`by_type` is sorted by instruction name. A future instruction the parser does
not yet model is counted as `UNKNOWN` rather than an empty key.
"]
#[derive(Debug, PartialEq, Clone)]
pub struct InstructionStats {
    #[pyo3(get)]
    pub total_count: u32,
    #[pyo3(get)]
    pub by_type: BTreeMap<String, u32>,
}

#[pymethods]
impl InstructionStats {
    fn __repr__(&self) -> String {
        format!(
            "InstructionStats(total_count={}, by_type={:?})",
            self.total_count, self.by_type
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("total_count", self.total_count)?;
        dict.set_item("by_type", &self.by_type)?;
        Ok(dict.into())
    }
}

#[pyclass(skip_from_py_object)]
#[doc = "Parsed components of a Docker image reference.

Attributes:
    registry (str | None): The registry hostname (e.g., 'docker.io')
    name (str): The image name (e.g., 'ubuntu')
    tag (str | None): The image tag (e.g., '20.04')
    digest (str | None): The image digest if specified, including short digests
"]
#[derive(Debug, PartialEq, Clone)]
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
            "ImageComponents(registry={}, name={:?}, tag={}, digest={})",
            fmt_opt(&self.registry),
            self.name,
            fmt_opt(&self.tag),
            fmt_opt(&self.digest)
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("registry", &self.registry)?;
        dict.set_item("name", &self.name)?;
        dict.set_item("tag", &self.tag)?;
        dict.set_item("digest", &self.digest)?;
        Ok(dict.into())
    }
}

#[pyclass(skip_from_py_object)]
#[doc = "Information about a Docker image used in a Dockerfile.

Attributes:
    full (str): The image reference. Non-variable references are lowercased.
    components (ImageComponents | None): Parsed components, or None when the
        reference contains an unexpanded variable.
"]
#[derive(Debug, PartialEq, Clone)]
pub struct Image {
    #[pyo3(get)]
    pub full: String,
    #[pyo3(get)]
    pub components: Option<ImageComponents>,
}

#[pymethods]
impl Image {
    fn __repr__(&self) -> String {
        let components = match &self.components {
            Some(components) => components.__repr__(),
            None => "None".to_string(),
        };
        format!("Image(full={:?}, components={components})", self.full)
    }

    fn to_dict(&self, py: Python) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("full", &self.full)?;
        let components = match &self.components {
            Some(components) => Some(components.to_dict(py)?),
            None => None,
        };
        dict.set_item("components", components)?;
        Ok(dict.into())
    }
}

#[pyclass(skip_from_py_object)]
#[doc = "How named stages are referenced by later instructions.

`is_multistage` is true when the Dockerfile has at least two stages and a
later instruction uses an earlier stage as a base image or as a COPY/ADD
`--from` source. An index such as `--from=0` counts. An unused named stage is
one that no later instruction references. The final stage is often unused in
that sense: it is the image the build produces.
"]
#[derive(Debug, PartialEq, Clone)]
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

    fn to_dict(&self, py: Python) -> PyResult<Py<PyAny>> {
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

#[pyclass(skip_from_py_object)]
#[doc = "One build stage, from its `FROM` line to the next `FROM`.

Values are the stage's own, as written in that stage. Nothing is inherited
from a parent stage, and global `ARG`s declared before the first `FROM` are
not attributed to any stage.

Attributes:
    index (int): Zero-based position of the stage in the Dockerfile.
    name (str | None): The lowercased `AS` name, or None when unnamed.
    base_image (str): The `FROM` reference. It is lowercased unless it
        contains `$`, matching `Analysis.images`.
    parent_stage (str | None): The name of the earlier stage this `FROM`
        builds on, or None. Only earlier stages are matched, as in Docker; a
        name defined later, or any other name, is treated as an image.
    platform (str | None): The `--platform` value as written, or None.
    args (dict[str, str | None]): `ARG`s declared in this stage. A
        redeclaration without a default is None here.
    env_vars (dict[str, str]): `ENV` values set in this stage.
    labels (dict[str, str]): `LABEL` values set in this stage.
    exposed_ports (list[str]): `EXPOSE` tokens in this stage, sorted.
    user (str | None): The last `USER` in this stage, or None.
    workdir (str | None): The last `WORKDIR` in this stage, or None.
"]
#[derive(Debug, PartialEq, Clone)]
pub struct Stage {
    #[pyo3(get)]
    pub index: usize,
    #[pyo3(get)]
    pub name: Option<String>,
    #[pyo3(get)]
    pub base_image: String,
    #[pyo3(get)]
    pub parent_stage: Option<String>,
    #[pyo3(get)]
    pub platform: Option<String>,
    #[pyo3(get)]
    pub args: BTreeMap<String, Option<String>>,
    #[pyo3(get)]
    pub env_vars: BTreeMap<String, String>,
    #[pyo3(get)]
    pub labels: BTreeMap<String, String>,
    #[pyo3(get)]
    pub exposed_ports: Vec<String>,
    #[pyo3(get)]
    pub user: Option<String>,
    #[pyo3(get)]
    pub workdir: Option<String>,
}

impl Stage {
    pub fn repr(&self) -> String {
        self.__repr__()
    }
}

#[pymethods]
impl Stage {
    fn __repr__(&self) -> String {
        format!(
            "Stage(index={}, name={}, base_image={:?}, parent_stage={}, platform={}, args={}, env_vars={:?}, labels={:?}, exposed_ports={:?}, user={}, workdir={})",
            self.index,
            fmt_opt(&self.name),
            self.base_image,
            fmt_opt(&self.parent_stage),
            fmt_opt(&self.platform),
            fmt_opt_map(&self.args),
            self.env_vars,
            self.labels,
            self.exposed_ports,
            fmt_opt(&self.user),
            fmt_opt(&self.workdir)
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("index", self.index)?;
        dict.set_item("name", &self.name)?;
        dict.set_item("base_image", &self.base_image)?;
        dict.set_item("parent_stage", &self.parent_stage)?;
        dict.set_item("platform", &self.platform)?;
        dict.set_item("args", &self.args)?;
        dict.set_item("env_vars", &self.env_vars)?;
        dict.set_item("labels", &self.labels)?;
        dict.set_item("exposed_ports", &self.exposed_ports)?;
        dict.set_item("user", &self.user)?;
        dict.set_item("workdir", &self.workdir)?;
        Ok(dict.into())
    }
}

#[pyclass(skip_from_py_object)]
#[doc = "Analysis of one Dockerfile.

Image references that do not contain `$` are lowercased, so `Ubuntu:22.04`
and `ubuntu:22.04` are one image. References that contain `$` are not
expanded. A reference whose first character is `$` has `components` set to
None; other unresolved references also have `components` set to None. `EXPOSE` values are raw tokens,
including protocol suffixes and ranges.

`stage_names` lists only stages with an `AS` name, sorted. `final_stage` is
the last stage's name, or None when that stage is unnamed. An unnamed stage
cannot be referenced by name. When `COPY --from` or `ADD --from` uses an
in-range index of an unnamed stage, the index is reported in place of a name.

`images` contains `FROM` images. External `COPY --from` and `ADD --from`
images are `copy_from_images` and `add_from_images`. A `--from` value that
names a stage, or an in-range stage index, is a stage reference rather than
an image.

`args`, `labels`, `env_vars`, and `exposed_ports` are merged across all
stages (and, for `args`, global `ARG`s before the first `FROM`); a later
stage's value for a key replaces an earlier one. Use `stages` for the values
each stage sets. `stages` lists every stage in Dockerfile order.
`final_stage_info` is the last `Stage`, or None when there are no stages;
`final_stage` keeps returning only the last stage's name.
"]
#[derive(Debug, PartialEq, Clone)]
pub struct Analysis {
    #[pyo3(get)]
    pub num_stages: usize,
    #[pyo3(get)]
    pub images: Vec<Image>,
    #[pyo3(get)]
    pub stage_names: Vec<String>,
    #[pyo3(get)]
    pub final_stage: Option<String>,
    #[pyo3(get)]
    pub copy_from_stages: Vec<String>,
    #[pyo3(get)]
    pub add_from_stages: Vec<String>,
    #[pyo3(get)]
    pub copy_from_images: Vec<Image>,
    #[pyo3(get)]
    pub add_from_images: Vec<Image>,
    #[pyo3(get)]
    pub multistage_analysis: MultistageAnalysis,
    #[pyo3(get)]
    pub exposed_ports: Vec<String>,
    #[pyo3(get)]
    pub instructions: InstructionStats,
    #[pyo3(get)]
    pub args: BTreeMap<String, Option<String>>,
    #[pyo3(get)]
    pub labels: BTreeMap<String, String>,
    #[pyo3(get)]
    pub env_vars: BTreeMap<String, String>,
    #[pyo3(get)]
    pub stages: Vec<Stage>,
}

impl Analysis {
    pub fn repr(&self) -> String {
        self.__repr__()
    }
}

#[pymethods]
impl Analysis {
    #[getter]
    pub fn final_stage_info(&self) -> Option<Stage> {
        self.stages.last().cloned()
    }

    fn __repr__(&self) -> String {
        let images = join_images(&self.images);
        let copy_from_images = join_images(&self.copy_from_images);
        let add_from_images = join_images(&self.add_from_images);
        format!(
            "Analysis(num_stages={}, images=[{images}], stage_names={:?}, final_stage={}, copy_from_stages={:?}, add_from_stages={:?}, copy_from_images=[{copy_from_images}], add_from_images=[{add_from_images}], multistage_analysis={}, exposed_ports={:?}, instructions={}, args={}, labels={:?}, env_vars={:?})",
            self.num_stages,
            self.stage_names,
            fmt_opt(&self.final_stage),
            self.copy_from_stages,
            self.add_from_stages,
            self.multistage_analysis.__repr__(),
            self.exposed_ports,
            self.instructions.__repr__(),
            fmt_opt_map(&self.args),
            self.labels,
            self.env_vars
        )
    }

    fn to_dict(&self, py: Python) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("num_stages", self.num_stages)?;
        dict.set_item("images", images_to_dicts(py, &self.images)?)?;
        dict.set_item("stage_names", &self.stage_names)?;
        dict.set_item("final_stage", &self.final_stage)?;
        dict.set_item("copy_from_stages", &self.copy_from_stages)?;
        dict.set_item("add_from_stages", &self.add_from_stages)?;
        dict.set_item(
            "copy_from_images",
            images_to_dicts(py, &self.copy_from_images)?,
        )?;
        dict.set_item(
            "add_from_images",
            images_to_dicts(py, &self.add_from_images)?,
        )?;
        dict.set_item("multistage_analysis", self.multistage_analysis.to_dict(py)?)?;
        dict.set_item("exposed_ports", &self.exposed_ports)?;
        dict.set_item("instructions", self.instructions.to_dict(py)?)?;
        dict.set_item("args", &self.args)?;
        dict.set_item("labels", &self.labels)?;
        dict.set_item("env_vars", &self.env_vars)?;
        let stages = self
            .stages
            .iter()
            .map(|stage| stage.to_dict(py))
            .collect::<PyResult<Vec<_>>>()?;
        dict.set_item("stages", stages)?;
        Ok(dict.into())
    }
}

fn join_images(images: &[Image]) -> String {
    images
        .iter()
        .map(Image::__repr__)
        .collect::<Vec<_>>()
        .join(", ")
}

fn images_to_dicts(py: Python, images: &[Image]) -> PyResult<Vec<Py<PyAny>>> {
    images.iter().map(|image| image.to_dict(py)).collect()
}
