use crate::constants;
use crate::models;
use crate::parse_utils;
use docker_image::DockerImage;
use parse_dockerfile::{Flag, Instruction, Stage, parse};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

struct StageInfo {
    images: BTreeSet<String>,
    stage_names: BTreeSet<String>,
    /// `AS` name of each stage, in Dockerfile order. `None` when the stage is unnamed.
    ordered_names: Vec<Option<String>>,
}

struct InstructionFacts {
    copy_from_stages: BTreeSet<String>,
    add_from_stages: BTreeSet<String>,
    copy_from_images: BTreeMap<String, models::Image>,
    add_from_images: BTreeMap<String, models::Image>,
    exposed_ports: BTreeSet<String>,
    by_type: BTreeMap<String, u32>,
    total_count: u32,
    args: BTreeMap<String, Option<String>>,
    labels: BTreeMap<String, String>,
    env_vars: BTreeMap<String, String>,
}

enum FromTarget {
    Stage(String),
    Image(String),
}

pub fn analyze_dockerfile(body: &str) -> Result<models::Analysis, Box<dyn Error>> {
    let dockerfile = parse(body)?;
    let stages: Vec<_> = dockerfile.stages().collect();
    let stage_info = extract_stage_info(&stages);
    let facts = collect_instructions(&dockerfile.instructions, &stage_info.ordered_names);
    let multistage_analysis = analyze_multistage(
        stages.len(),
        &stage_info.images,
        &stage_info.stage_names,
        &facts.copy_from_stages,
        &facts.add_from_stages,
    );

    Ok(models::Analysis {
        num_stages: stages.len(),
        images: parse_images(&stage_info.images),
        stage_names: stage_info.stage_names.into_iter().collect(),
        final_stage: stage_info.ordered_names.last().cloned().flatten(),
        copy_from_stages: facts.copy_from_stages.into_iter().collect(),
        add_from_stages: facts.add_from_stages.into_iter().collect(),
        copy_from_images: facts.copy_from_images.into_values().collect(),
        add_from_images: facts.add_from_images.into_values().collect(),
        multistage_analysis,
        exposed_ports: facts.exposed_ports.into_iter().collect(),
        instructions: models::InstructionStats {
            total_count: facts.total_count,
            by_type: facts.by_type,
        },
        args: facts.args,
        labels: facts.labels,
        env_vars: facts.env_vars,
    })
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

    let used_names: BTreeSet<String> = stages_used_as_base_images
        .iter()
        .chain(copy_from_stages.iter())
        .chain(add_from_stages.iter())
        .filter(|name| stage_names.contains(*name))
        .cloned()
        .collect();
    let unused_stages = stage_names.difference(&used_names).cloned();

    let referenced_stage = !stages_used_as_base_images.is_empty()
        || !copy_from_stages.is_empty()
        || !add_from_stages.is_empty();

    models::MultistageAnalysis {
        is_multistage: num_stages >= 2 && referenced_stage,
        stages_used_as_base_images: stages_used_as_base_images.into_iter().collect(),
        stages_copied_from: copy_from_stages.iter().cloned().collect(),
        stages_added_from: add_from_stages.iter().cloned().collect(),
        unused_stages: unused_stages.collect(),
    }
}

fn extract_stage_info(stages: &[Stage]) -> StageInfo {
    let mut images = BTreeSet::new();
    let mut stage_names = BTreeSet::new();
    let mut ordered_names = Vec::with_capacity(stages.len());

    for stage in stages {
        images.insert(normalize_image_ref(&stage.from.image.value));
        let name = stage
            .from
            .as_
            .as_ref()
            .map(|(_, stage_name)| stage_name.value.to_lowercase());
        if let Some(name) = &name {
            stage_names.insert(name.clone());
        }
        ordered_names.push(name);
    }

    StageInfo {
        images,
        stage_names,
        ordered_names,
    }
}

