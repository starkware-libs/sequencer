FROM ubuntu:20.04

ARG USERNAME=sequencer
ARG USER_UID=1000
ARG USER_GID=$USER_UID
ARG SEQUENCER_DIR=local
ENV SEQUENCER_DIR=${SEQUENCER_DIR}

RUN apt update && apt install -y sudo

RUN groupadd --gid $USER_GID $USERNAME && \
    useradd -s /bin/bash --uid $USER_UID --gid $USER_GID -m $USERNAME
RUN echo "%${USERNAME}        ALL=(ALL)       NOPASSWD: ALL" >> /etc/sudoers.d/developer

USER ${USERNAME}

WORKDIR /cairo_native

ENV RUSTUP_HOME=/var/tmp/rust
ENV CARGO_HOME=${RUSTUP_HOME}
ENV PATH=$PATH:${RUSTUP_HOME}/bin

COPY install_build_tools.sh .
COPY dependencies.sh .
COPY bootstrap.sh .

RUN ./install_build_tools.sh /cairo_native

ENTRYPOINT [ "sh", "-c", "/cairo_native/bootstrap.sh ${SEQUENCER_DIR} $0 $@" ]