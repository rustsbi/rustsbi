# RustSBI Prototyper

RustSBI Prototyper is a developing RISC-V Secure Bootloader solution. It can be integrated with the Rust or C language ecosystem to form a complete RISC-V bootloader ecosystem.

## Setting Up the Development Environment

### Packages to be installed

```bash
cargo install cargo-binutils
sudo apt install u-boot-tools
```


### Optional Tools

The following tools are not mandatory but can be useful for enhancing your development experience.

#### Install pre-commit

pre-commit is a tool that runs code checks before you commit your code.

```bash
pipx install pre-commit

# After installation, run pre-commit install to set it up for your project.
pre-commit install
```

#### Install Cargo Deny

Cargo deny is a Cargo plugin used to check the security of your dependencies.

```bash
cargo install --locked cargo-deny
```

#### Install typos

typos is a spell-checking tool.

```bash
cargo install typos-cli
```

#### Install git cliff

git cliff is a tool for generating changelogs.

```bash
cargo install git-cliff
```

## License

This project is dual-licensed under MIT or Mulan-PSL v2. See [LICENSE-MIT](./LICENSE-MIT) and [LICENSE-MULAN](./LICENSE-MULAN) for details.
