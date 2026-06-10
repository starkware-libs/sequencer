# docker-ci/images/ci-base.Dockerfile
#
# CI runtime image for GitHub Actions. Equivalent to running the `bootstrap`
# composite action, but baked ahead of time: it is the published `base` image
# (system build deps, Rust toolchain, cargo tools, Sierra/native compilers)
# plus the only dependency `bootstrap` installs that `base` does not — Anvil.
#
# Intended to be referenced by workflows via `container:` so jobs can skip the
# `bootstrap` action. Only third-party tooling is baked here. The cargo
# registry/git sources and the sccache compilation cache are NOT part of this
# image — they are mounted from the workflow cache at runtime.

FROM ghcr.io/starkware-libs/sequencer/base:latest

# Foundry / Anvil. `bootstrap` installs this via the foundry-rs/foundry-toolchain
# action pinned to v1.5.1 (.github/actions/install_rust/action.yml); keep the
# same version here. FOUNDRY_DIR fixes the install location so the binaries land
# on a deterministic PATH (foundryup otherwise targets $HOME/.foundry, which
# varies by the runtime user).
ENV FOUNDRY_DIR=/opt/foundry
ENV PATH="${FOUNDRY_DIR}/bin:${PATH}"
RUN curl -L https://foundry.paradigm.xyz | bash && \
    foundryup --install v1.5.1

# Crate sources are not part of this image. The base image populated
# /var/tmp/rust/{registry,git} as a side effect of its `cargo install` steps;
# drop them so the image ships only binaries. At runtime these directories are
# mounted (empty, then populated) from the workflow cache, and the sccache
# compilation cache is mounted at $SCCACHE_DIR — neither comes from this image.
RUN rm -rf /var/tmp/rust/registry /var/tmp/rust/git