fn collect_instructions(
    instructions: &[Instruction],
    ordered_stage_names: &[Option<String>],
) -> InstructionFacts {
    let mut facts = InstructionFacts {
        copy_from_stages: BTreeSet::new(),
        add_from_stages: BTreeSet::new(),
        copy_from_images: BTreeMap::new(),
        add_from_images: BTreeMap::new(),
        exposed_ports: BTreeSet::new(),
        by_type: BTreeMap::new(),
        total_count: 0,
        args: BTreeMap::new(),
        labels: BTreeMap::new(),
        env_vars: BTreeMap::new(),
    };

    for instruction in instructions {
        facts.total_count += 1;
        let name = instruction_name(instruction);
        *facts.by_type.entry(name.to_string()).or_insert(0) += 1;

        match instruction {
            Instruction::Arg(arg) => {
                parse_utils::merge_kv_pairs(
                    &mut facts.args,
                    parse_utils::parse_kv_pairs(arg.arguments.value.as_ref(), false),
                );
            }
            Instruction::Env(env) => {
                insert_required_values(
                    &mut facts.env_vars,
                    parse_utils::parse_kv_pairs(env.arguments.value.as_ref(), true),
                );
            }
            Instruction::Label(label) => {
                insert_required_values(
                    &mut facts.labels,
                    parse_utils::parse_kv_pairs(label.arguments.value.as_ref(), true),
                );
            }
            Instruction::Expose(expose) => {
                facts
                    .exposed_ports
                    .extend(expose.arguments.iter().map(|port| port.value.to_string()));
            }
            Instruction::Copy(copy) => {
                record_from_flag(
                    &copy.options,
                    ordered_stage_names,
                    &mut facts.copy_from_stages,
                    &mut facts.copy_from_images,
                );
            }
            Instruction::Add(add) => {
                record_from_flag(
                    &add.options,
                    ordered_stage_names,
                    &mut facts.add_from_stages,
                    &mut facts.add_from_images,
                );
            }
            _ => {}
        }
    }

    facts
}

fn insert_required_values(
    into: &mut BTreeMap<String, String>,
    pairs: BTreeMap<String, Option<String>>,
) {
    for (key, value) in pairs {
        into.insert(key, value.unwrap_or_default());
    }
}

fn record_from_flag(
    options: &[Flag],
    ordered_stage_names: &[Option<String>],
    stages: &mut BTreeSet<String>,
    images: &mut BTreeMap<String, models::Image>,
) {
    let Some(raw) = from_flag_value(options) else {
        return;
    };
    match resolve_from_target(&raw, ordered_stage_names) {
        FromTarget::Stage(name) => {
            stages.insert(name);
        }
        FromTarget::Image(reference) => {
            images
                .entry(reference.clone())
                .or_insert_with(|| parse_image(&reference));
        }
    }
}

fn from_flag_value(options: &[Flag]) -> Option<String> {
    options.iter().find_map(|flag| {
        flag.name
            .value
            .eq_ignore_ascii_case(constants::FROM)
            .then(|| flag.value.as_ref().map(|value| value.value.to_string()))
            .flatten()
    })
}

/// Map a `--from` value to a stage or an external image.
///
/// An all-digit value is a stage index. Docker uses the index even when a
/// stage happens to be named with that same number, and even when the index
/// is out of range. An in-range index of an unnamed stage is reported as the
/// decimal index. Any other value that is not a known stage name is an image
/// reference. `$VAR` references are not expanded and keep their original case.
fn resolve_from_target(raw: &str, ordered_stage_names: &[Option<String>]) -> FromTarget {
    if raw.contains('$') {
        let folded = raw.to_lowercase();
        if is_known_stage(&folded, ordered_stage_names) {
            return FromTarget::Stage(folded);
        }
        return FromTarget::Image(raw.to_string());
    }

    let normalized = raw.to_lowercase();
    if is_stage_index(&normalized) {
        return FromTarget::Stage(stage_index_label(&normalized, ordered_stage_names));
    }
    if is_known_stage(&normalized, ordered_stage_names) {
        return FromTarget::Stage(normalized);
    }
    FromTarget::Image(normalized)
}

