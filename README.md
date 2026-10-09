# Dockerfile Analyzer

## Description

This Python package analyzes a Dockerfile and returns detailed analysis information. Some of the metadata returned include:

- Number of stages, named stages, and the final stage name
- Base images, including images pulled by `COPY --from` and `ADD --from`
- Multistage analysis, with numeric `--from` indexes resolved to stages
- Instructions statistics
- Environment variables, labels, and arguments
- Exposed ports
- A per-stage breakdown (`stages`, `final_stage_info`): each stage's base image, parent stage, `--platform`, and its own `ARG`, `ENV`, `LABEL`, `EXPOSE`, `USER`, and `WORKDIR` values

Image references that contain `$` are not expanded. Other image references are lowercased. `EXPOSE` values are returned as written, including protocol suffixes and ranges. A later `ARG NAME` without a default keeps an earlier default.

The top-level `args`, `env_vars`, `labels`, and `exposed_ports` are merged across all stages (and `args` also includes global `ARG`s declared before the first `FROM`), so a later stage's value for a key replaces an earlier one. To see which stage set what, use the per-stage breakdown described below.

## Per-stage breakdown

`analysis.stages` is a list of `Stage` objects in Dockerfile order. Each `Stage` has:

| Attribute | Description |
| --- | --- |
| `index` | Zero-based position of the stage. |
| `name` | The lowercased `AS` name, or `None` when the stage is unnamed. |
| `base_image` | The `FROM` reference, normalized like `images` (lowercased unless it contains `$`). |
| `parent_stage` | The name of the earlier stage this `FROM` builds on, or `None`. Docker only resolves earlier stages, so a name defined later, or an unknown name, is treated as an image and has no parent. |
| `platform` | The `FROM --platform` value as written, or `None`. |
| `args`, `env_vars`, `labels`, `exposed_ports` | Values set by this stage's own instructions. Global `ARG`s before the first `FROM` are not attributed to any stage, and nothing is inherited from a parent stage. |
| `user`, `workdir` | The last `USER` and `WORKDIR` in this stage, or `None`. |

`Stage` has `to_dict()` and a readable `repr()`. `analysis.final_stage_info` returns the last `Stage`, or `None` when there are no stages. The existing `analysis.final_stage` is unchanged and still returns only the last stage's name (or `None` when it is unnamed). `analysis.to_dict()` includes the stages under a `stages` key.

The code leverages the highly useful Rust crates `parse-dockerfile` and `docker-image`.

## Example

This Dockerfile is short, but it uses the distinctions the analyzer is built to recover: a stage referenced by index, a stage used as a base image, an unused stage, an external `COPY --from` image, a digest-pinned image, an unexpanded variable, and an `ARG` redeclared without a default.

### Sample Input Dockerfile

```Dockerfile
# syntax=docker/dockerfile:1.7
ARG GO_VERSION=1.22
ARG ALPINE_VERSION=3.20
ARG VERSION=1.0.0

# Stage 0. A later COPY refers to this stage by index.
FROM alpine:${ALPINE_VERSION} AS tools
RUN apk add --no-cache ca-certificates

FROM golang:${GO_VERSION}-alpine AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -o /out/app ./cmd/app

# Uses the build stage as its base image, and is itself an ADD --from source.
FROM build AS assets
ADD README.md /docs/README.md

# Named, but no later instruction references it.
FROM alpine:${ALPINE_VERSION} AS debug
RUN apk add --no-cache busybox

# Unnamed, so final_stage is None. Digest is split into components.
FROM gcr.io/distroless/static-debian12:nonroot@sha256:2c8e1f5d9a3b7e6c4d1f0a8b5e2c9d7f3a6b1c4e8d2f5a9b3c7e1d4f6a8b0c2e
COPY --from=0 /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=build /out/app /app
ADD --from=assets /docs/README.md /docs/README.md
COPY --from=ghcr.io/acme/assets:1.4 /share /share
# Redeclared with no default, so the global default is kept.
ARG VERSION
ENV APP_VERSION=$VERSION
LABEL org.opencontainers.image.title="app" \
      org.opencontainers.image.version="${VERSION}"
EXPOSE 8080/tcp 9090/udp
USER nonroot
ENTRYPOINT ["/app"]
CMD ["serve"]
```

### Sample Python Code

```sh
pip install dockerfile-analyzer
```

```python
import json

import dockerfile_analyzer as da

dockerfile = open("Dockerfile").read()
analysis = da.analyze_dockerfile(dockerfile)

# --from=0 resolved to the tools stage. ghcr.io is an image, not a stage.
print(analysis.copy_from_stages)  # ['build', 'tools']
print(analysis.copy_from_images[0].components.registry)  # ghcr.io

# debug is named and never referenced. The final stage is unnamed.
print(analysis.multistage_analysis.unused_stages)  # ['debug']
print(analysis.final_stage)  # None

# The second ARG VERSION has no default, so 1.0.0 is kept.
print(analysis.args["VERSION"])  # 1.0.0

# Per-stage values. Global ARGs before the first FROM belong to no stage.
print(analysis.stages[0].args)  # {}
print(analysis.stages[2].parent_stage)  # build
final = analysis.final_stage_info
print(final.index, final.user, final.exposed_ports)  # 4 nonroot ['8080/tcp', '9090/udp']

print(json.dumps(analysis.to_dict(), indent=2))
```

