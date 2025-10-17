FROM rust:1.90-slim-bookworm AS rust
ENV TZ=UTC
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install curl build-essential pkg-config libssl-dev wget tar gawk \
    --no-install-recommends -y && apt-get autoremove -y && apt-get clean && \
    rm -rf /var/lib/apt/lists/* /var/tmp/ /var/cache/apt
RUN /bin/bash -c "curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash"
RUN /bin/bash -c "cargo binstall trunk"
RUN rustup target add wasm32-unknown-unknown
COPY tracked_error tracked_error
COPY tracer-backend tracer-backend
COPY io-providers/axum-io-provider io-providers/axum-io-provider
COPY io-providers/datetime-io-provider io-providers/datetime-io-provider
COPY io-providers/gel-io-provider io-providers/gel-io-provider
COPY io-providers/gel-io-to-parameters io-providers/gel-io-to-parameters
COPY tracer tracer
COPY tracer-ui tracer-ui
COPY api-structs api-structs
COPY Cargo.lock .
COPY Cargo.toml .
ARG API_SERVER_URL_NO_TRAILING_SLASH
RUN trunk build --release --config=./tracer-ui/trunk.toml --dist=dist index.html
ARG GIT_COMMIT
RUN cargo build --release --bin tracer-backend

FROM debian:bookworm-slim AS binary
ENV TZ=UTC
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install build-essential pkg-config libssl-dev ca-certificates \
    --no-install-recommends -y && apt-get autoremove -y && apt-get clean && \
    rm -rf /var/lib/apt/lists/* /var/tmp/ /var/cache/apt
COPY --from=rust target/release/tracer-backend /usr/local/bin
COPY --from=rust tracer-ui/dist /usr/local/bin/tracer-ui/dist
WORKDIR /usr/local/bin/
ENTRYPOINT ["./tracer-backend"]
