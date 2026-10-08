# syntax=docker/dockerfile:1.7
ARG FDB_VERSION=7.3.71

FROM rust:1-bookworm AS build
ARG FDB_VERSION
RUN apt-get update && apt-get install -y --no-install-recommends libclang-dev && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL -o /tmp/fdb.deb "https://github.com/apple/foundationdb/releases/download/${FDB_VERSION}/foundationdb-clients_${FDB_VERSION}-1_amd64.deb" \
    && dpkg -i /tmp/fdb.deb && rm /tmp/fdb.deb
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p zoen-relay && cp target/release/zoen-relay /zoen-relay

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 zoen
COPY --from=build /usr/lib/libfdb_c.so /usr/lib/libfdb_c.so
COPY --from=build /zoen-relay /usr/local/bin/zoen-relay
COPY infra/fdb/client-entrypoint.sh /usr/local/bin/client-entrypoint.sh
USER zoen
ENV ZOEN_BIND=0.0.0.0:8080 ZOEN_METRICS_BIND=0.0.0.0:9091 LOG_FORMAT=json FDB_CLUSTER_FILE=/tmp/fdb.cluster
EXPOSE 8080 9091
ENTRYPOINT ["/usr/local/bin/client-entrypoint.sh", "/usr/local/bin/zoen-relay"]
