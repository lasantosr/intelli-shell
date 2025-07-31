# Import secrets if present
let
  secrets =
    if builtins.pathExists ./secrets.nix then
      import ./secrets.nix
    else
      { };
in
# To learn more about how to use Nix to configure your environment
# see: https://developers.google.com/idx/guides/customize-idx-env
{ pkgs, ... }: {
  # Which nixpkgs channel to use.
  channel = "stable-24.11";
  # Use https://search.nixos.org/packages to find packages
  packages = [
    pkgs.rustup
    pkgs.bash
    pkgs.zsh
    pkgs.fish
    pkgs.vhs
    pkgs.chromium
    pkgs.mdbook
    pkgs.cargo-binstall
    pkgs.cargo-audit
    pkgs.cargo-nextest
    pkgs.stdenv.cc
    pkgs.pkg-config
  ];
  # Sets environment variables in the workspace (including secrets)
  env = pkgs.lib.recursiveUpdate
    {
      PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
    }
    secrets;
  # Services
  services = {
    # Docker
    docker.enable = true;
  };
  # IDX config
  idx = {
    # Search for the extensions you want on https://open-vsx.org/ and use "publisher.id"
    extensions = [
      "rust-lang.rust-analyzer"
      "fill-labs.dependi"
      "tamasfe.even-better-toml"
      "vadimcn.vscode-lldb"
      "eamodio.gitlens"
      "usernamehw.errorlens"
      "aaron-bond.better-comments"
      "oderwat.indent-rainbow"
      "gruntfuggly.todo-tree"
      "skyapps.fish-vscode"
      "yzhang.markdown-all-in-one"
      "davidanson.vscode-markdownlint"
    ];
    workspace = {
      # Runs when a workspace is first created with this `dev.nix` file
      onCreate = {
        init-rustup = ''
          rustup toolchain install stable nightly
          rustup default stable
          rustup component add rust-src
        '';
        init-secrets = ''
          if [[ ! -f ".idx/secrets.nix" ]]; then
            cat > .idx/secrets.nix <<EOF
          {
            # GitHub Personal Access Token for Gists
            GIST_TOKEN = "...";
          }
          EOF
          fi
        '';
        shell-arrows = ''
          if ! grep -q "history-search-backward" ~/.bashrc; then
            echo -e '\n# Search up & down' >> ~/.bashrc
            echo 'bind '\'''"\e[A": history-search-backward'\' >> ~/.bashrc
            echo 'bind '\'''"\e[B": history-search-forward'\' >> ~/.bashrc
          fi
          if ! grep -q "up-line-or-search" ~/.zshrc; then
            echo -e '\n# Search up & down' >> ~/.zshrc
            echo "bindkey '^[[A' up-line-or-search" >> ~/.zshrc
            echo "bindkey '^[[B' down-line-or-search" >> ~/.zshrc
          fi
        '';
        post-create = ''
          .devcontainer/post-create.sh .
        '';
        # Open editors for the following files by default, if they exist:
        default.openFiles = [ "README.md" "src/lib.rs" ];
      };
      # To run something each time the workspace is (re)started, use the `onStart` hook
    };
  };
}
