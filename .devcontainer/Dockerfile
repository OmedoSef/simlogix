FROM rust:1-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    git \
    curl \
    ca-certificates \
    libclang-dev \
    libgtk-3-dev \
    libxcb-render0-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev \
    libxkbcommon-x11-dev \
    libssl-dev \
    libx11-dev \
    libxi-dev \
    libxcursor-dev \
    libxrandr-dev \
    libxext-dev \
    libgl1-mesa-dev \
    && rm -rf /var/lib/apt/lists/*

ARG USERNAME=vscode
RUN useradd -ms /bin/bash $USERNAME

USER $USERNAME
WORKDIR /workspace

# Run as vscode (not root) so the cargo registry cache these populate stays
# owned by the same user who'll be running cargo afterward — doing this as
# root left root-owned files blocking vscode from writing new cache entries.
RUN rustup component add clippy rustfmt
RUN cargo install cargo-audit --locked
