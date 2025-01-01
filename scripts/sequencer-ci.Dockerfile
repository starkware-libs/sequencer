FROM ubuntu:24.04

ARG USERNAME=ubuntu
ARG USER_UID=1000
ARG USER_GID=$USER_UID

RUN apt update && apt install -y sudo

RUN echo "#${USER_UID}        ALL=(ALL)       NOPASSWD: ALL" >> /etc/sudoers.d/developer

COPY install_build_tools.sh .
COPY dependencies.sh .

RUN ./install_build_tools.sh

USER ${USER_UID}:${USER_GID}

ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH=$PATH:${RUSTUP_HOME}/bin
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
