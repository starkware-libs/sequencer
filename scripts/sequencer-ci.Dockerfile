FROM ubuntu:20.04

ARG USERNAME=sequencer
ARG USER_UID=1000
ARG USER_GID=$USER_UID
ENV DEBIAN_FRONTEND=noninteractive
ENV TZ=America/New_York

RUN apt update && apt -y install \
    sudo \
    build-essential \
    clang \
    curl \
    python3-dev

RUN groupadd --gid $USER_GID $USERNAME && \
    useradd -s /bin/bash --uid $USER_UID --gid $USER_GID -m $USERNAME
RUN echo "%${USERNAME}        ALL=(ALL)       NOPASSWD: ALL" >> /etc/sudoers.d/developer

USER ${USERNAME}

ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH=$PATH:${RUSTUP_HOME}/bin

ENV MLIR_SYS_180_PREFIX=/usr/lib/llvm-18/
ENV LLVM_SYS_181_PREFIX=/usr/lib/llvm-18/
ENV TABLEGEN_180_PREFIX=/usr/lib/llvm-18/

COPY install_build_tools.sh .
COPY dependencies.sh .

RUN ./install_build_tools.sh
