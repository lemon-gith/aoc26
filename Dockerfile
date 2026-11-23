# Dev container for this repo: the Rust toolchain, clippy, bacon and the `aoc` helper
# all live in here, so nothing needs installing on your machine except Docker.
# You normally don't build this by hand: `./start` does it.

ARG RUST_VERSION=1.100
FROM rust:${RUST_VERSION}-slim-trixie

RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates less procps \
 && rm -rf /var/lib/apt/lists/*

# rust-analyzer + rust-src are for editors attached to the container
RUN rustup component add clippy rustfmt rust-analyzer rust-src

# bacon, for anyone who wants a plain watcher too (`aoc watch` doesn't need it)
RUN cargo install --locked bacon && rm -rf /usr/local/cargo/registry

# The helper, installed as `aoc`. `./start` rebuilds this layer whenever aoc_helper/ changes.
COPY aoc_helper /opt/aoc_helper
RUN cargo install --locked --path /opt/aoc_helper --root /usr/local \
 && rm -rf /opt/aoc_helper /usr/local/cargo/registry

# Run as your own user id, so files created in the mounted repo belong to you
ARG UID=1000
ARG GID=1000
RUN groupadd -o -g "$GID" aoc \
 && useradd -o -m -u "$UID" -g "$GID" -s /bin/bash aoc \
 && mkdir -p /home/aoc/.cargo /home/aoc/target \
 && chown -R "$UID:$GID" /home/aoc \
 && printf '%s\n' \
    'echo -e "\e[1;36maoc\e[0m is ready: try \e[1maoc status\e[0m, \e[1maoc day 1\e[0m or \e[1maoc watch\e[0m (\e[1maoc help\e[0m for more)"' \
    >> /home/aoc/.bashrc

# login shells (some editors use them) reset PATH, so put cargo back on it there too
RUN echo 'export PATH=/usr/local/cargo/bin:$PATH' > /etc/profile.d/cargo.sh

USER aoc
# The toolchain stays in /usr/local; downloaded crates and build output go to volumes
ENV CARGO_HOME=/home/aoc/.cargo \
    CARGO_TARGET_DIR=/home/aoc/target \
    PATH=/usr/local/cargo/bin:$PATH \
    AOC_ROOT=/aoc
WORKDIR /aoc
