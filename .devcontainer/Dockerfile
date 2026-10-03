FROM ubuntu:26.04
ENV PATH="/root/.cargo/bin:${PATH}"
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh \
    && sh /tmp/rustup.sh -y --profile minimal --default-toolchain none \
    && rm /tmp/rustup.sh
WORKDIR /opt/signaltty
COPY rust-toolchain.toml ./
COPY scripts/setup-dev.sh scripts/dev-packages.txt scripts/verify.sh scripts/verify.py ./scripts/
RUN scripts/setup-dev.sh --system
WORKDIR /workspace
