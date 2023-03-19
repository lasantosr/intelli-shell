FROM rust

# Install clippy and nightly rustfmt
RUN rustup component add clippy
RUN rustup toolchain install nightly
RUN rustup component add rustfmt --toolchain nightly-x86_64-unknown-linux-gnu

# Install some cargo utilities
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN cargo install cargo-audit
RUN curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin

# Install zsh
RUN apt-get update && apt-get install zsh -y

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

# Source artifact to be ready to execute
RUN echo "\n# Search up & down" >> ~/.bashrc
RUN echo "bind '\"\e[A\": history-search-backward'" >> ~/.bashrc
RUN echo "bind '\"\e[B\": history-search-forward'" >> ~/.bashrc
RUN echo "\n# IntelliShell debug" >> ~/.bashrc
RUN echo "alias intelli-shell=/workspaces/intelli-shell/target/debug/intelli-shell" >> ~/.bashrc
RUN echo "source /workspaces/intelli-shell/intelli-shell.sh" >> ~/.bashrc
RUN echo "alias ll='ls -alF'" >> ~/.bashrc
