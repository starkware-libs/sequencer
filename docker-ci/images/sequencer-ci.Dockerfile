# syntax = devthefuture/dockerfile-x
# docker-ci/images/sequencer-ci.Dockerfile

# Dockerfile with multi-stage builds for efficient dependency caching and lightweight final image.
# For more on Docker stages, visit: https://docs.docker.com/build/building/multi-stage/
# We use dockerfile-x, for more information visit: https://github.com/devthefuture-org/dockerfile-x/blob/master/README.md

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
