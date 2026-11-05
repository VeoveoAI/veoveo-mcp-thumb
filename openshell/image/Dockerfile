# syntax=docker/dockerfile:1.27@sha256:4edf897a3ffa55b89f906fc8cc78afdb3f1834cc9c7083565e611a8a7d5fe99e
# The factory environment: the toolchain that builds a branch in its OpenShell sandbox, and the
# standard programs that draw files. The installation mounts the Lore CLI that fetches it.
FROM rust:1.99.0-trixie@sha256:6ff07edce8775d0f64be7aba9197229407301bddf2054d62c27b541a6238a181
USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates clang cmake git pkg-config tzdata \
        ffmpeg imagemagick librsvg2-bin poppler-utils \
        libreoffice-writer libreoffice-calc libreoffice-impress libreoffice-draw \
        fonts-liberation fonts-dejavu-core fonts-noto-cjk \
        libgl1 libegl1 libglvnd0 libglx0 libopengl0 libx11-6 libxext6 libxcursor1 libxrender1 libxi6 \
    && rm -rf /var/lib/apt/lists/* /var/log/apt /var/log/dpkg.log \
    && rustup component add clippy rustfmt \
    && rm -rf /usr/local/cargo/registry
# F3D's own release: Debian's has no EGL, so it could not draw on the GPU without a display. It goes
# under /usr, which the factory's policy lets a sandbox read.
ADD --checksum=sha256:7fd4c139b74fb2ec411d81b4675dce2f721dd4d244c0a7c5f3d0bde0d15e811b \
    https://github.com/f3d-app/f3d/releases/download/v4.0.1/F3D-4.0.1-Linux-x86_64.tar.gz /tmp/f3d.tar.gz
RUN mkdir -p /usr/local/f3d \
    && tar -xzf /tmp/f3d.tar.gz --strip-components=1 -C /usr/local/f3d \
    && rm /tmp/f3d.tar.gz \
    && ln -s /usr/local/f3d/bin/f3d /usr/local/bin/f3d \
    && f3d --version
# This repository draws 3D files on the GPU: under the nvidia runtime these ask for the device and
# for the driver's libraries, which the packages above let it load.
ENV NVIDIA_VISIBLE_DEVICES=all NVIDIA_DRIVER_CAPABILITIES=all
# The persistent disk keeps fetched crates across restarts; cargo fetches pinned git
# dependencies with git, which the policy admits.
ENV CARGO_HOME=/sandbox/.cargo CARGO_NET_GIT_FETCH_WITH_CLI=true
# OpenShell runs the workload as uid 10001.
RUN groupadd --gid 10001 sandbox \
    && useradd --create-home --uid 10001 --gid 10001 --shell /bin/bash sandbox \
    && chown -R sandbox:sandbox /usr/local/rustup /usr/local/cargo \
    && mkdir -p /sandbox && chown sandbox:sandbox /sandbox
USER sandbox
WORKDIR /sandbox
