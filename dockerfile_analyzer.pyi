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

    Raises:
        ValueError: If the dockerfile content is empty or invalid

    Example:
        >>> analysis = analyze_dockerfile('FROM ubuntu:20.04\nRUN echo hello')
        >>> print(analysis.num_stages)
        1

    """

from typing import Dict, List, Optional, Any

class Analysis:
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