fn is_known_stage(name: &str, ordered_stage_names: &[Option<String>]) -> bool {
    ordered_stage_names
        .iter()
        .any(|stage_name| stage_name.as_deref() == Some(name))
}

fn is_stage_index(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn stage_index_label(index_text: &str, ordered_stage_names: &[Option<String>]) -> String {
    let Ok(index) = index_text.parse::<usize>() else {
        return index_text.to_string();
    };
    match ordered_stage_names.get(index) {
        Some(Some(name)) => name.clone(),
        Some(None) => index.to_string(),
        None => index_text.to_string(),
    }
}

fn instruction_name(instruction: &Instruction) -> &'static str {
    match instruction {
        Instruction::Add(_) => constants::ADD,
        Instruction::Arg(_) => constants::ARG,
        Instruction::Cmd(_) => constants::CMD,
        Instruction::Copy(_) => constants::COPY,
        Instruction::Entrypoint(_) => constants::ENTRYPOINT,
        Instruction::Env(_) => constants::ENV,
        Instruction::Expose(_) => constants::EXPOSE,
        Instruction::From(_) => constants::FROM_UC,
        Instruction::Healthcheck(_) => constants::HEALTHCHECK,
        Instruction::Label(_) => constants::LABEL,
        Instruction::Maintainer(_) => constants::MAINTAINER,
        Instruction::Onbuild(_) => constants::ONBUILD,
        Instruction::Run(_) => constants::RUN,
        Instruction::Shell(_) => constants::SHELL,
        Instruction::Stopsignal(_) => constants::STOPSIGNAL,
        Instruction::User(_) => constants::USER,
        Instruction::Volume(_) => constants::VOLUME,
        Instruction::Workdir(_) => constants::WORKDIR,
        _ => "UNKNOWN",
    }
}

fn normalize_image_ref(value: &str) -> String {
    if value.contains('$') {
        value.to_string()
    } else {
        value.to_lowercase()
    }
}

fn parse_images(images: &BTreeSet<String>) -> Vec<models::Image> {
    images.iter().map(|image| parse_image(image)).collect()
}

fn parse_image(reference: &str) -> models::Image {
    if reference_has_variable(reference) {
        return models::Image {
            full: reference.to_string(),
            components: None,
        };
    }
    let components = DockerImage::parse(reference)
        .ok()
        .map(|parsed| models::ImageComponents {
            registry: parsed.registry,
            name: parsed.name,
            tag: parsed.tag,
            digest: parsed.digest,
        })
        .or_else(|| fallback_components(reference));
    models::Image {
        full: reference.to_string(),
        components,
    }
}

fn reference_has_variable(reference: &str) -> bool {
    reference.starts_with('$') || reference.contains("${")
}

/// Split `registry/name:tag@digest` when `docker-image` rejects the reference.
///
/// The common miss is a short digest. Variable references are left unparsed.
fn fallback_components(reference: &str) -> Option<models::ImageComponents> {
    if reference.is_empty() || reference.contains(char::is_whitespace) {
        return None;
    }
    let (without_digest, digest) = split_digest(reference)?;
    let (name_part, tag) = split_tag(without_digest);
    let (registry, name) = split_registry(name_part)?;
    if name.is_empty() {
        return None;
    }
    Some(models::ImageComponents {
        registry,
        name,
        tag,
        digest,
    })
}

fn split_digest(reference: &str) -> Option<(&str, Option<String>)> {
    match reference.rsplit_once('@') {
        Some((rest, digest)) if is_digest(digest) && !rest.is_empty() => {
            Some((rest, Some(digest.to_string())))
        }
        Some(_) => None,
        None => Some((reference, None)),
    }
}

