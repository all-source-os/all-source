FROM rust:1.94-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /audit
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

FROM scratch AS binary
COPY --from=build /audit/target/release/archive-budget-audit /archive-budget-audit
