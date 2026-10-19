# syntax=docker/dockerfile:1.7
FROM rust:1.99.1-bookworm AS build
ARG FDB_VERSION=7.3.71
RUN apt-get update && apt-get install -y --no-install-recommends libclang-dev cmake pkg-config && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL -o /tmp/fdb.deb "https://github.com/apple/foundationdb/releases/download/${FDB_VERSION}/foundationdb-clients_${FDB_VERSION}-1_amd64.deb" \
    && dpkg -i /tmp/fdb.deb && rm /tmp/fdb.deb
ENV CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_DEBUG=0
WORKDIR /src
COPY . .
COPY infra/local/s3-contract.rs crates/zoen-relay/examples/local-s3-contract.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p zoen-relay --example local-s3-contract \
    && cp target/release/examples/local-s3-contract /s3-contract

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/lib/libfdb_c.so /usr/lib/libfdb_c.so
COPY --from=build /s3-contract /usr/local/bin/s3-contract
ENTRYPOINT ["/usr/local/bin/s3-contract"]