fn is_digest(digest: &str) -> bool {
    let Some((algorithm, hex_digits)) = digest.split_once(':') else {
        return false;
    };
    !algorithm.is_empty()
        && algorithm
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && !hex_digits.is_empty()
        && hex_digits.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn split_tag(reference: &str) -> (&str, Option<String>) {
    match reference.rfind(':') {
        Some(index) if !reference[index + 1..].contains('/') => {
            let tag = &reference[index + 1..];
            if tag.is_empty() {
                (reference, None)
            } else {
                (&reference[..index], Some(tag.to_string()))
            }
        }
        _ => (reference, None),
    }
}

fn split_registry(name_part: &str) -> Option<(Option<String>, String)> {
    let Some((first, rest)) = name_part.split_once('/') else {
        return Some((None, name_part.to_string()));
    };
    if rest.is_empty() {
        return None;
    }
    if first == "localhost" || first.contains('.') || first.contains(':') {
        Some((Some(first.to_string()), rest.to_string()))
    } else {
        Some((None, name_part.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    fn image(full: &str, name: &str, tag: Option<&str>, digest: Option<&str>) -> models::Image {
        models::Image {
            full: full.to_string(),
            components: Some(models::ImageComponents {
                registry: None,
                name: name.to_string(),
                tag: tag.map(str::to_string),
                digest: digest.map(str::to_string),
            }),
        }
    }

    fn image_with_registry(
        full: &str,
        registry: &str,
        name: &str,
        tag: Option<&str>,
        digest: Option<&str>,
    ) -> models::Image {
        models::Image {
            full: full.to_string(),
            components: Some(models::ImageComponents {
                registry: Some(registry.to_string()),
                name: name.to_string(),
                tag: tag.map(str::to_string),
                digest: digest.map(str::to_string),
            }),
        }
    }

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
        let images: Vec<models::Image> = vec![
            image("base", "base", None, None),
            image_with_registry(
                "docker.abc.com/base-images/python:3.13-debian@sha256:55f1d15ef4c37870e23c03e89ad238940b55c8ede9f13fac4b7d71c7955f1053",
                "docker.abc.com",
                "base-images/python",
                Some("3.13-debian"),
                Some("sha256:55f1d15ef4c37870e23c03e89ad238940b55c8ede9f13fac4b7d71c7955f1053"),
            ),
        ];

        let instructions = models::InstructionStats {
            total_count: 22,
            by_type: BTreeMap::from([
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

        let env_vars = BTreeMap::from([
            ("PYTHONPATH".into(), "/src".into()),
            ("PYTHONUNBUFFERED".into(), "1".into()),
            (
                "REQUESTS_CA_BUNDLE".into(),
                "/etc/ssl/certs/ca-certificates.crt".into(),
            ),
            ("PATH".into(), "/home/appuser/.local/bin:$PATH".into()),
            ("GIT_COMMIT".into(), "$GIT_COMMIT".into()),
        ]);

        let args = BTreeMap::from([("GIT_COMMIT".into(), None)]);
        let labels = BTreeMap::from([
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
            final_stage: None,
            images,
            copy_from_stages: vec![],
            add_from_stages: vec![],
            copy_from_images: vec![],
            add_from_images: vec![],
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
        let images = vec![image("node:20-alpine", "node", Some("20-alpine"), None)];
        let instructions = models::InstructionStats {
            total_count: 11,
            by_type: BTreeMap::from([
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
        let expected = models::Analysis {
            num_stages: 1,
            stage_names: vec![],
            final_stage: None,
            images,
            copy_from_stages: vec![],
            add_from_stages: vec![],
            copy_from_images: vec![],
            add_from_images: vec![],
            multistage_analysis: msa,
            exposed_ports: vec!["3000".to_string()],
            instructions,
            args: BTreeMap::new(),
            labels: BTreeMap::new(),
            env_vars: BTreeMap::from([("NODE_ENV".into(), "production".into())]),
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
        let images = vec![
            image("alpine:3.18", "alpine", Some("3.18"), None),
            image("node:20-alpine", "node", Some("20-alpine"), None),
        ];
        let instructions = models::InstructionStats {
            total_count: 31,
            by_type: BTreeMap::from([
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
            final_stage: Some("production".to_string()),
            images,
            copy_from_stages: vec!["builder".to_string(), "dependencies".to_string()],
            add_from_stages: vec!["config-builder".to_string()],
            copy_from_images: vec![],
            add_from_images: vec![],
            multistage_analysis: msa,
            exposed_ports: vec!["8080".to_string()],
            instructions,
            args: BTreeMap::new(),
            labels: BTreeMap::new(),
            env_vars: BTreeMap::new(),
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
        let images = vec![
            image("alpine:3.18", "alpine", Some("3.18"), None),
            image("golang:1.21-alpine", "golang", Some("1.21-alpine"), None),
        ];
        let instructions = models::InstructionStats {
            total_count: 27,
            by_type: BTreeMap::from([
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
            final_stage: None,
            images,
            copy_from_stages: vec!["cert-generator".to_string(), "go-builder".to_string()],
            add_from_stages: vec!["downloader".to_string()],
            copy_from_images: vec![],
            add_from_images: vec![],
            multistage_analysis: msa,
            exposed_ports: vec!["8080".to_string(), "8443".to_string()],
            instructions,
            args: BTreeMap::new(),
            labels: BTreeMap::new(),
            env_vars: BTreeMap::new(),
        };
        let res = analyze_dockerfile(dockerfile);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), expected);
    }

    #[test]
    fn test_empty_dockerfile() {
        assert!(analyze_dockerfile("").is_err());
    }

    #[test]
    fn test_dockerfile_with_only_comments() {
        let dockerfile = r#"
# This is a comment
# Another comment
        "#;
        assert!(analyze_dockerfile(dockerfile).is_err());
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
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert!(analysis.multistage_analysis.is_multistage);
        assert_eq!(analysis.stage_names, vec!["builder".to_string()]);
        assert_eq!(analysis.final_stage, None);
        assert_eq!(analysis.copy_from_stages, vec!["builder".to_string()]);
        assert_eq!(analysis.exposed_ports, vec!["80".to_string()]);
        assert_eq!(analysis.images.len(), 2);
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
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert!(analysis.multistage_analysis.is_multistage);
        assert_eq!(
            analysis.multistage_analysis.stages_used_as_base_images,
            vec!["base".to_string()]
        );
        assert_eq!(
            analysis.multistage_analysis.stages_copied_from,
            vec!["builder".to_string()]
        );
        assert!(analysis.multistage_analysis.unused_stages.is_empty());
        assert_eq!(analysis.final_stage, None);
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
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert_eq!(
            analysis.args,
            BTreeMap::from([("BASE_IMAGE".into(), Some("node:18-alpine".into()))])
        );
        assert_eq!(analysis.images[0].full, "$BASE_IMAGE");
        assert!(analysis.images[0].components.is_none());
        assert_eq!(analysis.copy_from_stages, vec!["builder".to_string()]);
    }

    #[test]
    fn test_arg_redeclaration_keeps_global_default() {
        let dockerfile = r#"
ARG VERSION=1
FROM alpine:${VERSION}
ARG VERSION
"#;
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert_eq!(
            analysis.args,
            BTreeMap::from([("VERSION".into(), Some("1".into()))])
        );
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
        let err_text = analyze_dockerfile(dockerfile).unwrap_err().to_string();
        assert!(
            err_text.contains("duplicate stage name 'base'"),
            "unexpected error: {err_text}"
        );
    }

    #[test]
    fn test_numeric_from_resolves_named_stage() {
        let dockerfile = r#"
FROM alpine AS base
FROM scratch
COPY --from=0 /etc/os-release /
ADD --from=0 /etc/os-release /from-add
"#;
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert!(analysis.multistage_analysis.is_multistage);
        assert_eq!(analysis.copy_from_stages, vec!["base".to_string()]);
        assert_eq!(analysis.add_from_stages, vec!["base".to_string()]);
        assert!(analysis.multistage_analysis.unused_stages.is_empty());
        assert!(analysis.copy_from_images.is_empty());
    }

    #[test]
    fn test_numeric_from_unnamed_stage_reports_index() {
        let dockerfile = r#"
FROM alpine
FROM scratch
COPY --from=0 /etc/os-release /
"#;
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert!(analysis.multistage_analysis.is_multistage);
        assert_eq!(analysis.copy_from_stages, vec!["0".to_string()]);
        assert_eq!(
            analysis.multistage_analysis.stages_copied_from,
            vec!["0".to_string()]
        );
        assert_eq!(analysis.final_stage, None);
    }

    #[test]
    fn test_external_from_is_an_image() {
        let dockerfile = r#"
FROM alpine
COPY --from=nginx:1.25 /etc/nginx /etc/nginx
ADD --from=ghcr.io/org/app:1.2 /bin/app /bin/app
"#;
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert!(!analysis.multistage_analysis.is_multistage);
        assert!(analysis.copy_from_stages.is_empty());
        assert!(analysis.add_from_stages.is_empty());
        assert_eq!(
            analysis.copy_from_images,
            vec![image("nginx:1.25", "nginx", Some("1.25"), None)]
        );
        assert_eq!(
            analysis.add_from_images,
            vec![image_with_registry(
                "ghcr.io/org/app:1.2",
                "ghcr.io",
                "org/app",
                Some("1.2"),
                None
            )]
        );
    }

    #[test]
    fn test_from_index_beats_stage_named_with_digits() {
        let dockerfile = r#"
FROM alpine AS 0
FROM scratch
COPY --from=0 /x /x
"#;
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert_eq!(analysis.stage_names, vec!["0".to_string()]);
        assert_eq!(analysis.copy_from_stages, vec!["0".to_string()]);
        assert!(analysis.multistage_analysis.is_multistage);
    }

    #[test]
    fn test_short_digest_uses_component_fallback() {
        let dockerfile = "FROM python:3.12-slim@sha256:abcdef\n";
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert_eq!(
            analysis.images,
            vec![image(
                "python:3.12-slim@sha256:abcdef",
                "python",
                Some("3.12-slim"),
                Some("sha256:abcdef")
            )]
        );
    }

    #[test]
    fn test_variable_image_is_not_expanded() {
        let dockerfile = r#"
ARG REG=docker.io
FROM ${REG}/library/ubuntu:${TAG:-22.04} AS build
"#;
        let analysis = analyze_dockerfile(dockerfile).unwrap();
        assert_eq!(
            analysis.images[0].full,
            "${REG}/library/ubuntu:${TAG:-22.04}"
        );
        assert!(analysis.images[0].components.is_none());
        assert_eq!(analysis.final_stage, Some("build".to_string()));
        assert_eq!(
            analysis.multistage_analysis.unused_stages,
            vec!["build".to_string()]
        );
    }

    #[test]
    fn test_repr_uses_python_none_and_nested_components() {
        let analysis = analyze_dockerfile("FROM alpine\nARG VERSION\n").unwrap();
        let rendered = analysis.repr();
        assert!(rendered.contains("final_stage=None"));
        assert!(rendered.contains("args={\"VERSION\": None}"));
        assert!(rendered.contains(
            "components=ImageComponents(registry=None, name=\"alpine\", tag=None, digest=None)"
        ));
        assert!(!rendered.contains("components=\"ImageComponents"));
        assert!(!rendered.contains("Some("));
    }
}
