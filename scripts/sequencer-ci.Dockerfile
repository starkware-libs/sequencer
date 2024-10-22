FROM ubuntu:20.04

ARG DEBIAN_FRONTEND=noninteractive
ARG USERNAME=sequencer
ARG USER_UID=1000
ARG USER_GID=$USER_UID

RUN apt update && apt -y install \
    build-essential \
    clang \
    curl \
    python3-dev \
    sudo


RUN groupadd --gid $USER_GID $USERNAME && \
    useradd -s /bin/bash --uid $USER_UID --gid $USER_GID -m $USERNAME
RUN echo "%${USERNAME}        ALL=(ALL)       NOPASSWD: ALL" >> /etc/sudoers.d/developer

USER ${USERNAME}

ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH=$PATH:${RUSTUP_HOME}/bin

COPY install_build_tools.sh .
RUN bash install_build_tools.sh
