#!/bin/env bash

set -e

[[ ${UID} == "0" ]] || SUDO="sudo"

function install_common_packages() {
    $SUDO  bash -c '
        apt update && \
        DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC apt -y install software-properties-common && \
        add-apt-repository ppa:deadsnakes/ppa && \
        apt update && \
        DEBIAN_FRONTEND=noninteractive TZ=Etc/UTC apt -y install \
            build-essential \
            clang \
            curl \
            gnupg \
            libzstd-dev \
            python3.9-dev \
            python3.9-venv \
            python3-dev \
            python3-venv \
            sudo \
            tzdata \
            wget
        '
}

function install_pypy() {
    pushd /opt
    $SUDO bash -c '
        curl -Lo pypy3.9-v7.3.11-linux64.tar.bz2 https://downloads.python.org/pypy/pypy3.9-v7.3.11-linux64.tar.bz2
        tar -xf pypy3.9-v7.3.11-linux64.tar.bz2
        rm pypy3.9-v7.3.11-linux64.tar.bz2
        chmod +x pypy3.9-v7.3.11-linux64/bin/pypy3

        if [ -L /usr/local/bin/pypy3.9 ]; then
            unlink /usr/local/bin/pypy3.9
        fi

        ln -s /opt/pypy3.9-v7.3.11-linux64/bin/pypy3 /usr/local/bin/pypy3.9

        if [ -L /opt/pypy3.9 ]; then
            unlink /opt/pypy3.9
        fi

        ln -s /opt/pypy3.9-v7.3.11-linux64 /opt/pypy3.9
        pypy3.9 -m ensurepip
        pypy3.9 -m pip install wheel
        '
    popd
}

function install_rust() {
    curl https://sh.rustup.rs -sSf | sh -s -- -y
}

function install_cargo_tools() {
    curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mitsuhiko/insta/releases/download/1.42.0/cargo-insta-installer.sh | sh
}

cd "$(dirname "$0")"
install_common_packages
install_pypy &
install_rust &
install_cargo_tools &
wait
./dependencies.sh
