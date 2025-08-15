// This logic does not handle numeric stages such as "COPY --from=1".
// Dockerfiles should now use named stages rather than numeric stages.

use crate::constants;
use crate::models;
use crate::models::KeyValueInstr;
use crate::parse_utils;
use docker_image::DockerImage;
use parse_dockerfile::{AddInstruction, CopyInstruction, Instruction, Stage, parse};
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::error::Error;

trait HasOptions {
    fn options(&self) -> &[parse_dockerfile::Flag<'_>];
}

impl HasOptions for CopyInstruction<'_> {
    fn options(&self) -> &[parse_dockerfile::Flag<'_>] {
        &self.options
    }
}

impl HasOptions for AddInstruction<'_> {
    fn options(&self) -> &[parse_dockerfile::Flag<'_>] {
        &self.options
    }
}

fn get_from_flag_val<T: HasOptions>(instruction: &T) -> Option<String> {
    for flag in instruction.options() {
        let flag_name = &flag.name.value;
        let flag_val = flag.value.as_ref().map(|v| &v.value);
        if flag_name.as_ref() == constants::FROM
            && let Some(from_value) = flag_val
        {
            return Some(from_value.to_string());
        }
    }
    None
}

fn analyze_multistage(
    num_stages: usize,
    images: &BTreeSet<String>,
    stage_names: &BTreeSet<String>,
    copy_from_stages: &BTreeSet<String>,
    add_from_stages: &BTreeSet<String>,
) -> models::MultistageAnalysis {
    let stages_used_as_base_images: BTreeSet<String> =
        stage_names.intersection(images).cloned().collect();

    let stages_copied_from: BTreeSet<String> = stage_names
        .intersection(copy_from_stages)
        .cloned()
        .collect();

    let stages_added_from: BTreeSet<String> =
        stage_names.intersection(add_from_stages).cloned().collect();

    let used_stages: BTreeSet<String> = stages_used_as_base_images
        .iter()
        .chain(stages_copied_from.iter())
        .chain(stages_added_from.iter())
        .cloned()
        .collect();

    let unused_stages = stage_names.difference(&used_stages);
    let is_multistage = num_stages >= 2 && !used_stages.is_empty();

    models::MultistageAnalysis {
        is_multistage,
        stages_used_as_base_images: stages_used_as_base_images.into_iter().collect(),
        stages_copied_from: stages_copied_from.into_iter().collect(),
        stages_added_from: stages_added_from.into_iter().collect(),
        unused_stages: unused_stages.into_iter().cloned().collect(),
    }
}

fn get_parsed_images(images: &BTreeSet<String>) -> Vec<models::Image> {
    let mut parsed_images: Vec<models::Image> = vec![];
    for img in images {
        if let Ok(parsed) = DockerImage::parse(img) {
            let components = models::ImageComponents {
                registry: parsed.registry,
                name: parsed.name,
                tag: parsed.tag,
                digest: parsed.digest,
            };
            parsed_images.push(models::Image {
                full: img.clone(),
                components: Some(components),
            });
        } else {
            parsed_images.push(models::Image {
                full: img.clone(),
                components: None,
            })
        }
    }

    parsed_images
}

pub fn analyze_dockerfile(body: &str) -> Result<models::Analysis, Box<dyn Error>> {
    let df = parse(body)?;
    let stages: Vec<_> = df.stages().collect();
    let num_stages = stages.len();

    let (images, stage_names) = extract_stage_info(&stages);
    let (copy_from_stages, add_from_stages) = extract_from_references(&df.instructions);

    let multistage_analysis = analyze_multistage(
        num_stages,
        &images,
        &stage_names,
        &copy_from_stages,
        &add_from_stages,
    );

    let parsed_images: Vec<models::Image> = get_parsed_images(&images);
    let exposed_ports = extract_ports(&df.instructions);
    let instructions = extract_instructions(&df.instructions);
    let kv_pairs = extract_key_value_pairs(&df.instructions);

    Ok(models::Analysis {
        num_stages,
        images: parsed_images,
        stage_names: stage_names.into_iter().collect(),
        copy_from_stages: copy_from_stages.into_iter().collect(),
        add_from_stages: add_from_stages.into_iter().collect(),
        multistage_analysis,
        exposed_ports: exposed_ports.into_iter().collect(),
        instructions,
        args: kv_pairs.args,
        labels: kv_pairs.labels,
        env_vars: kv_pairs.env_vars,
    })
}

fn extract_key_value_pairs(instructions: &[Instruction]) -> models::KeyValueInstr {
    let mut args: HashMap<String, Option<String>> = HashMap::new();
    let mut labels: HashMap<String, String> = HashMap::new();
    let mut env_vars: HashMap<String, String> = HashMap::new();

    for ins in instructions {
        match ins {
            Instruction::Arg(a) => args.extend(parse_utils::parse_kv_instruction_opt_val(
                a.arguments.value.as_ref(),
            )),
            Instruction::Label(l) => labels.extend(parse_utils::parse_kv_instruction(
                l.arguments.value.as_ref(),
            )),
            Instruction::Env(e) => env_vars.extend(parse_utils::parse_kv_instruction(
                e.arguments.value.as_ref(),
            )),
            _ => {}
        }
    }

    KeyValueInstr {
        args,
        labels,
        env_vars,
    }
}

