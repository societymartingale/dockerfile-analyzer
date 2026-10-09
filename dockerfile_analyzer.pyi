# flake8: noqa: PYI021
def analyze_dockerfile(body: str) -> Analysis:
    """
    Analyzes a Dockerfile and returns detailed analysis information.

    Args:
        dockerfile_content (str): The content of the Dockerfile to analyze

    Returns:
        Analysis: Stages, images, instruction counts, ARG/ENV/LABEL values,
        and exposed ports. ``COPY --from`` and ``ADD --from`` indexes are
        resolved to stages. External ``--from`` images are returned separately
        from stage names.

        Image references that do not contain a variable are lowercased.
        ``${VAR}`` and ``$VAR`` are not expanded. ``EXPOSE`` values are raw
        tokens, including protocol suffixes and ranges. ``stage_names`` lists
        named stages only; ``final_stage`` is the last stage name, or None
        when it is unnamed. A later ``ARG NAME`` without a value keeps an
        earlier default.

        ``args``, ``labels``, ``env_vars``, and ``exposed_ports`` are merged
        across all stages. ``stages`` lists each stage with the values it sets
        itself, and ``final_stage_info`` is the last ``Stage`` (None when
        there are no stages).

    Raises:
        ValueError: If the dockerfile content is empty or invalid

    Example:
        >>> analysis = analyze_dockerfile('FROM ubuntu:20.04\nRUN echo hello')
        >>> print(analysis.num_stages)
        1

    """

from typing import Dict, List, Optional, Any

class Analysis:
    """Analysis of one Dockerfile.

    ``args``, ``labels``, ``env_vars``, and ``exposed_ports`` are merged across
    all stages (``args`` also includes global ``ARG``s before the first
    ``FROM``); a later stage's value for a key replaces an earlier one. Use
    ``stages`` for the values each stage sets. ``final_stage`` is the last
    stage's name (None when unnamed); ``final_stage_info`` is the last
    ``Stage`` object, or None when there are no stages.
    """

    num_stages: int
    images: List[Image]
    stage_names: List[str]
    final_stage: Optional[str]
    copy_from_stages: List[str]
    add_from_stages: List[str]
    copy_from_images: List[Image]
    add_from_images: List[Image]
    multistage_analysis: MultistageAnalysis
    exposed_ports: List[str]
    instructions: InstructionStats
    args: Dict[str, Optional[str]]
    labels: Dict[str, str]
    env_vars: Dict[str, str]
    stages: List[Stage]

    @property
    def final_stage_info(self) -> Optional[Stage]: ...
    def to_dict(self) -> Dict[str, Any]: ...
    def __repr__(self) -> str: ...

class Stage:
    """One build stage, from its ``FROM`` line to the next ``FROM``.

    Values are the stage's own. Nothing is inherited from a parent stage, and
    global ``ARG``s declared before the first ``FROM`` are not attributed to
    any stage. ``parent_stage`` is set only when ``FROM`` names an earlier
    stage; a later or unknown name is treated as an image.
    """

    index: int
    name: Optional[str]
    base_image: str
    parent_stage: Optional[str]
    platform: Optional[str]
    args: Dict[str, Optional[str]]
    env_vars: Dict[str, str]
    labels: Dict[str, str]
    exposed_ports: List[str]
    user: Optional[str]
    workdir: Optional[str]

    def to_dict(self) -> Dict[str, Any]: ...
    def __repr__(self) -> str: ...

class Image:
    full: str
    components: Optional[ImageComponents]

    def to_dict(self) -> Dict[str, Any]: ...
    def __repr__(self) -> str: ...

class ImageComponents:
    registry: Optional[str]
    name: str
    tag: Optional[str]
    digest: Optional[str]

    def to_dict(self) -> Dict[str, Any]: ...
    def __repr__(self) -> str: ...

class InstructionStats:
    total_count: int
    by_type: Dict[str, int]

    def to_dict(self) -> Dict[str, Any]: ...
    def __repr__(self) -> str: ...

class MultistageAnalysis:
    is_multistage: bool
    stages_used_as_base_images: List[str]
    stages_copied_from: List[str]
    stages_added_from: List[str]
    unused_stages: List[str]

    def to_dict(self) -> Dict[str, Any]: ...
    def __repr__(self) -> str: ...
