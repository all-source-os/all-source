FROM rust:1.95-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM prom/prometheus:v2.51.0 AS prometheus
FROM prom/alertmanager:v0.27.0 AS alertmanager
FROM alpine:3.22
RUN apk add --no-cache ca-certificates curl
COPY --from=prometheus /bin/prometheus /bin/promtool /bin/
COPY --from=alertmanager /bin/alertmanager /bin/amtool /bin/
COPY --from=build /build/target/release/monitor-stack /bin/
WORKDIR /app
COPY prometheus.yml /app/prometheus.yml
COPY rules /app/rules
ENTRYPOINT ["/bin/monitor-stack"]
