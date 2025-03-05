# syntax = devthefuture/dockerfile-x

INCLUDE deployments/images/base/Dockerfile

FROM base AS builder

ARG USERNAME=sequencer
ARG USER_UID=1000
ARG USER_GID=$USER_UID

RUN apt update && apt install -y sudo

RUN groupadd --gid $USER_GID $USERNAME || true
RUN useradd -s /bin/bash --uid $USER_UID --gid $USER_GID -m $USERNAME || \
    usermod --login ${USERNAME} --move-home --home /home/${USERNAME} `grep ${USER_UID} /etc/passwd | awk -F: '{print $1}'`

RUN echo "#${USER_UID}        ALL=(ALL)       NOPASSWD: ALL" >> /etc/sudoers.d/developer

USER ${USERNAME}
