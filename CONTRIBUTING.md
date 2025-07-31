# Contributing to IntelliShell

First off, thank you for considering contributing to IntelliShell! It's people like you that make open source such a
great community. We welcome any form of contribution, from reporting a bug to submitting a feature request, or even
writing documentation.

## How to Contribute

There are several ways you can contribute to the project:

- **Reporting Bugs**: If you find a bug, please open an issue on our [GitHub repository](https://github.com/lasantosr/intelli-shell/issues).
  Describe the issue in detail, including the steps to reproduce it, your operating system, and shell version.
- **Suggesting Enhancements**: If you have an idea for a new feature or an improvement to an existing one, feel free to
  open an issue to discuss it.
- **Pull Requests**: If you'd like to contribute code, you can open a pull request. Please make sure to read the
  development setup section below before you start.

## Development Setup

To get started with the development of IntelliShell, you'll need to have the following prerequisites installed on your system:

- **Rust**: The project is written in Rust, so you'll need to have the Rust toolchain installed. You can install it
  from [rustup.rs](https://rustup.rs/).
- **Git**: You'll need Git to clone the repository and contribute code.

Once you have the prerequisites, you can set up your development environment with the following steps:

1. **Clone the repository**:

    ```sh
    git clone https://github.com/lasantosr/intelli-shell.git
    cd intelli-shell
    ```

2. **Build the project**:

    ```sh
    cargo build
    ```

3. **Run the tests**:

    ```sh
    cargo test
    ```

4. **Format the code**: Before committing your changes, make sure to format the code using the nightly toolchain:

    ```sh
    cargo +nightly fmt
    ```

Alternatively, this repository is equipped with a **Dev Container** and an **IDX/Firebase** setup, which allows you to
get a full development environment with all the necessary tools and configurations.

### Optional Dependencies for Documentation

If you plan to work on the documentation, you might need the following tools:

- [**mdbook**](https://github.com/rust-lang/mdBook): Used to build the project's book/documentation
- [**vhs**](https://github.com/charmbracelet/vhs): Used to generate the animated GIF demos from `.tape` files

## Project Structure

For an overview of the project structure, you can refer to the `lib.rs` file, which provides a high-level overview of
the different modules and their responsibilities. Additionally, you can find more detailed documentation in the
project's [book](https://lasantosr.github.io/intelli-shell/).
