FROM ghcr.io/actions/actions-runner:2.338.2@sha256:759a1d94a15b05941aba5196974ff43fe15b4cbba29b694b35cbc32a1166b232

ARG VERSION
LABEL org.opencontainers.image.source=https://github.com/codeh007/mtm-gh-runner \
      org.opencontainers.image.version=${VERSION} \
      org.opencontainers.image.licenses=MIT

USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential gh git-lfs libnss3-tools libssl-dev pkg-config \
        python3-venv rsync xz-utils zip \
        fonts-freefont-ttf fonts-ipafont-gothic fonts-liberation fonts-noto-color-emoji \
        fonts-tlwg-loma-otf fonts-unifont fonts-wqy-zenhei libasound2t64 \
        libatk-bridge2.0-0t64 libatk1.0-0t64 libatspi2.0-0t64 libcairo2 libcups2t64 \
        libdbus-1-3 libdrm2 libfontconfig1 libfreetype6 libgbm1 libglib2.0-0t64 \
        libnspr4 libnss3 libpango-1.0-0 libx11-6 libxcb1 libxcomposite1 libxdamage1 \
        libxext6 libxfixes3 libxkbcommon0 libxrandr2 xfonts-cyrillic xfonts-scalable xvfb \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o runner -g docker /home/runner/.local /opt/cache \
    && chmod go-w /home/runner

ENV NPM_CONFIG_PREFIX=/home/runner/.local
ENV NPM_CONFIG_CACHE=/opt/cache/npm
ENV pnpm_config_store_dir=/opt/cache/pnpm-store
ENV XDG_CACHE_HOME=/opt/cache
ENV RUNNER_TOOL_CACHE=/opt/cache/toolcache
ENV GOPATH=/opt/cache/go
ENV CARGO_HOME=/opt/cache/cargo
ENV RUSTUP_HOME=/opt/cache/rustup
ENV CARGO_TARGET_DIR=/opt/cache/cargo-target
ENV GRADLE_USER_HOME=/opt/cache/gradle
ENV ANDROID_HOME=/opt/cache/android-sdk
ENV ANDROID_SDK_ROOT=/opt/cache/android-sdk
ENV PATH=/opt/cache/cargo/bin:/home/runner/.local/bin:${PATH}
USER runner
