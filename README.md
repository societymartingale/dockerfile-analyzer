# Dockerfile Analyzer

## Description

This Python package analyzes a Dockerfile and returns detailed analysis information. Some of the metadata returned include:

- Number of stages and stage names
- Base images used
- Multistage analysis (if applicable)
- Instructions statistics
- Environment variables, labels, and arguments
- Exposed ports

The code leverages the highly useful Rust crates `parse-dockerfile` and `docker-image`.

## Example

### Sample Input Dockerfile

```Dockerfile
ARG NODE_VERSION=18.17.0
ARG ALPINE_VERSION=3.18

# Stage 1: Build dependencies and compile application
FROM node:${NODE_VERSION}-alpine${ALPINE_VERSION} AS builder

# Set build-time variables
ARG BUILD_DATE
ARG VCS_REF
ARG VERSION=1.0.0

# Add metadata labels
LABEL maintainer="devops@company.com" \
      org.label-schema.build-date=$BUILD_DATE \
      org.label-schema.name="sample-app" \
      org.label-schema.description="Complex multi-stage Node.js application" \
      org.label-schema.url="https://company.com" \
      org.label-schema.vcs-ref=$VCS_REF \
      org.label-schema.vcs-url="https://github.com/company/sample-app" \
      org.label-schema.vendor="Company Inc." \
      org.label-schema.version=$VERSION \
      org.label-schema.schema-version="1.0"

# Install system dependencies and security updates
RUN apk update && apk upgrade && \
    apk add --no-cache \
        dumb-init \
        python3 \
        make \
        g++ \
        git \
        curl \
        ca-certificates && \
    rm -rf /var/cache/apk/*

# Create non-root user for security
RUN addgroup -g 1001 -S nodejs && \
    adduser -S nextjs -u 1001 -G nodejs

# Set working directory
WORKDIR /app

# Copy package files first for better layer caching
COPY package*.json ./
COPY yarn.lock ./

# Install dependencies
RUN yarn install --frozen-lockfile --production=false && \
    yarn cache clean

# Copy source code
COPY . .

# Build the application
RUN yarn build && \
    yarn install --frozen-lockfile --production=true && \
    yarn cache clean

# Stage 2: Create production image
FROM node:${NODE_VERSION}-alpine${ALPINE_VERSION} AS production

# Install runtime dependencies and security updates
RUN apk update && apk upgrade && \
    apk add --no-cache \
        dumb-init \
        curl \
        ca-certificates \
        tzdata && \
    rm -rf /var/cache/apk/*

# Create non-root user
RUN addgroup -g 1001 -S nodejs && \
    adduser -S nextjs -u 1001 -G nodejs

# Set working directory
WORKDIR /app

# Copy built application from builder stage
COPY --from=builder --chown=nextjs:nodejs /app/dist ./dist
COPY --from=builder --chown=nextjs:nodejs /app/node_modules ./node_modules
COPY --from=builder --chown=nextjs:nodejs /app/package.json ./package.json

# Create necessary directories and set permissions
RUN mkdir -p /app/logs /app/tmp && \
    chown -R nextjs:nodejs /app

# Switch to non-root user
USER nextjs

# Set environment variables
ENV NODE_ENV=production \
    PORT=3000 \
    LOG_LEVEL=info

# Expose port
EXPOSE $PORT

# Add health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:$PORT/health || exit 1

# Use dumb-init to handle signals properly
ENTRYPOINT ["dumb-init", "--"]

# Default command
CMD ["node", "dist/server.js"]

# Stage 3: Development image (optional)
FROM builder AS development

# Install development tools
RUN apk add --no-cache \
        vim \
        bash \
        htop

# Switch to non-root user
USER nextjs

# Set development environment variables
ENV NODE_ENV=development \
    PORT=3000 \
    LOG_LEVEL=debug

# Expose port and debug port
EXPOSE $PORT 9229

# Development command with hot reload
CMD ["yarn", "dev"]

# Stage 4: Testing image (optional)
FROM builder AS testing

# Install testing dependencies
RUN yarn install --frozen-lockfile

# Copy test files
COPY --chown=nextjs:nodejs tests/ ./tests/
COPY --chown=nextjs:nodejs jest.config.js ./

# Switch to non-root user
USER nextjs

# Run tests
CMD ["yarn", "test"]
```

### Sample Python Code

