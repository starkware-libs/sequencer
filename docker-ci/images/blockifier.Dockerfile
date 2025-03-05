FROM ubuntu:22.04

ENV ID=1001
ENV USER=runner
ENV USER_HOME=/home/${USER}

RUN apt update && \
    DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC apt -y install \
    ca-certificates \
    libssl-dev \
    pkg-config \
    ripgrep \
    software-properties-common \
    zstd \
    lld \
    build-essential \
    clang \
    curl \
    gnupg \
    libzstd-dev \
    python3-dev \
    sudo \
    tzdata \
    wget \
    git

RUN bash -c 'curl https://apt.llvm.org/llvm.sh -Lo llvm.sh && \
    bash ./llvm.sh 19 all && \
    rm -f ./llvm.sh && \
    apt update && apt install -y \
        libgmp3-dev \
        libmlir-19-dev \
        libpolly-19-dev \
        libzstd-dev \
        mlir-19-tools'

# Install pypy3.9
RUN bash -c 'pushd /opt; \
    curl -Lo pypy3.9-v7.3.11-linux64.tar.bz2 https://downloads.python.org/pypy/pypy3.9-v7.3.11-linux64.tar.bz2 && \
    tar -xf pypy3.9-v7.3.11-linux64.tar.bz2 && \
    rm pypy3.9-v7.3.11-linux64.tar.bz2 && \
    chmod +x pypy3.9-v7.3.11-linux64/bin/pypy3 && \
    ln -s /opt/pypy3.9-v7.3.11-linux64/bin/pypy3 /usr/local/bin/pypy3.9 && \
    ln -s /opt/pypy3.9-v7.3.11-linux64 /opt/pypy3.9 && \
    pypy3.9 -m ensurepip && \
    pypy3.9 -m pip install wheel; \
    popd'

RUN set -ex; \
    addgroup --gid ${ID} ${USER} && \
    adduser --ingroup $(getent group ${ID} | cut -d: -f1) --uid ${ID} --gecos "" --disabled-password --home ${USER_HOME} ${USER} && \
    chown -R ${USER}:${USER} ${USER_HOME} && \
    usermod -aG sudo ${USER} && \
    echo "%sudo   ALL=(ALL:ALL) NOPASSWD:ALL" > /etc/sudoers

WORKDIR ${USER_HOME}

USER ${ID}

ENV CARGO_HOME=${USER_HOME}/.cargo
ENV PATH=$PATH:${CARGO_HOME}/bin

COPY scripts/install_rust.sh .

RUN ./install_rust.sh --versions nightly-2023-07-05,nightly-2024-04-29 \
    --default-version nightly-2024-04-29 \
    --components rustfmt,clippy \
    --tools cargo-machete@0.7.0,taplo-cli@0.9.3

# Cleanup
RUN sudo apt autoremove -y && \
    sudo apt clean && \
    sudo rm -rf /var/lib/apt/lists/* \
        install_build_tools.sh \
        dependencies.sh \
        ${CARGO_HOME}/registry \
        ${CARGO_HOME}/git