fn extract_instructions(instructions: &[Instruction]) -> models::InstructionStats {
    let mut by_type = HashMap::new();
    for ins in instructions {
        let s = match ins {
            Instruction::Add(..) => constants::ADD.to_string(),
            Instruction::Arg(..) => constants::ARG.to_string(),
            Instruction::Cmd(..) => constants::CMD.to_string(),
            Instruction::Copy(..) => constants::COPY.to_string(),
            Instruction::Entrypoint(..) => constants::ENTRYPOINT.to_string(),
            Instruction::Env(..) => constants::ENV.to_string(),
            Instruction::Expose(..) => constants::EXPOSE.to_string(),
            Instruction::From(..) => constants::FROM_UC.to_string(),
            Instruction::Healthcheck(..) => constants::HEALTHCHECK.to_string(),
            Instruction::Label(..) => constants::LABEL.to_string(),
            Instruction::Maintainer(..) => constants::MAINTAINER.to_string(),
            Instruction::Onbuild(..) => constants::ONBUILD.to_string(),
            Instruction::Run(..) => constants::RUN.to_string(),
            Instruction::Shell(..) => constants::SHELL.to_string(),
            Instruction::Stopsignal(..) => constants::STOPSIGNAL.to_string(),
            Instruction::User(..) => constants::USER.to_string(),
            Instruction::Volume(..) => constants::VOLUME.to_string(),
            Instruction::Workdir(..) => constants::WORKDIR.to_string(),
            &_ => "".to_string(),
        };
        *by_type.entry(s).or_insert(0) += 1;
    }

    models::InstructionStats {
        total_count: instructions.len() as u32,
        by_type,
    }
}

fn extract_ports(instructions: &[Instruction]) -> BTreeSet<String> {
    let mut all_ports = BTreeSet::new();
    for ins in instructions {
        let mut ports = match ins {
            Instruction::Expose(exp) => exp.arguments.iter().map(|x| x.value.to_string()).collect(),
            _ => BTreeSet::new(),
        };
        all_ports.append(&mut ports);
    }

    all_ports
}

fn extract_stage_info(stages: &[Stage]) -> (BTreeSet<String>, BTreeSet<String>) {
    let images = stages
        .iter()
        .map(|s| {
            let value = s.from.image.value.to_string();
            match value.starts_with('$') {
                true => value,
                false => value.to_lowercase(),
            }
        })
        .collect();

    let stage_names = stages
        .iter()
        .filter_map(|s| s.from.as_.as_ref())
        .map(|stage_name| stage_name.1.value.to_string().to_lowercase())
        .collect();

    (images, stage_names)
}

