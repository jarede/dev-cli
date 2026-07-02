# 🛠️ dev-cli

[![Rust](https://github.com/jarede/dev-cli/actions/workflows/rust.yml/badge.svg)](https://github.com/jarede/dev-cli/actions/workflows/rust.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

CLI em Rust — um canivete suíço para tarefas de desenvolvimento.

Este é um **projeto de aprendizado**: o objetivo é me tornar um engenheiro
Rust produtivo construindo uma ferramenta real, iteração a iteração. Veja
[`docs/dev-cli-mentorship.md`](docs/dev-cli-mentorship.md) para a filosofia de
aprendizado e o roadmap.

O código é comentado de forma didática, explicando o que cada trecho faz e o
conceito de Rust por trás — para servir de material de estudo.

## 📦 Instalação

Binários pré-compilados para macOS, Linux e Windows são publicados em cada
[release](https://github.com/jarede/dev-cli/releases) (veja `.github/workflows/release.yml`).

### macOS e Linux — Homebrew (recomendado)

A fórmula mora neste mesmo repositório, então o `tap` recebe a URL explicitamente
(não precisa de um repositório separado `homebrew-dev-cli`):

```bash
brew tap jarede/dev-cli https://github.com/jarede/dev-cli
brew install dev-cli
```

Cobre macOS (Apple Silicon e Intel) e Linux x86_64. Atualizar depois:

```bash
brew update && brew upgrade dev-cli
```

### Windows — Scoop (recomendado)

[Scoop](https://scoop.sh/) é o gerenciador de pacotes mais popular para
instalar CLIs no Windows sem passar por um processo de aprovação central
(diferente do winget/Chocolatey) — por isso o "bucket" também mora neste
repositório, como o tap do Homebrew:

```powershell
scoop bucket add dev-cli https://github.com/jarede/dev-cli
scoop install dev-cli
```

Atualizar depois: `scoop update dev-cli`.

Se preferir não instalar o Scoop, baixe o `.zip` de
`dev-cli-vX.Y.Z-x86_64-pc-windows-msvc.zip` direto na
[página de releases](https://github.com/jarede/dev-cli/releases), extraia e
rode `dev-cli.exe` (ou adicione a pasta ao `PATH`).

### Download direto (qualquer SO)

Baixe o arquivo do seu SO/arquitetura na
[página de releases](https://github.com/jarede/dev-cli/releases) — `.tar.gz`
para macOS/Linux, `.zip` para Windows — extraia e rode o binário. O arquivo
`SHA256SUMS.txt` de cada release traz os checksums para conferência.

### A partir do código-fonte

Requer [Rust](https://rustup.rs/) — toolchain que suporte edition 2024.

```bash
cargo install --git https://github.com/jarede/dev-cli
```

## ✨ Subcomandos

| Comando | O que faz |
|---|---|
| `dev-cli version` | Imprime a versão do `Cargo.toml`. |
| `dev-cli logs stats [container]` | Estatísticas coloridas dos logs por container. Sem argumento, varre todos em `--path` (default `dados/logs`). |
| `dev-cli ai stats opencode [YYYY-MM\|YYYY-MM-DD]` | Dashboard de tokens/custo do OpenCode: heatmap de atividade, streaks, tabela de modelos com barras coloridas, custo em US$/R$ (câmbio ao vivo). Lê o SQLite local do app. |
| `dev-cli ai stats claude [YYYY-MM\|YYYY-MM-DD]` | Horas trabalhadas com o Claude Code por semana/dia/top-N, mais custo estimado por modelo em US$/R$. Lê os transcritos JSONL locais. |

Ambos os subcomandos de `ai stats` aceitam `--json` (saída estruturada em vez do dashboard) e `--no-color`.

## 🚀 Como usar

```bash
cargo build
cargo run -- version
cargo run -- logs stats               # todos os containers
cargo run -- logs stats prezzo        # um container
cargo run -- ai stats opencode        # dashboard de tokens/custo do OpenCode
cargo run -- ai stats claude          # horas + custo do mês atual
cargo run -- ai stats claude 2026-06  # horas + custo de um mês específico
cargo build --release
./target/release/dev-cli --help
```

## 🧪 Testes

```bash
cargo test
```

28 testes unitários, cobrindo o núcleo puro de cada subcomando (sem tocar
banco/disco/rede): `src/logs.rs`, `src/ai/render.rs`, `src/ai/precos.rs` e
`src/ai/cambio.rs`.

## 📁 Estrutura

```
src/
├── main.rs         # entry point: parse do Cli e despacho
├── cli.rs          # struct Cli + enum Commands + VersionArgs
├── logs.rs         # subcomando logs: núcleo puro de contagem + IO + render colorido
└── ai/             # subcomando ai stats
    ├── mod.rs      # AiArgs, dispatch
    ├── stats.rs    # StatsArgs, dispatch por provedor (opencode | claude)
    ├── opencode.rs # IO: lê o SQLite do OpenCode via rusqlite
    ├── claude.rs   # IO: lê os transcritos JSONL via walkdir + serde_json
    ├── precos.rs   # tabela de preços por modelo (custo do claude)
    ├── cambio.rs   # busca a cotação USD/BRL via reqwest (blocking)
    └── render.rs   # núcleo puro compartilhado: heatmap, tabelas, barras, streaks
dados/              # fixtures de log (gitignored)
Formula/dev-cli.rb  # fórmula do Homebrew — regenerada pelo workflow de release
bucket/dev-cli.json # manifest do Scoop (Windows) — regenerado pelo workflow de release
```

## 🧱 Adicionando um subcomando

1. Escrever um `*Args: clap::Args` com um método `execute(&self) -> Result<String, Box<dyn Error>>`.
2. Adicionar a variante no enum `Commands` (em `src/cli.rs`) e o braço no `match`.
3. Se a lógica crescer, extrair para o seu próprio módulo (ex.: `src/logs.rs`).

## 🚀 Publicando uma release

1. Atualizar `version` em `Cargo.toml` (ex: `0.2.0`) e commitar.
2. Criar e empurrar uma tag com o mesmo número, prefixada com `v`:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

3. O workflow `.github/workflows/release.yml` cuida do resto: compila para
   macOS (arm64/x86_64), Linux (x86_64) e Windows (x86_64), publica os
   binários numa [GitHub Release](https://github.com/jarede/dev-cli/releases)
   com checksums (`SHA256SUMS.txt`), e comita a atualização de
   `Formula/dev-cli.rb` e `bucket/dev-cli.json` direto na `main` com a nova
   versão e os hashes corretos — nada manual além dos dois passos acima.

## 🤝 Contribuindo

Veja [`CONTRIBUTING.md`](CONTRIBUTING.md) para como rodar os testes/lints
localmente e as convenções do projeto antes de abrir um PR.

## 📄 Licença

[MIT](LICENSE) © 2026 Jarede Silva.
