"""Python integration tests for the dockerfile_analyzer extension module."""

from pathlib import Path

import pytest

import dockerfile_analyzer as da

FIXTURES = Path(__file__).parent / "fixtures"


def _read_fixture(name: str) -> str:
    return (FIXTURES / name).read_text()


def test_analyze_multistage_dockerfile():
    analysis = da.analyze_dockerfile(_read_fixture("multistage.Dockerfile"))

    assert analysis.num_stages == 3
    assert analysis.stage_names == ["base", "test"]
    assert analysis.copy_from_stages == []
    assert analysis.add_from_stages == []
    assert analysis.exposed_ports == ["5000"]

    assert analysis.multistage_analysis.is_multistage is True
    assert analysis.multistage_analysis.stages_used_as_base_images == ["base"]
    assert analysis.multistage_analysis.stages_copied_from == []
    assert analysis.multistage_analysis.stages_added_from == []
    assert analysis.multistage_analysis.unused_stages == ["test"]

    assert analysis.instructions.total_count == 22
    assert analysis.instructions.by_type == {
        "ARG": 1,
        "CMD": 1,
        "COPY": 5,
        "ENV": 2,
        "EXPOSE": 1,
        "FROM": 3,
        "LABEL": 1,
        "RUN": 4,
        "USER": 3,
        "WORKDIR": 1,
    }

    assert analysis.args == {"GIT_COMMIT": None}
    assert analysis.labels == {
        "org.opencontainers.image.title": "My App",
        "org.opencontainers.image.version": "1.0",
        "org.opencontainers.image.authors": "john@example.com",
    }
    assert analysis.env_vars == {
        "PYTHONPATH": "/src",
        "PYTHONUNBUFFERED": "1",
        "REQUESTS_CA_BUNDLE": "/etc/ssl/certs/ca-certificates.crt",
        "PATH": "/home/appuser/.local/bin:$PATH",
        "GIT_COMMIT": "$GIT_COMMIT",
    }

    assert len(analysis.images) == 2
    base_stage_image, external_image = analysis.images

    assert base_stage_image.full == "base"
    assert base_stage_image.components is not None
    assert base_stage_image.components.registry is None
    assert base_stage_image.components.name == "base"
    assert base_stage_image.components.tag is None
    assert base_stage_image.components.digest is None

    assert (
        external_image.full
        == "docker.abc.com/base-images/python:3.13-debian@sha256:55f1d15ef4c37870e23c03e89ad238940b55c8ede9f13fac4b7d71c7955f1053"
    )
    assert external_image.components is not None
    assert external_image.components.registry == "docker.abc.com"
    assert external_image.components.name == "base-images/python"
    assert external_image.components.tag == "3.13-debian"
    assert (
        external_image.components.digest
        == "sha256:55f1d15ef4c37870e23c03e89ad238940b55c8ede9f13fac4b7d71c7955f1053"
    )


def test_analyze_multistage_to_dict():
    analysis = da.analyze_dockerfile(_read_fixture("multistage.Dockerfile"))
    data = analysis.to_dict()

    assert data["num_stages"] == 3
    assert data["stage_names"] == ["base", "test"]
    assert data["exposed_ports"] == ["5000"]
    assert data["multistage_analysis"]["is_multistage"] is True
    assert data["multistage_analysis"]["unused_stages"] == ["test"]
    assert data["instructions"]["total_count"] == 22
    assert data["args"]["GIT_COMMIT"] is None
    assert data["env_vars"]["PYTHONPATH"] == "/src"
    assert data["images"][1]["components"]["registry"] == "docker.abc.com"


def test_analyze_single_stage_dockerfile():
    dockerfile = """\
FROM node:20-alpine
WORKDIR /app
COPY package*.json ./
RUN npm install
COPY . .
EXPOSE 3000
CMD ["npm", "start"]
"""
    analysis = da.analyze_dockerfile(dockerfile)

    assert analysis.num_stages == 1
    assert analysis.stage_names == []
    assert analysis.multistage_analysis.is_multistage is False
    assert analysis.exposed_ports == ["3000"]
    assert analysis.instructions.by_type["FROM"] == 1
    assert analysis.instructions.by_type["RUN"] == 1
    assert analysis.images[0].full == "node:20-alpine"
    assert analysis.images[0].components is not None
    assert analysis.images[0].components.name == "node"
    assert analysis.images[0].components.tag == "20-alpine"


def test_analyze_invalid_dockerfile_raises():
    with pytest.raises(ValueError, match="unknown instruction"):
        da.analyze_dockerfile("invalid dockerfile content")


def test_analyze_empty_dockerfile_raises():
    with pytest.raises(ValueError):
        da.analyze_dockerfile("")