fn extract_from_references(instructions: &[Instruction]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut copy_from_stages = BTreeSet::new();
    let mut add_from_stages = BTreeSet::new();

    for ins in instructions {
        let from_val = match ins {
            Instruction::Copy(c) => get_from_flag_val(c),
            Instruction::Add(a) => get_from_flag_val(a),
            _ => continue,
        };

        if let Some(val) = from_val {
            let target_set = match ins {
                Instruction::Copy(_) => &mut copy_from_stages,
                Instruction::Add(_) => &mut add_from_stages,
                _ => unreachable!(),
            };
            target_set.insert(val.to_lowercase());
        }
    }

    (copy_from_stages, add_from_stages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    #[test]
    fn test_multistage() {
        let dockerfile = r#"
FROM docker.abc.com/base-images/python:3.13-debian@sha256:55f1d15ef4c37870e23c03e89ad238940b55c8ede9f13fac4b7d71c7955f1053 AS base

LABEL org.opencontainers.image.title="My App" \
      org.opencontainers.image.version="1.0" \
      org.opencontainers.image.authors="john@example.com"

ENV PYTHONPATH=/src \
    PYTHONUNBUFFERED=1 \
    REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt \
    PATH="/home/appuser/.local/bin:\$PATH"
WORKDIR /src
USER root:root

RUN apt-get update && \
    apt-get install --no-install-recommends -y postgresql-client curl git && \
    apt-get autoremove -y && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir --upgrade pip
COPY --chown=1000:1000 requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

FROM base AS test
COPY --chown=1000:1000 test-requirements.txt ./
USER 1000:1000
RUN pip install --user --no-cache-dir -r test-requirements.txt
COPY ./app ./app
COPY ./test ./test

FROM base
COPY --chown=1000:1000 ./app ./app
USER 1000:1000
ARG GIT_COMMIT
ENV GIT_COMMIT=\$GIT_COMMIT
EXPOSE 5000

CMD ["uvicorn", "--host", "0.0.0.0", "--port", "5000", "app.main:app"]"#;
        let msa = models::MultistageAnalysis {
            is_multistage: true,
            stages_used_as_base_images: vec!["base".to_string()],
            stages_copied_from: vec![],
            stages_added_from: vec![],
            unused_stages: vec!["test".to_string()],
        };
        let images: Vec<models::Image> = vec![models::Image {
            full: "base".to_string(),
            components: Some(models::ImageComponents {
                registry: None,
                name: "base".to_string(),
                tag: None,
                digest: None,
            }),
        }, models::Image {
        full: "docker.abc.com/base-images/python:3.13-debian@sha256:55f1d15ef4c37870e23c03e89ad238940b55c8ede9f13fac4b7d71c7955f1053".to_string(),
        components: Some(models::ImageComponents {
            registry: Some("docker.abc.com".to_string()),
            name: "base-images/python".to_string(),
            tag: Some("3.13-debian".to_string()),
            digest: Some("sha256:55f1d15ef4c37870e23c03e89ad238940b55c8ede9f13fac4b7d71c7955f1053".to_string()),
        }),
    }];

        let instructions = models::InstructionStats {
            total_count: 22,
            by_type: HashMap::from([
                ("ARG".to_string(), 1),
                ("CMD".to_string(), 1),
                ("COPY".to_string(), 5),
                ("ENV".to_string(), 2),
                ("EXPOSE".to_string(), 1),
                ("FROM".to_string(), 3),
                ("LABEL".to_string(), 1),
                ("RUN".to_string(), 4),
                ("USER".to_string(), 3),
                ("WORKDIR".to_string(), 1),
            ]),
        };

        let env_vars = HashMap::from([
            ("PYTHONPATH".into(), "/src".into()),
            ("PYTHONUNBUFFERED".into(), "1".into()),
            (
                "REQUESTS_CA_BUNDLE".into(),
                "/etc/ssl/certs/ca-certificates.crt".into(),
            ),
            ("PATH".into(), "/home/appuser/.local/bin:$PATH".into()),
            ("GIT_COMMIT".into(), "$GIT_COMMIT".into()),
        ]);

        let args = HashMap::from([("GIT_COMMIT".into(), None)]);
        let labels = HashMap::from([
            ("org.opencontainers.image.title".into(), "My App".into()),
            ("org.opencontainers.image.version".into(), "1.0".into()),
            (
                "org.opencontainers.image.authors".into(),
                "john@example.com".into(),
            ),
        ]);

        let expected = models::Analysis {
            num_stages: 3,
            stage_names: vec!["base".to_string(), "test".to_string()],
            images,
            copy_from_stages: vec![],
            add_from_stages: vec![],
            multistage_analysis: msa,
            exposed_ports: vec!["5000".to_string()],
            instructions,
            args,
            labels,
            env_vars,
        };

        let res = analyze_dockerfile(dockerfile);
        assert!(res.is_ok());
        let analysis = res.unwrap();
        assert_eq!(analysis, expected);
    }

    #[test]
    fn test_invalid_dockerfile() {
        let res = analyze_dockerfile("invalid dockerfile content");
        assert!(res.is_err());
        let err_text = res.unwrap_err().to_string();
        assert!(err_text.contains("unknown instruction 'invalid'"));
    }
    #[test]
    fn test_single_stage() {
        let dockerfile = r#"
FROM node:20-alpine

# Set working directory
WORKDIR /app

# Copy package files
COPY package*.json ./

# Install dependencies
RUN npm install

# Copy application source code
COPY . .

# Create non-root user
RUN addgroup -g 1001 -S nodejs && \
    adduser -S nextjs -u 1001

# Change ownership of the app directory
RUN chown -R nextjs:nodejs /app

# Switch to non-root user
USER nextjs

# Expose port
EXPOSE 3000

# Set environment variable
ENV NODE_ENV=production

# Start the application
CMD ["npm", "start"]
"#;
        let msa = models::MultistageAnalysis {
            is_multistage: false,
            stages_used_as_base_images: vec![],
            stages_copied_from: vec![],
            stages_added_from: vec![],
            unused_stages: vec![],
        };
        let images: Vec<models::Image> = vec![models::Image {
            full: "node:20-alpine".to_string(),
            components: Some(models::ImageComponents {
                registry: None,
                name: "node".to_string(),
                tag: Some("20-alpine".to_string()),
                digest: None,
            }),
        }];

        let instructions = models::InstructionStats {
            total_count: 11,
            by_type: HashMap::from([
                ("CMD".to_string(), 1),
                ("COPY".to_string(), 2),
                ("ENV".to_string(), 1),
                ("EXPOSE".to_string(), 1),
                ("FROM".to_string(), 1),
                ("RUN".to_string(), 3),
                ("USER".to_string(), 1),
                ("WORKDIR".to_string(), 1),
            ]),
        };

        let env_vars = HashMap::from([("NODE_ENV".into(), "production".into())]);

        let expected = models::Analysis {
            num_stages: 1,
            stage_names: vec![],
            images,
            copy_from_stages: vec![],
            add_from_stages: vec![],
            multistage_analysis: msa,
            exposed_ports: vec!["3000".to_string()],
            instructions,
            args: HashMap::new(),
            labels: HashMap::new(),
            env_vars,
        };
        let res = analyze_dockerfile(dockerfile);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), expected);
    }

    #[test]
    fn test_multistage_with_copy_and_add() {
        let dockerfile = r#"
# Stage 1: Build dependencies and tools
FROM node:20-alpine AS dependencies
WORKDIR /app
COPY package*.json ./
RUN npm ci --only=production && \
    npm cache clean --force

# Stage 2: Build the application
FROM node:20-alpine AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY src/ ./src/
COPY public/ ./public/
COPY tsconfig.json ./
RUN npm run build

# Stage 3: Create configuration and assets
FROM alpine:3.18 AS config-builder
WORKDIR /configs
RUN echo "server.port=8080" > app.properties && \
    echo "database.host=localhost" >> app.properties && \
    echo "Generated config" > app.conf && \
    mkdir -p assets && \
    echo "Asset file content" > assets/data.txt

# Stage 4: Final production image
FROM node:20-alpine AS production
WORKDIR /app

# Create non-root user
RUN addgroup -g 1001 -S nodejs && \
    adduser -S nextjs -u 1001

# Copy production dependencies from stage 1 using COPY --from
COPY --from=dependencies /app/node_modules ./node_modules

# Copy built application from stage 2 using COPY --from
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/public ./public

# Copy configuration files from stage 3 using ADD --from
# Note: ADD --from can be used similarly to COPY --from
ADD --from=config-builder /configs/app.properties ./config/
ADD --from=config-builder /configs/app.conf ./config/
ADD --from=config-builder /configs/assets ./assets/

# Copy application files
COPY package*.json ./
COPY server.js ./

# Set ownership
RUN chown -R nextjs:nodejs /app
USER nextjs

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Start the application
CMD ["node", "server.js"]
"#;

        let msa = models::MultistageAnalysis {
            is_multistage: true,
            stages_used_as_base_images: vec![],
            stages_copied_from: vec!["builder".to_string(), "dependencies".to_string()],
            stages_added_from: vec!["config-builder".to_string()],
            unused_stages: vec!["production".to_string()],
        };
        let images: Vec<models::Image> = vec![
            models::Image {
                full: "alpine:3.18".to_string(),
                components: Some(models::ImageComponents {
                    registry: None,
                    name: "alpine".to_string(),
                    tag: Some("3.18".to_string()),
                    digest: None,
                }),
            },
            models::Image {
                full: "node:20-alpine".to_string(),
                components: Some(models::ImageComponents {
                    registry: None,
                    name: "node".to_string(),
                    tag: Some("20-alpine".to_string()),
                    digest: None,
                }),
            },
        ];
        let instructions = models::InstructionStats {
            total_count: 31,
            by_type: HashMap::from([
                ("ADD".to_string(), 3),
                ("CMD".to_string(), 1),
                ("COPY".to_string(), 10),
                ("EXPOSE".to_string(), 1),
                ("FROM".to_string(), 4),
                ("HEALTHCHECK".to_string(), 1),
                ("RUN".to_string(), 6),
                ("USER".to_string(), 1),
                ("WORKDIR".to_string(), 4),
            ]),
        };

        let expected = models::Analysis {
            num_stages: 4,
            stage_names: vec![
                "builder".to_string(),
                "config-builder".to_string(),
                "dependencies".to_string(),
                "production".to_string(),
            ],
            images,
            copy_from_stages: vec!["builder".to_string(), "dependencies".to_string()],
            add_from_stages: vec!["config-builder".to_string()],
            multistage_analysis: msa,
            exposed_ports: vec!["8080".to_string()],
            instructions,
            args: HashMap::new(),
            labels: HashMap::new(),
            env_vars: HashMap::new(),
        };
        let res = analyze_dockerfile(dockerfile);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), expected);
    }

    #[test]
    fn test_multistage_with_copy_and_add2() {
        let dockerfile = r#"
# Stage 1: Download and prepare external dependencies
FROM alpine:3.18 AS downloader
RUN apk add --no-cache curl tar
WORKDIR /downloads
RUN curl -L https://github.com/some-project/releases/download/v1.0.0/binary.tar.gz -o binary.tar.gz && \
    tar -xzf binary.tar.gz

# Stage 2: Compile application
FROM golang:1.21-alpine AS go-builder
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 GOOS=linux go build -o app .

# Stage 3: Generate certificates and configs
FROM alpine:3.18 AS cert-generator
RUN apk add --no-cache openssl
WORKDIR /certs
RUN openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
    -subj "/C=US/ST=State/L=City/O=Organization/CN=localhost"
RUN echo "tls_cert=/app/certs/cert.pem" > server.conf && \
    echo "tls_key=/app/certs/key.pem" >> server.conf

# Stage 4: Final runtime image
FROM alpine:3.18
RUN apk add --no-cache ca-certificates tzdata
WORKDIR /app

# Copy binary from Go builder stage
COPY --from=go-builder /src/app ./

# Copy external dependencies using ADD --from
ADD --from=downloader /downloads/binary ./bin/
ADD --from=downloader /downloads/config/ ./external-config/

# Copy certificates and config using COPY --from
COPY --from=cert-generator /certs/*.pem ./certs/
COPY --from=cert-generator /certs/server.conf ./config/

# Create necessary directories and set permissions
RUN mkdir -p /app/logs && \
    adduser -D -s /bin/sh appuser && \
    chown -R appuser:appuser /app

USER appuser
EXPOSE 8080 8443

CMD ["./app"]
"#;

        let msa = models::MultistageAnalysis {
            is_multistage: true,
            stages_used_as_base_images: vec![],
            stages_copied_from: vec!["cert-generator".to_string(), "go-builder".to_string()],
            stages_added_from: vec!["downloader".to_string()],
            unused_stages: vec![],
        };
        let images: Vec<models::Image> = vec![
            models::Image {
                full: "alpine:3.18".to_string(),
                components: Some(models::ImageComponents {
                    registry: None,
                    name: "alpine".to_string(),
                    tag: Some("3.18".to_string()),
                    digest: None,
                }),
            },
            models::Image {
                full: "golang:1.21-alpine".to_string(),
                components: Some(models::ImageComponents {
                    registry: None,
                    name: "golang".to_string(),
                    tag: Some("1.21-alpine".to_string()),
                    digest: None,
                }),
            },
        ];
        let instructions = models::InstructionStats {
            total_count: 27,
            by_type: HashMap::from([
                ("ADD".to_string(), 2),
                ("CMD".to_string(), 1),
                ("COPY".to_string(), 5),
                ("EXPOSE".to_string(), 1),
                ("FROM".to_string(), 4),
                ("RUN".to_string(), 9),
                ("USER".to_string(), 1),
                ("WORKDIR".to_string(), 4),
            ]),
        };

        let expected = models::Analysis {
            num_stages: 4,
            stage_names: vec![
                "cert-generator".to_string(),
                "downloader".to_string(),
                "go-builder".to_string(),
            ],
            images,
            copy_from_stages: vec!["cert-generator".to_string(), "go-builder".to_string()],
            add_from_stages: vec!["downloader".to_string()],
            multistage_analysis: msa,
            exposed_ports: vec!["8080".to_string(), "8443".to_string()],
            instructions,
            args: HashMap::new(),
            labels: HashMap::new(),
            env_vars: HashMap::new(),
        };
        let res = analyze_dockerfile(dockerfile);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), expected);
    }

    #[cfg(test)]
    mod additional_tests {
        use super::*;
        use std::vec;

        #[test]
        fn test_empty_dockerfile() {
            let dockerfile = "";
            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_err());
        }

        #[test]
        fn test_dockerfile_with_only_comments() {
            let dockerfile = r#"
# This is a comment
# Another comment
        "#;
            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_err());
        }

        #[test]
        fn test_case_insensitive_instructions() {
            let dockerfile = r#"
from node:18-alpine as builder
workdir /app
copy package*.json ./
run npm install
copy . .
run npm run build

from nginx:alpine
copy --from=builder /app/dist /usr/share/nginx/html
expose 80
cmd ["nginx", "-g", "daemon off;"]
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: true,
                stages_used_as_base_images: vec![],
                stages_copied_from: vec!["builder".to_string()],
                stages_added_from: vec![],
                unused_stages: vec![],
            };
            let images: Vec<models::Image> = vec![
                models::Image {
                    full: "nginx:alpine".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "nginx".to_string(),
                        tag: Some("alpine".to_string()),
                        digest: None,
                    }),
                },
                models::Image {
                    full: "node:18-alpine".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "node".to_string(),
                        tag: Some("18-alpine".to_string()),
                        digest: None,
                    }),
                },
            ];

            let instructions = models::InstructionStats {
                total_count: 10,
                by_type: HashMap::from([
                    ("CMD".to_string(), 1),
                    ("COPY".to_string(), 3),
                    ("EXPOSE".to_string(), 1),
                    ("FROM".to_string(), 2),
                    ("RUN".to_string(), 2),
                    ("WORKDIR".to_string(), 1),
                ]),
            };

            let expected = models::Analysis {
                num_stages: 2,
                stage_names: vec!["builder".to_string()],
                images,
                copy_from_stages: vec!["builder".to_string()],
                add_from_stages: vec![],
                multistage_analysis: msa,
                exposed_ports: vec!["80".to_string()],
                instructions,
                args: HashMap::new(),
                labels: HashMap::new(),
                env_vars: HashMap::new(),
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }

        #[test]
        fn test_multistage_with_stage_used_as_base_and_copy_source() {
            let dockerfile = r#"
FROM ubuntu:20.04 AS base
RUN apt-get update && apt-get install -y curl
WORKDIR /app

FROM base AS builder
COPY . .
RUN make build

FROM base
COPY --from=builder /app/dist ./
CMD ["./app"]
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: true,
                stages_used_as_base_images: vec!["base".to_string()],
                stages_copied_from: vec!["builder".to_string()],
                stages_added_from: vec![],
                unused_stages: vec![],
            };
            let images: Vec<models::Image> = vec![
                models::Image {
                    full: "base".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "base".to_string(),
                        tag: None,
                        digest: None,
                    }),
                },
                models::Image {
                    full: "ubuntu:20.04".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "ubuntu".to_string(),
                        tag: Some("20.04".to_string()),
                        digest: None,
                    }),
                },
            ];
            let instructions = models::InstructionStats {
                total_count: 9,
                by_type: HashMap::from([
                    ("CMD".to_string(), 1),
                    ("COPY".to_string(), 2),
                    ("FROM".to_string(), 3),
                    ("RUN".to_string(), 2),
                    ("WORKDIR".to_string(), 1),
                ]),
            };

            let env_vars = HashMap::new();

            let expected = models::Analysis {
                num_stages: 3,
                stage_names: vec!["base".to_string(), "builder".to_string()],
                images,
                copy_from_stages: vec!["builder".to_string()],
                add_from_stages: vec![],
                multistage_analysis: msa,
                exposed_ports: vec![],
                instructions,
                args: HashMap::new(),
                labels: HashMap::new(),
                env_vars,
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }

        #[test]
        fn test_dockerfile_with_arg_in_from() {
            let dockerfile = r#"
ARG BASE_IMAGE=node:18-alpine
FROM $BASE_IMAGE AS builder
WORKDIR /app
COPY . .
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: true,
                stages_used_as_base_images: vec![],
                stages_copied_from: vec!["builder".to_string()],
                stages_added_from: vec![],
                unused_stages: vec![],
            };
            let images: Vec<models::Image> = vec![
                models::Image {
                    full: r"$BASE_IMAGE".to_string(),
                    components: None,
                },
                models::Image {
                    full: "nginx:alpine".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "nginx".to_string(),
                        tag: Some("alpine".to_string()),
                        digest: None,
                    }),
                },
            ];
            let instructions = models::InstructionStats {
                total_count: 7,
                by_type: HashMap::from([
                    ("ARG".to_string(), 1),
                    ("COPY".to_string(), 2),
                    ("FROM".to_string(), 2),
                    ("RUN".to_string(), 1),
                    ("WORKDIR".to_string(), 1),
                ]),
            };
            let args = HashMap::from([("BASE_IMAGE".into(), Some("node:18-alpine".into()))]);

            let expected = models::Analysis {
                num_stages: 2,
                stage_names: vec!["builder".to_string()],
                images,
                copy_from_stages: vec!["builder".to_string()],
                add_from_stages: vec![],
                multistage_analysis: msa,
                exposed_ports: vec![],
                instructions,
                args,
                labels: HashMap::new(),
                env_vars: HashMap::new(),
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }

        #[test]
        fn test_multistage_with_duplicate_stage_names() {
            let dockerfile = r#"
FROM ubuntu:20.04 AS base
RUN apt-get update

FROM alpine:3.18 AS base
RUN apk add --no-cache curl

FROM scratch
COPY --from=base /usr/bin/curl /usr/bin/curl
"#;

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_err());
            let err_text = res.unwrap_err().to_string();
            assert!(err_text.contains("duplicate name 'base'"));
        }

        #[test]
        fn test_multistage_with_self_referencing_stage() {
            let dockerfile = r#"
FROM ubuntu:20.04 AS base
RUN apt-get update

FROM base AS builder
COPY . .
RUN make build
# This would be invalid in practice, but testing parser behavior
COPY --from=builder /app/temp ./temp
RUN process_temp

FROM base
COPY --from=builder /app/dist ./
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: true,
                stages_used_as_base_images: vec!["base".to_string()],
                stages_copied_from: vec!["builder".to_string()],
                stages_added_from: vec![],
                unused_stages: vec![],
            };
            let images: Vec<models::Image> = vec![
                models::Image {
                    full: "base".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "base".to_string(),
                        tag: None,
                        digest: None,
                    }),
                },
                models::Image {
                    full: "ubuntu:20.04".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "ubuntu".to_string(),
                        tag: Some("20.04".to_string()),
                        digest: None,
                    }),
                },
            ];
            let instructions = models::InstructionStats {
                total_count: 9,
                by_type: HashMap::from([
                    ("COPY".to_string(), 3),
                    ("FROM".to_string(), 3),
                    ("RUN".to_string(), 3),
                ]),
            };

            let expected = models::Analysis {
                num_stages: 3,
                stage_names: vec!["base".to_string(), "builder".to_string()],
                images,
                copy_from_stages: vec!["builder".to_string()],
                add_from_stages: vec![],
                multistage_analysis: msa,
                exposed_ports: vec![],
                instructions,
                args: HashMap::new(),
                labels: HashMap::new(),
                env_vars: HashMap::new(),
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }

        #[test]
        fn test_multistage_with_only_add_from() {
            let dockerfile = r#"
FROM alpine:3.18 AS assets
WORKDIR /assets
RUN echo "config data" > config.json

FROM ubuntu:20.04
ADD --from=assets /assets/ ./assets/
RUN cat assets/config.json
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: true,
                stages_used_as_base_images: vec![],
                stages_copied_from: vec![],
                stages_added_from: vec!["assets".to_string()],
                unused_stages: vec![],
            };
            let images: Vec<models::Image> = vec![
                models::Image {
                    full: "alpine:3.18".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "alpine".to_string(),
                        tag: Some("3.18".to_string()),
                        digest: None,
                    }),
                },
                models::Image {
                    full: "ubuntu:20.04".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "ubuntu".to_string(),
                        tag: Some("20.04".to_string()),
                        digest: None,
                    }),
                },
            ];

            let instructions = models::InstructionStats {
                total_count: 6,
                by_type: HashMap::from([
                    ("ADD".to_string(), 1),
                    ("FROM".to_string(), 2),
                    ("RUN".to_string(), 2),
                    ("WORKDIR".to_string(), 1),
                ]),
            };

            let expected = models::Analysis {
                num_stages: 2,
                stage_names: vec!["assets".to_string()],
                images,
                copy_from_stages: vec![],
                add_from_stages: vec!["assets".to_string()],
                multistage_analysis: msa,
                exposed_ports: vec![],
                instructions,
                args: HashMap::new(),
                labels: HashMap::new(),
                env_vars: HashMap::new(),
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }

        #[test]
        fn test_dockerfile_with_whitespace_and_comments() {
            let dockerfile = r#"
# Build stage
FROM node:18-alpine AS builder  
WORKDIR /app
# Install dependencies
COPY package*.json ./
RUN npm ci

# Production stage  
FROM node:18-alpine
WORKDIR /app
COPY --from=builder /app/node_modules ./node_modules
# Copy source code
COPY . .
CMD ["npm", "start"]
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: true,
                stages_used_as_base_images: vec![],
                stages_copied_from: vec!["builder".to_string()],
                stages_added_from: vec![],
                unused_stages: vec![],
            };
            let images: Vec<models::Image> = vec![models::Image {
                full: "node:18-alpine".to_string(),
                components: Some(models::ImageComponents {
                    registry: None,
                    name: "node".to_string(),
                    tag: Some("18-alpine".to_string()),
                    digest: None,
                }),
            }];

            let instructions = models::InstructionStats {
                total_count: 9,
                by_type: HashMap::from([
                    ("CMD".to_string(), 1),
                    ("COPY".to_string(), 3),
                    ("FROM".to_string(), 2),
                    ("RUN".to_string(), 1),
                    ("WORKDIR".to_string(), 2),
                ]),
            };

            let expected = models::Analysis {
                num_stages: 2,
                stage_names: vec!["builder".to_string()],
                images,
                copy_from_stages: vec!["builder".to_string()],
                add_from_stages: vec![],
                multistage_analysis: msa,
                exposed_ports: vec![],
                instructions,
                args: HashMap::new(),
                labels: HashMap::new(),
                env_vars: HashMap::new(),
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }

        #[test]
        fn test_multistage_with_unreferenced_stages() {
            let dockerfile = r#"
FROM ubuntu:20.04 AS unused-stage
RUN apt-get update

FROM alpine:3.18 AS another-unused
RUN apk add --no-cache curl

FROM node:18-alpine AS builder
WORKDIR /app
COPY . .
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: true,
                stages_used_as_base_images: vec![],
                stages_copied_from: vec!["builder".to_string()],
                stages_added_from: vec![],
                unused_stages: vec!["another-unused".to_string(), "unused-stage".to_string()],
            };
            let images: Vec<models::Image> = vec![
                models::Image {
                    full: "alpine:3.18".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "alpine".to_string(),
                        tag: Some("3.18".to_string()),
                        digest: None,
                    }),
                },
                models::Image {
                    full: "nginx:alpine".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "nginx".to_string(),
                        tag: Some("alpine".to_string()),
                        digest: None,
                    }),
                },
                models::Image {
                    full: "node:18-alpine".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "node".to_string(),
                        tag: Some("18-alpine".to_string()),
                        digest: None,
                    }),
                },
                models::Image {
                    full: "ubuntu:20.04".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "ubuntu".to_string(),
                        tag: Some("20.04".to_string()),
                        digest: None,
                    }),
                },
            ];
            let instructions = models::InstructionStats {
                total_count: 10,
                by_type: HashMap::from([
                    ("COPY".to_string(), 2),
                    ("FROM".to_string(), 4),
                    ("RUN".to_string(), 3),
                    ("WORKDIR".to_string(), 1),
                ]),
            };

            let expected = models::Analysis {
                num_stages: 4,
                stage_names: vec![
                    "another-unused".to_string(),
                    "builder".to_string(),
                    "unused-stage".to_string(),
                ],
                images,
                copy_from_stages: vec!["builder".to_string()],
                add_from_stages: vec![],
                multistage_analysis: msa,
                exposed_ports: vec![],
                instructions,
                args: HashMap::new(),
                labels: HashMap::new(),
                env_vars: HashMap::new(),
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }

        #[test]
        fn test_dockerfile_with_platform_in_from() {
            let dockerfile = r#"
FROM --platform=linux/amd64 node:18-alpine AS builder
WORKDIR /app
COPY . .
RUN npm run build

FROM --platform=linux/amd64 nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: true,
                stages_used_as_base_images: vec![],
                stages_copied_from: vec!["builder".to_string()],
                stages_added_from: vec![],
                unused_stages: vec![],
            };
            let images: Vec<models::Image> = vec![
                models::Image {
                    full: "nginx:alpine".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "nginx".to_string(),
                        tag: Some("alpine".to_string()),
                        digest: None,
                    }),
                },
                models::Image {
                    full: "node:18-alpine".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "node".to_string(),
                        tag: Some("18-alpine".to_string()),
                        digest: None,
                    }),
                },
            ];
            let instructions = models::InstructionStats {
                total_count: 6,
                by_type: HashMap::from([
                    ("COPY".to_string(), 2),
                    ("FROM".to_string(), 2),
                    ("RUN".to_string(), 1),
                    ("WORKDIR".to_string(), 1),
                ]),
            };

            let expected = models::Analysis {
                num_stages: 2,
                stage_names: vec!["builder".to_string()],
                images,
                copy_from_stages: vec!["builder".to_string()],
                add_from_stages: vec![],
                multistage_analysis: msa,
                exposed_ports: vec![],
                instructions,
                args: HashMap::new(),
                labels: HashMap::new(),
                env_vars: HashMap::new(),
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }

        #[test]
        fn test_single_stage_with_scratch_image() {
            let dockerfile = r#"
FROM scratch
COPY binary /
CMD ["/binary"]
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: false,
                stages_used_as_base_images: vec![],
                stages_copied_from: vec![],
                stages_added_from: vec![],
                unused_stages: vec![],
            };
            let images: Vec<models::Image> = vec![models::Image {
                full: "scratch".to_string(),
                components: Some(models::ImageComponents {
                    registry: None,
                    name: "scratch".to_string(),
                    tag: None,
                    digest: None,
                }),
            }];
            let instructions = models::InstructionStats {
                total_count: 3,
                by_type: HashMap::from([
                    ("CMD".to_string(), 1),
                    ("COPY".to_string(), 1),
                    ("FROM".to_string(), 1),
                ]),
            };

            let expected = models::Analysis {
                num_stages: 1,
                stage_names: vec![],
                images,
                copy_from_stages: vec![],
                add_from_stages: vec![],
                multistage_analysis: msa,
                exposed_ports: vec![],
                instructions,
                args: HashMap::new(),
                labels: HashMap::new(),
                env_vars: HashMap::new(),
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }

        #[test]
        fn test_multistage_complex_dependency_chain() {
            let dockerfile = r#"
FROM alpine:3.18 AS source
RUN echo "source data" > /data.txt

FROM ubuntu:20.04 AS processor
COPY --from=source /data.txt ./
RUN cat data.txt > processed.txt

FROM node:18-alpine AS builder
ADD --from=processor /processed.txt ./
COPY . .
RUN npm run build

FROM nginx:alpine
COPY --from=builder /app/dist /usr/share/nginx/html
ADD --from=source /data.txt /usr/share/nginx/html/
"#;

            let msa = models::MultistageAnalysis {
                is_multistage: true,
                stages_used_as_base_images: vec![],
                stages_copied_from: vec!["builder".to_string(), "source".to_string()],
                stages_added_from: vec!["processor".to_string(), "source".to_string()],
                unused_stages: vec![],
            };
            let images: Vec<models::Image> = vec![
                models::Image {
                    full: "alpine:3.18".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "alpine".to_string(),
                        tag: Some("3.18".to_string()),
                        digest: None,
                    }),
                },
                models::Image {
                    full: "nginx:alpine".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "nginx".to_string(),
                        tag: Some("alpine".to_string()),
                        digest: None,
                    }),
                },
                models::Image {
                    full: "node:18-alpine".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "node".to_string(),
                        tag: Some("18-alpine".to_string()),
                        digest: None,
                    }),
                },
                models::Image {
                    full: "ubuntu:20.04".to_string(),
                    components: Some(models::ImageComponents {
                        registry: None,
                        name: "ubuntu".to_string(),
                        tag: Some("20.04".to_string()),
                        digest: None,
                    }),
                },
            ];
            let instructions = models::InstructionStats {
                total_count: 12,
                by_type: HashMap::from([
                    ("ADD".to_string(), 2),
                    ("COPY".to_string(), 3),
                    ("FROM".to_string(), 4),
                    ("RUN".to_string(), 3),
                ]),
            };

            let expected = models::Analysis {
                num_stages: 4,
                stage_names: vec![
                    "builder".to_string(),
                    "processor".to_string(),
                    "source".to_string(),
                ],
                images,
                copy_from_stages: vec!["builder".to_string(), "source".to_string()],
                add_from_stages: vec!["processor".to_string(), "source".to_string()],
                multistage_analysis: msa,
                exposed_ports: vec![],
                instructions,
                args: HashMap::new(),
                labels: HashMap::new(),
                env_vars: HashMap::new(),
            };

            let res = analyze_dockerfile(dockerfile);
            assert!(res.is_ok());
            assert_eq!(res.unwrap(), expected);
        }
    }
}
