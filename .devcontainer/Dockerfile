# Use a multi-stage build to get the latest ttyd binary from a minimal image
FROM tsl0922/ttyd:alpine AS ttyd

# Start from the official Rust image, which is Debian-based
FROM rust

# Add Charm & GitHub CLI repos and update package lists
RUN mkdir -p /etc/apt/keyrings && \
    # Charm Repo
    curl -fsSL https://repo.charm.sh/apt/gpg.key | gpg --dearmor -o /etc/apt/keyrings/charm.gpg && \
    echo "deb [signed-by=/etc/apt/keyrings/charm.gpg] https://repo.charm.sh/apt/ * *" | tee /etc/apt/sources.list.d/charm.list && \
    # GitHub CLI Repo
    curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | gpg --dearmor -o /etc/apt/keyrings/githubcli-archive-keyring.gpg && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | tee /etc/apt/sources.list.d/github-cli.list

# Install system dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    # GitHub CLI
    gh \
    # Shells
    zsh \
    fish \
    # Utilities
    vim \
    sudo \
    gnupg2 \
    bat \
    bc \
    ffmpeg \
    chromium \
    # vhs from Charm repo
    vhs && \
    # Clean up apt cache to keep the image lean
    rm -rf /var/lib/apt/lists/*

# Install extra Rust components
RUN rustup component add clippy && \
    rustup toolchain install nightly && \
    rustup component add rustfmt --toolchain nightly-x86_64-unknown-linux-gnu

# Copy the ttyd binary from the first stage
COPY --from=ttyd /usr/bin/ttyd /usr/bin/ttyd

# Install cargo-binstall to quickly install pre-compiled Rust binaries
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# Install Nushell and other cargo utilities
RUN cargo binstall --no-confirm --no-symlinks nu cargo-audit cargo-nextest cargo-hack mdbook

# Create a non-root user and grant sudo privileges
ARG USERNAME=rust
ARG USER_UID=1000
ARG USER_GID=$USER_UID
RUN groupadd --gid $USER_GID $USERNAME && \
    useradd --uid $USER_UID --gid $USER_GID -m $USERNAME -s /bin/bash && \
    echo "$USERNAME ALL=(root) NOPASSWD:ALL" > /etc/sudoers.d/$USERNAME && \
    chmod 0440 /etc/sudoers.d/$USERNAME && \
    # Set correct ownership for cargo and rustup directories
    chown -R $USERNAME:$USERNAME /usr/local/cargo && \
    chown -R $USERNAME:$USERNAME /usr/local/rustup

# Switch to the new non-root user
USER $USERNAME
WORKDIR /home/$USERNAME

# Set the default editor to VSCode
ENV EDITOR="code"

# VHS requires this to run without a sandbox inside Docker
ENV VHS_NO_SANDBOX="true"

# Add useful aliases and functions to shell profiles
RUN cat <<'EOF' >> ~/.bashrc
# Customize PS1 with git branch
parse_git_branch() {
  git branch 2> /dev/null | sed -e "/^[^*]/d" -e "s/* \\(.*\\)/ (\\1) /"
}
PS1=${PS1/'\[\033[00m\]\$ '/'\[\033[33m\]$(parse_git_branch)\[\033[00m\]\$ '}

# Aliases
alias ll="ls -alF"

# Search shell history using up/down arrows
bind '"\e[A": history-search-backward'
bind '"\e[B": history-search-forward'
EOF

# Add useful aliases and functions to the zsh profile
RUN cat <<'EOF' >> ~/.zshrc
# Aliases
alias ll="ls -alF"

# Search up & down shell history
bindkey "^[[A" up-line-or-search
bindkey "^[[B" down-line-or-search
EOF

# Set the default command to start a bash shell
CMD ["/bin/bash"]
