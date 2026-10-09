"""Tests for the per-stage breakdown (Analysis.stages and final_stage_info)."""

from pathlib import Path

import dockerfile_analyzer as da

FIXTURES = Path(__file__).parent / "fixtures"

STAGE_KEYS = {
    "index",
    "name",
    "base_image",
    "parent_stage",
    "platform",
    "args",
    "env_vars",
    "labels",
    "exposed_ports",
    "user",
    "workdir",
}


def test_multistage_fixture_stages():
    analysis = da.analyze_dockerfile((FIXTURES / "multistage.Dockerfile").read_text())
    base, test, final = analysis.stages

    assert [stage.index for stage in analysis.stages] == [0, 1, 2]
    assert [stage.name for stage in analysis.stages] == ["base", "test", None]
    assert base.base_image.startswith("docker.abc.com/base-images/python:3.13-debian@")
    assert base.parent_stage is None
    assert test.base_image == "base"
    assert test.parent_stage == "base"
    assert final.parent_stage == "base"

    assert base.labels["org.opencontainers.image.title"] == "My App"
    assert base.env_vars["PYTHONPATH"] == "/src"
    assert "GIT_COMMIT" not in base.env_vars
    assert base.workdir == "/src"
    assert base.user == "root:root"

    assert test.env_vars == {}
    assert test.labels == {}
    assert test.user == "1000:1000"
    assert test.workdir is None

    assert final.args == {"GIT_COMMIT": None}
    assert final.env_vars == {"GIT_COMMIT": "$GIT_COMMIT"}
    assert final.exposed_ports == ["5000"]
    assert final.platform is None

    # Top-level values remain merged across stages.
    assert set(analysis.env_vars) == set(base.env_vars) | set(final.env_vars)
    assert analysis.final_stage is None
    assert analysis.final_stage_info.to_dict() == final.to_dict()


def test_parent_stage_only_resolves_earlier_stages():
    dockerfile = """\
FROM alpine:3.20 AS Base
FROM base AS child
FROM later AS early
FROM node:20 AS later
FROM unknown-image
FROM child
"""
    stages = da.analyze_dockerfile(dockerfile).stages

    assert [(s.name, s.base_image, s.parent_stage) for s in stages] == [
        ("base", "alpine:3.20", None),
        ("child", "base", "base"),
        ("early", "later", None),
        ("later", "node:20", None),
        (None, "unknown-image", None),
        (None, "child", "child"),
    ]


def test_platform_per_stage():
    dockerfile = """\
FROM --platform=$BUILDPLATFORM golang:1.22 AS build
FROM --platform=linux/amd64 alpine:3.20
"""
    stages = da.analyze_dockerfile(dockerfile).stages
    assert [s.platform for s in stages] == ["$BUILDPLATFORM", "linux/amd64"]


def test_per_stage_values_and_overrides():
    dockerfile = """\
FROM golang:1.22 AS build
ARG TARGETOS=linux
ENV MODE=build
LABEL stage=build
EXPOSE 6060
WORKDIR /src
WORKDIR /src/cmd
USER builder

FROM alpine:3.20
ENV MODE=runtime
LABEL stage=runtime
EXPOSE 8080/tcp
USER root
USER 65532
"""
    analysis = da.analyze_dockerfile(dockerfile)
    build, runtime = analysis.stages

    assert build.args == {"TARGETOS": "linux"}
    assert build.env_vars == {"MODE": "build"}
    assert build.labels == {"stage": "build"}
    assert build.exposed_ports == ["6060"]
    assert build.workdir == "/src/cmd"
    assert build.user == "builder"

    assert runtime.args == {}
    assert runtime.env_vars == {"MODE": "runtime"}
    assert runtime.labels == {"stage": "runtime"}
    assert runtime.exposed_ports == ["8080/tcp"]
    assert runtime.workdir is None
    assert runtime.user == "65532"

    assert analysis.env_vars == {"MODE": "runtime"}
    assert analysis.labels == {"stage": "runtime"}
    assert analysis.exposed_ports == ["6060", "8080/tcp"]
    assert analysis.args == {"TARGETOS": "linux"}


def test_global_args_not_attributed_to_first_stage():
    dockerfile = """\
ARG VERSION=1
FROM alpine:${VERSION} AS base
FROM base
ARG VERSION
"""
    analysis = da.analyze_dockerfile(dockerfile)
    assert analysis.stages[0].args == {}
    assert analysis.stages[1].args == {"VERSION": None}
    assert analysis.args == {"VERSION": "1"}


def test_bare_arg_in_same_stage_clears_default():
    analysis = da.analyze_dockerfile("FROM alpine\nARG VERSION=1\nARG VERSION\n")
    assert analysis.stages[0].args == {"VERSION": None}
    assert analysis.args == {"VERSION": "1"}


def test_final_stage_info_and_final_stage_name():
    analysis = da.analyze_dockerfile("FROM golang:1.22 AS build\nFROM build AS release\n")
    assert analysis.final_stage == "release"
    final = analysis.final_stage_info
    assert isinstance(final, da.Stage)
    assert final.index == 1
    assert final.name == "release"
    assert final.parent_stage == "build"


def test_to_dict_adds_only_stages_key():
    analysis = da.analyze_dockerfile((FIXTURES / "multistage.Dockerfile").read_text())
    data = analysis.to_dict()

    assert list(data) == [
        "num_stages",
        "images",
        "stage_names",
        "final_stage",
        "copy_from_stages",
        "add_from_stages",
        "copy_from_images",
        "add_from_images",
        "multistage_analysis",
        "exposed_ports",
        "instructions",
        "args",
        "labels",
        "env_vars",
        "stages",
    ]
    assert len(data["stages"]) == 3
    assert all(set(stage) == STAGE_KEYS for stage in data["stages"])
    assert data["stages"][2] == analysis.stages[2].to_dict()
    assert data["stages"][1]["parent_stage"] == "base"
    assert data["stages"][2]["name"] is None


def test_stage_repr():
    stage = da.analyze_dockerfile("FROM alpine\nARG VERSION\n").stages[0]
    assert repr(stage) == (
        'Stage(index=0, name=None, base_image="alpine", parent_stage=None, '
        'platform=None, args={"VERSION": None}, env_vars={}, labels={}, '
        "exposed_ports=[], user=None, workdir=None)"
    )
