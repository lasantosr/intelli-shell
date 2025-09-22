FROM tsl0922/ttyd:alpine AS ttyd
FROM rust

# Add vhs repo
RUN mkdir -p /etc/apt/keyrings
RUN curl -fsSL https://repo.charm.sh/apt/gpg.key | gpg --dearmor -o /etc/apt/keyrings/charm.gpg
RUN echo "deb [signed-by=/etc/apt/keyrings/charm.gpg] https://repo.charm.sh/apt/ * *" | tee /etc/apt/sources.list.d/charm.list

# Update apt-get first
RUN apt-get update

# Install clippy and nightly rustfmt
RUN rustup component add clippy
RUN rustup toolchain install nightly
RUN rustup component add rustfmt --toolchain nightly-x86_64-unknown-linux-gnu

# Install latest ttyd
COPY --from=ttyd /usr/bin/ttyd /usr/bin/ttyd

# Install zsh, fish and required packages
RUN apt-get install -y zsh fish gnupg2 bat bc ffmpeg chromium vhs

# Install some cargo utilities
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall --no-confirm --no-symlinks cargo-audit cargo-nextest cargo-hack mdbook

# Create default non-root user
ARG USERNAME=rust
ARG USER_UID=1000
ARG USER_GID=$USER_UID
RUN groupadd --gid $USER_GID $USERNAME \
    && useradd --uid $USER_UID --gid $USER_GID -m $USERNAME \
    && apt-get install -y sudo \
    && echo $USERNAME ALL=\(root\) NOPASSWD:ALL > /etc/sudoers.d/$USERNAME \
    && chmod 0440 /etc/sudoers.d/$USERNAME \
    && chown -R $USERNAME:$USERNAME /usr/local/cargo \
    && chown -R $USERNAME:$USERNAME /usr/local/rustup \
    && chsh -s /bin/bash $USERNAME
USER $USERNAME

ENV VHS_NO_SANDBOX "true"

# Useful additions to shell profiles
RUN echo '\n# Customize PS1 with git branch' >> ~/.bashrc
RUN echo 'parse_git_branch() {' >> ~/.bashrc
RUN echo "  git branch 2> /dev/null | sed -e '/^[^*]/d' -e 's/* \\(.*\\)/ (\\\\1) /'" >> ~/.bashrc
RUN echo '}' >> ~/.bashrc
RUN echo "PS1=\${PS1/'\\\\[\\\\033[00m\\\\]\\\\$ '/'\\\\[\\\\033[33m\\\\]\$(parse_git_branch)\\\\[\\\\033[00m\\\\]\\\\$ '}" >> ~/.bashrc
RUN echo '\n# Aliases' >> ~/.bashrc
RUN echo "alias ll='ls -alF'" >> ~/.bashrc
RUN echo '\n# Search up & down' >> ~/.bashrc
RUN echo "bind '\"\\\\e[A\": history-search-backward'" >> ~/.bashrc
RUN echo "bind '\"\\\\e[B\": history-search-forward'" >> ~/.bashrc

RUN echo "alias ll='ls -alF'" >> ~/.zshrc
RUN echo "\n# Search up & down" >> ~/.zshrc
RUN echo "bindkey '^[[A' up-line-or-search" >> ~/.zshrc
RUN echo "bindkey '^[[B' down-line-or-search" >> ~/.zshrc
