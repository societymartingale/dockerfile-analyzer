FROM docker.abc.com/base-images/python:3.13-debian@sha256:55f1d15ef4c37870e23c03e89ad238940b55c8ede9f13fac4b7d71c7955f1053 AS base

LABEL org.opencontainers.image.title="My App" \
      org.opencontainers.image.version="1.0" \
      org.opencontainers.image.authors="john@example.com"

ENV PYTHONPATH=/src \
    PYTHONUNBUFFERED=1 \
    REQUESTS_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt \
    PATH="/home/appuser/.local/bin:$PATH"
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
ENV GIT_COMMIT=$GIT_COMMIT
EXPOSE 5000

CMD ["uvicorn", "--host", "0.0.0.0", "--port", "5000", "app.main:app"]
