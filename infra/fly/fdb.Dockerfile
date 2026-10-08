# syntax=docker/dockerfile:1.7
FROM debian:bookworm-slim
ARG FDB_VERSION=7.3.71
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && for p in clients server; do \
         curl -fsSL -o /tmp/$p.deb "https://github.com/apple/foundationdb/releases/download/${FDB_VERSION}/foundationdb-${p}_${FDB_VERSION}-1_amd64.deb"; \
       done \
    && dpkg-deb -x /tmp/clients.deb / && dpkg-deb -x /tmp/server.deb / \
    && rm -rf /tmp/*.deb /var/lib/apt/lists/* \
    && ln -s /usr/sbin/fdbserver /usr/local/bin/fdbserver
COPY infra/fdb/server-entrypoint.sh /usr/local/bin/server-entrypoint.sh
EXPOSE 4500
ENTRYPOINT ["/usr/local/bin/server-entrypoint.sh"]
