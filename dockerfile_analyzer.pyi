# flake8: noqa: PYI021
def analyze_dockerfile(body: str) -> Analysis:
    """
    Analyzes a Dockerfile and returns detailed analysis information.

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
        >>> analysis = analyze_dockerfile('FROM ubuntu:20.04\nRUN echo hello')
        >>> print(analysis.num_stages)
        1

    """

from typing import Dict, List, Optional, Any

class Analysis:
    num_stages: int
    images: List[Image]
    stage_names: List[str]
    copy_from_stages: List[str]
    add_from_stages: List[str]
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

class KeyValueInstr:
    args: Dict[str, Optional[str]]
    labels: Dict[str, str]
    env_vars: Dict[str, str]

    def to_dict(self) -> Dict[str, Any]: ...
    def __repr__(self) -> str: ...
