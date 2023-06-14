FROM rust:1.70-slim-bullseye as rust
ENV TZ=UTC
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install binaryen build-essential pkg-config libssl-dev wget tar gawk \
    --no-install-recommends -y && apt-get autoremove -y && apt-get clean && \
    rm -rf /var/lib/apt/lists/* /var/tmp/ /var/cache/apt
RUN /bin/bash -c 'ARCH=`uname -m` && \
                      if [ "$ARCH" == "x86_64" ]; then \
                         echo "x86_64" && \
                         wget -qO- https://github.com/thedodd/trunk/releases/download/v0.16.0/trunk-x86_64-unknown-linux-gnu.tar.gz | tar -xzf- && chmod +x ./trunk; \
                      else \
                         echo "non x86_64 arch $ARCH, installing with cargo" && \
                         cargo install --root . --locked trunk && \
                         mv ./bin/trunk . && \
                         cargo install --locked wasm-bindgen-cli; \
                      fi'
# RUN wget -qO- https://github.com/thedodd/trunk/releases/download/v0.16.0/trunk-x86_64-unknown-linux-gnu.tar.gz | tar -xzf- && chmod +x ./trunk
RUN rustup target add wasm32-unknown-unknown
COPY tracer-backend tracer-backend
COPY tracing-config-helper tracing-config-helper
COPY tracer-ui tracer-ui
COPY api-structs api-structs
COPY Cargo.lock .
COPY Cargo.toml .
COPY sqlx-data.json .
ARG API_SERVER_URL_NO_TRAILING_SLASH
ARG FRONTEND_PUBLIC_URL_PATH_NO_TRAILING_SLASH
RUN ./trunk build --release tracer-ui/index.html --dist=tracer-ui/dist
WORKDIR ../
ARG GIT_COMMIT
RUN cargo build --release --bin tracer-backend

FROM debian:bullseye-slim as binary
ENV TZ=UTC
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install build-essential pkg-config libssl-dev ca-certificates \
    --no-install-recommends -y && apt-get autoremove -y && apt-get clean && \
    rm -rf /var/lib/apt/lists/* /var/tmp/ /var/cache/apt
COPY --from=rust target/release/tracer-backend /usr/local/bin
COPY --from=rust tracer-ui/dist /usr/local/bin/tracer-ui/dist
WORKDIR /usr/local/bin/
ENTRYPOINT ["./tracer-backend"]
