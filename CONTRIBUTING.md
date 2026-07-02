# Contribuindo com o dev-cli

Obrigado pelo interesse! Este é um projeto pessoal de aprendizado de Rust
(veja [`docs/dev-cli-mentorship.md`](docs/dev-cli-mentorship.md)), então PRs
pequenos e focados são mais bem-vindos do que grandes reescritas.

## Rodando o projeto localmente

Requer [Rust](https://rustup.rs/) — o `rust-toolchain.toml` já fixa o canal
`stable`, então o `rustup` cuida de instalar a versão certa automaticamente.

```bash
cargo build
cargo run -- version
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

O CI (`.github/workflows/rust.yml`) roda esses quatro comandos em todo
push/PR pra `main`. Um PR com warning de clippy ou diff de `cargo fmt` conta
como incompleto.

## Convenções

- **Português (pt-br)** em nomes de struct, função e variável. Inglês só em
  crates/traits externas e nos nomes de subcomando da CLI pública.
- **Comentários didáticos são bem-vindos** — o projeto é material de estudo,
  então comente o "porquê"/conceito de Rust por trás de trechos não óbvios,
  sem parafrasear o óbvio.
- **Sem `unwrap()` fora de teste** (`#[cfg(test)]`).
- Separe o **núcleo puro** (sem IO, testável com dados inline) da **casca de
  IO** (arquivo, banco, rede) — ver `src/logs.rs` e `src/ai/render.rs` como
  referência.
- Commits em [Conventional Commits](https://www.conventionalcommits.org/),
  em português: `<tipo>(<escopo>): <resumo no imperativo>` (`feat`, `fix`,
  `refactor`, `test`, `docs`, `chore`, `ci`).

## Abrindo um PR

1. Um assunto por PR — evite misturar refatoração com feature nova.
2. Rode os quatro comandos da seção acima antes de abrir.
3. Descreva o "porquê" da mudança, não só o "o quê" (o diff já mostra o quê).

## Nota para quem usa Windows

`AGENTS.md` é um symlink para `CLAUDE.md` (mesmas instruções, dois nomes de
convenção). Se o `git clone` no Windows materializar `AGENTS.md` como um
arquivo de texto contendo só a palavra `CLAUDE.md` em vez do conteúdo real,
habilite suporte a symlink antes de clonar: `git config --global core.symlinks true`
(exige o "Developer Mode" ativado no Windows, ou rodar o terminal como
administrador).

## Reportando bugs

Abra uma [issue](https://github.com/jarede/dev-cli/issues) com o comando
exato rodado, a saída obtida e a esperada. Se envolver dados sensíveis (ex:
conteúdo de `~/.claude/projects` ou do banco do OpenCode), descreva o
formato em vez de colar o conteúdo real.