### Output

```
['build', 'tools']
ghcr.io
['debug']
None
1.0.0
{}
build
4 nonroot ['8080/tcp', '9090/udp']

```json
{
  "num_stages": 5,
  "images": [
    {
      "full": "alpine:${ALPINE_VERSION}",
      "components": null
    },
    {
      "full": "build",
      "components": {
        "registry": null,
        "name": "build",
        "tag": null,
        "digest": null
      }
    },
    {
      "full": "gcr.io/distroless/static-debian12:nonroot@sha256:2c8e1f5d9a3b7e6c4d1f0a8b5e2c9d7f3a6b1c4e8d2f5a9b3c7e1d4f6a8b0c2e",
      "components": {
        "registry": "gcr.io",
        "name": "distroless/static-debian12",
        "tag": "nonroot",
        "digest": "sha256:2c8e1f5d9a3b7e6c4d1f0a8b5e2c9d7f3a6b1c4e8d2f5a9b3c7e1d4f6a8b0c2e"
      }
    },
    {
      "full": "golang:${GO_VERSION}-alpine",
      "components": null
    }
  ],
  "stage_names": [
    "assets",
    "build",
    "debug",
    "tools"
  ],
  "final_stage": null,
  "copy_from_stages": [
    "build",
    "tools"
  ],
  "add_from_stages": [
    "assets"
  ],
  "copy_from_images": [
    {
      "full": "ghcr.io/acme/assets:1.4",
      "components": {
        "registry": "ghcr.io",
        "name": "acme/assets",
        "tag": "1.4",
        "digest": null
      }
    }
  ],
  "add_from_images": [],
  "multistage_analysis": {
    "is_multistage": true,
    "stages_used_as_base_images": [
      "build"
    ],
    "stages_copied_from": [
      "build",
      "tools"
    ],
    "stages_added_from": [
      "assets"
    ],
    "unused_stages": [
      "debug"
    ]
  },
  "exposed_ports": [
    "8080/tcp",
    "9090/udp"
  ],
  "instructions": {
    "total_count": 27,
    "by_type": {
      "ADD": 2,
      "ARG": 4,
      "CMD": 1,
      "COPY": 5,
      "ENTRYPOINT": 1,
      "ENV": 1,
      "EXPOSE": 1,
      "FROM": 5,
      "LABEL": 1,
      "RUN": 4,
      "USER": 1,
      "WORKDIR": 1
    }
  },
  "args": {
    "ALPINE_VERSION": "3.20",
    "GO_VERSION": "1.22",
    "VERSION": "1.0.0"
  },
  "labels": {
    "org.opencontainers.image.title": "app",
    "org.opencontainers.image.version": "${VERSION}"
  },
  "env_vars": {
    "APP_VERSION": "$VERSION"
  },
  "stages": [
    {
      "index": 0,
      "name": "tools",
      "base_image": "alpine:${ALPINE_VERSION}",
      "parent_stage": null,
      "platform": null,
      "args": {},
      "env_vars": {},
      "labels": {},
      "exposed_ports": [],
      "user": null,
      "workdir": null
    },
    {
      "index": 1,
      "name": "build",
      "base_image": "golang:${GO_VERSION}-alpine",
      "parent_stage": null,
      "platform": null,
      "args": {},
      "env_vars": {},
      "labels": {},
      "exposed_ports": [],
      "user": null,
      "workdir": "/src"
    },
    {
      "index": 2,
      "name": "assets",
      "base_image": "build",
      "parent_stage": "build",
      "platform": null,
      "args": {},
      "env_vars": {},
      "labels": {},
      "exposed_ports": [],
      "user": null,
      "workdir": null
    },
    {
      "index": 3,
      "name": "debug",
      "base_image": "alpine:${ALPINE_VERSION}",
      "parent_stage": null,
      "platform": null,
      "args": {},
      "env_vars": {},
      "labels": {},
      "exposed_ports": [],
      "user": null,
      "workdir": null
    },
    {
      "index": 4,
      "name": null,
      "base_image": "gcr.io/distroless/static-debian12:nonroot@sha256:2c8e1f5d9a3b7e6c4d1f0a8b5e2c9d7f3a6b1c4e8d2f5a9b3c7e1d4f6a8b0c2e",
      "parent_stage": null,
      "platform": null,
      "args": {
        "VERSION": null
      },
      "env_vars": {
        "APP_VERSION": "$VERSION"
      },
      "labels": {
        "org.opencontainers.image.title": "app",
        "org.opencontainers.image.version": "${VERSION}"
      },
      "exposed_ports": [
        "8080/tcp",
        "9090/udp"
      ],
      "user": "nonroot",
      "workdir": null
    }
  ]
}
```

## Developing

```sh
python3 -m venv env
source env/bin/activate
pip install -r requirements-dev.txt
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
maturin develop
pytest
```

`cargo test` and `cargo clippy --all-targets` link against libpython. `.cargo/config.toml` points PyO3 at `env/bin/python`, which must be a shared Python build (`libpython3.x.so` present). `PYO3_PYTHON` overrides that path when set.

