# TecExp: Tec Markdown Exporter

`tecexp` export Markdown documents from Obsidian to Hugo. `tecexp` find documents with property `publish: web` in Obsidian vault directory and export them to Hugo posts directory.

## Install

Clone the repository and use `cargo-install` to install

```shell
git clone https://github.com/tecposter/tecexp.git
cd tecexp
cargo install --path .
```

If rust is installed by asdf, run the following command to recreate shims

```shell
asdf reshim rust
```

## Usage

```shell
❯ tecexp -h
Export mds from Obsidian to Hugo

Usage: tecexp [OPTIONS] --obsidian-dir <OBSIDIAN_DIR> --hugo-dir <HUGO_DIR>

Options:
  -o, --obsidian-dir <OBSIDIAN_DIR>        Obsidian vault dir
  -g, --hugo-dir <HUGO_DIR>                Hugo dir
  -p, --hugo-posts-dir <HUGO_POSTS_DIR>    Hugo posts sub dir [default: content/posts]
  -a, --hugo-assets-dir <HUGO_ASSETS_DIR>  Hugo assets sub dir [default: content/assets]
  -w, --watch                              Watch
  -h, --help                               Print help
  -V, --version                            Print version
```
