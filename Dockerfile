# Image of scout-server, the price server on the homelab VM (docs/serveur.md); the desktop app is
# not in it. Built on the PC, cross-compiled for arm64 without emulation (same recipe as
# dofus_follow_up), from the homelab repo:
#
#   tools/release.sh ~/dev/outils-jeux/wow/scout-rs <version> scout-server
#   docker buildx build --platform linux/arm64 -t <image> --load .   (by hand)

FROM --platform=$BUILDPLATFORM rust:1.99.0-bookworm AS build
ARG BUILDARCH
ARG TARGETARCH
# C cross toolchain only when building arm64 from another architecture (ring and zstd have C code).
RUN if [ "$TARGETARCH" = arm64 ] && [ "$BUILDARCH" != arm64 ]; then \
        apt-get update && apt-get install -y --no-install-recommends gcc-aarch64-linux-gnu libc6-dev-arm64-cross \
        && rm -rf /var/lib/apt/lists/* \
        && rustup target add aarch64-unknown-linux-gnu; \
    fi
WORKDIR /src
COPY . .
RUN if [ "$TARGETARCH" = arm64 ] && [ "$BUILDARCH" != arm64 ]; then \
        export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
               CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
               AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar; \
        target="--target aarch64-unknown-linux-gnu"; bin=target/aarch64-unknown-linux-gnu/release/scout-server; \
    else \
        target=""; bin=target/release/scout-server; \
    fi \
    && cargo build --release --locked -p scout-server $target \
    && mkdir -p /out && cp "$bin" /out/

FROM debian:12.15-slim
# System roots for TLS to Battle.net, copied from the build stage (same Debian): no RUN, no emulation.
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /out/scout-server /usr/local/bin/
WORKDIR /app
ENV SCOUT_DATA_DIR=/app/data SCOUT_LISTEN=0.0.0.0:3000
# uid 10001 owns data/ on the VM (module.toml [data] owner).
USER 10001:10001
EXPOSE 3000
CMD ["scout-server"]