```sh
pip install dockerfile-analyzer
```

```python
"""Usage: python example.py <dockerfile path>"""

from sys import argv
import dockerfile_analyzer as da
import json


def get_dockerfile_contents(fname: str) -> str:
    with open(fname) as f:
        return f.read()


def main() -> None:
    df = get_dockerfile_contents(argv[1])
    res = da.analyze_dockerfile(df)
    print(res)
    print(json.dumps(res.to_dict(), indent=4))


if __name__ == "__main__":
    main()
```

### Output

```
Analysis(num_stages=4, images=[Image(full="builder", components="ImageComponents(registry=None, name=\"builder\", tag=None, digest=None)"), Image(full="node:${node_version}-alpine${alpine_version}", components="None")], stage_names=["builder", "development", "production", "testing"], copy_from_stages=["builder"], add_from_stages=[], multistage_analysis=MultistageAnalysis(is_multistage=true, stages_used_as_base_images=["builder"], stages_copied_from=["builder"], stages_added_from=[], unused_stages=["development", "production", "testing"]), exposed_ports=["$PORT", "9229"], instructions=InstructionStats(total_count=41, by_type={"ARG": 5, "COPY": 8, "HEALTHCHECK": 1, "RUN": 9, "EXPOSE": 2, "ENTRYPOINT": 1, "LABEL": 1, "WORKDIR": 2, "ENV": 2, "CMD": 3, "FROM": 4, "USER": 3}), args={"BUILD_DATE": None, "VERSION": Some("1.0.0"), "NODE_VERSION": Some("18.17.0"), "VCS_REF": None, "ALPINE_VERSION": Some("3.18")}, labels={"org.label-schema.build-date": "$BUILD_DATE", "maintainer": "devops@company.com", "org.label-schema.version": "$VERSION", "org.label-schema.description": "Complex multi-stage Node.js application", "org.label-schema.vcs-ref": "$VCS_REF", "org.label-schema.schema-version": "1.0", "org.label-schema.url": "https://company.com", "org.label-schema.vcs-url": "https://github.com/company/sample-app", "org.label-schema.vendor": "Company Inc.", "org.label-schema.name": "sample-app"}, env_vars={"PORT": "3000", "NODE_ENV": "development", "LOG_LEVEL": "debug"})
```

```json
{
  "num_stages": 4,
  "images": [
    {
      "full": "builder",
      "components": {
        "registry": null,
        "name": "builder",
        "tag": null,
        "digest": null
      }
    },
    {
      "full": "node:${node_version}-alpine${alpine_version}",
      "components": null
    }
  ],
  "stage_names": ["builder", "development", "production", "testing"],
  "copy_from_stages": ["builder"],
  "add_from_stages": [],
  "multistage_analysis": {
    "is_multistage": true,
    "stages_used_as_base_images": ["builder"],
    "stages_copied_from": ["builder"],
    "stages_added_from": [],
    "unused_stages": ["development", "production", "testing"]
  },
  "exposed_ports": ["$PORT", "9229"],
  "instructions": {
    "total_count": 41,
    "by_type": {
      "ARG": 5,
      "COPY": 8,
      "HEALTHCHECK": 1,
      "RUN": 9,
      "EXPOSE": 2,
      "ENTRYPOINT": 1,
      "LABEL": 1,
      "WORKDIR": 2,
      "ENV": 2,
      "CMD": 3,
      "FROM": 4,
      "USER": 3
    }
  },
  "args": {
    "BUILD_DATE": null,
    "VERSION": "1.0.0",
    "NODE_VERSION": "18.17.0",
    "VCS_REF": null,
    "ALPINE_VERSION": "3.18"
  },
  "labels": {
    "org.label-schema.build-date": "$BUILD_DATE",
    "maintainer": "devops@company.com",
    "org.label-schema.version": "$VERSION",
    "org.label-schema.description": "Complex multi-stage Node.js application",
    "org.label-schema.vcs-ref": "$VCS_REF",
    "org.label-schema.schema-version": "1.0",
    "org.label-schema.url": "https://company.com",
    "org.label-schema.vcs-url": "https://github.com/company/sample-app",
    "org.label-schema.vendor": "Company Inc.",
    "org.label-schema.name": "sample-app"
  },
  "env_vars": {
    "PORT": "3000",
    "NODE_ENV": "development",
    "LOG_LEVEL": "debug"
  }
}
```
