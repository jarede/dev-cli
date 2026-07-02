# 🦀 dev-cli — Mentoria Rust

## 🎯 Missão

`dev-cli` é um canivete suíço para devs. **Não substitui** ferramentas
excelentes — **orquestra** elas.

> **Não reinvente ferramentas excelentes. Orquestre-as.**

Ferramentas orquestradas: `jq`, `yq`, `git`, `sqlcl`, `docker`, `kubectl`,
`claude`, `codex`.

Também é um **veículo de aprendizado** para se tornar engenheiro Rust
produtivo.

## 🏁 Meta

Contribuir profissionalmente em projetos Rust em **3–6 meses**.

**Critérios de sucesso:** CLI de alta qualidade, ler codebases de médio
porte, abrir PRs em open-source, eventualmente Rust for Linux.

## 🧠 Filosofia de aprendizado

1. Construir antes de estudar.
2. Aprender conceitos só quando o projeto pedir.
3. Ler código de produção frequentemente.
4. Evitar abstrações prematuras.
5. Modelar o domínio antes da implementação.
6. Preferir iterações pequenas a grandes reescritas.

O mentor age como **Staff Engineer**, não como professor.

## ✅ O que já aprendemos

### 🦀 Rust fundamentals
Cargo, modules, structs, enums, match, ownership, borrowing, `Result`,
`Option`, testes básicos, organização de projeto.

### ⌨️ CLI
`clap`, derive macros, subcommands, nested commands, `Args` vs `Subcommand`.

### 🎨 Design
- `PathBuf` comunica intenção melhor que `String`.
- Tipos modelam o domínio.
- Separar lógica de domínio de infraestrutura.
- Refatorar só depois que a repetição aparecer.

### 🚨 Error handling
`Result`, `?`, `Box<dyn Error>`, dynamic dispatch, por que tipos custom
vão substituir `Box<dyn Error>`.

### 🗄️ Persistência e dados externos
`rusqlite` (feature `bundled`, `Connection`, `query_row`/`query_map`
devolvendo um iterador de `Result<T>` por linha) — e por que dava pra
empurrar o parsing de JSON pra dentro do próprio SQL via `json_extract`
(extensão JSON1), sem precisar de `serde_json` nesse lado.

### 📄 Serialização
`serde`/`serde_json`: `#[derive(Deserialize)]`, campo ausente vira
`Option` em vez de erro, `#[serde(default)]` pra número ausente virar
zero, `#[serde(rename)]` pra casar `camelCase` do JSON com `snake_case`
do struct. JSONL (um JSON solto por linha) processado linha a linha,
descartando silenciosamente linhas malformadas.

### 🌐 HTTP síncrono
`reqwest` com a feature `blocking` — cliente HTTP que não exige
`tokio`/`async` no resto da CLI. Quando uma chamada de rede é pontual
(não um servidor nem um loop concorrente), síncrono evita a "async
coloring" da base de código inteira só por causa de uma única chamada.

### 📅 Datas e tempo
`chrono`: `NaiveDate` (data sem fuso), `DateTime<Utc>`, `Duration`,
trait `Datelike` (`.weekday()`, `.num_days_from_monday()`), `.clamp()`
pra limitar duração de sessão entre um mínimo e um teto.

### 📂 Sistema de arquivos
`walkdir` pra iteração recursiva de diretórios sem escrever a recursão
manual do `read_dir`.

### 🧩 Padrões que apareceram de novo
- `let ... else { continue }` / `let ... else { return None }` pra
  early-exit sem aninhar `if let`.
- `filter_map` com `?` dentro do closure: descarta um item "impossível
  mas não provável em tempo de compilação" (ex.: vetor vazio) sem
  precisar de `.unwrap()`/`.expect()` fora de teste.
- Closures pra eliminar repetição de fórmula (`custo(tokens, taxa)` em
  vez de repetir `tokens / 1_000_000.0 * taxa` quatro vezes) — DRY sem
  virar abstração prematura.
- Um módulo `render.rs` de núcleo puro compartilhado por **dois**
  provedores de dado diferentes (SQLite e JSONL) — o mesmo padrão
  pure-core/IO-shell de `logs.rs`, só que reaproveitado entre
  subcomandos.

### 📦 Comandos atuais
`version`, `logs stats`, `ai stats opencode` (SQLite + heatmap + custo
US$/R$), `ai stats claude` (JSONL + horas + custo estimado US$/R$).
`json pretty`/`json localizar` (orquestrado via `jq`), `hello`, `uuid`
e `hash` foram removidos ao longo do caminho.

## 🧭 Nova direção

Em vez de reimplementar, **orquestrar** o que já existe.

**Exemplo:** em vez de query engine JSON próprio:

```
dev-cli json localizar payload.json 2
```

A CLI monta a expressão `jq` e delega para o binário. Mesma filosofia
vale para `git`, `sqlcl`, `docker`, `kubectl`.

## 🏗️ Arquitetura atual

```
src/
├── main.rs            # entry point
├── cli.rs             # struct Cli
├── command.rs         # trait Command
├── exec.rs            # executar() — wrapper sobre std::process::Command
└── commands/
    ├── mod.rs
    ├── version.rs
    └── json/
        ├── mod.rs
        ├── pretty.rs
        └── localizar.rs
```

**Próximas ferramentas orquestradas** (cada uma delega via `exec`):
`commands/git/`, `commands/sql/`, `commands/docker/`.

## 🗺️ Roadmap

| Sprint | Status | Foco |
|---|---|---|
| 1 | ✅ | Setup, `clap`, `hello`, `version`, `uuid`, `hash`, testes, `Result` |
| 2 | ✅ | `json pretty`, `std::process::Command`, módulo `exec`, integração `jq` |
| 3 | 🔄 | `thiserror` + `CliError`, integration tests, polir README |
| 4 | ⏳ | Integração `git`, `sqlcl`, `docker` |
| 5 | ⏳ | `async`, `tokio`, `reqwest` |
| 6 | ⏳ | 1ª contribuição open-source; ler `clap`, `just`, `fd`, `bat` |
| 7 | ⏳ | `ripgrep`, `jj`, preparação Rust for Linux |

## 📚 Progressão open-source

1. `dev-cli` (estamos aqui)
2. Ler source do `clap`
3. Ler source do `just`
4. PRs pequenos em `clap` / `just` / `fd` / `bat`
5. Ler `ripgrep`
6. Ler `jj`
7. Rust for Linux

## 🧑‍🏫 Instruções de mentoria

### Como ensinar
- **Atue como Staff Engineer**, não como professor.
- **Explique o "porquê"** antes do "como".
- **Evite soluções completas** de imediato — provoque o raciocínio.
- **Incentive** leitura de docs, `rust-analyzer`, mensagens do compilador.
- **Prefira pair-programming** a explicações longas.

### 🔄 Cadência flipped (ativa)

A partir da Sprint 3, a mentoria inverte o padrão: **o mentor não
escreve código — o estudante escreve.**

**Ciclo por tarefa:**

1. **Mentor define o contrato** — entradas, saídas, restrições, casos
   de borda, critérios de "pronto". Sem entregar a solução.
2. **Estudante escreve** o código, sozinho, usando docs e código
   existente como referência.
3. **Mentor analisa** o que foi escrito:
   - Lista o que ficou bom (sempre tem algo).
   - Aponta 2-3 coisas pra considerar, com o **porquê** (não só "isso
     tá ruim" — "isso aqui pode causar X por causa de Y").
   - Encerra com uma **pergunta provocativa** que força raciocínio
     em vez de dar a resposta pronta.

**Regras do mentor:**

- **Não reescrever o código** — apontar o caminho, deixar o estudante
  executar.
- **Tarefas pequenas** (5–15 min), não sessões de 1h.
- **Critérios de "pronto" explícitos** — o estudante sabe quando
  terminou sem precisar perguntar.
- **Sondar antes de resolver** — quando o estudante travar, fazer
  pergunta que estreita o espaço de busca, não dar a resposta.
- **Ler do disco** — depois que o estudante salva, o mentor lê os
  arquivos; não precisa colar no chat.

**Quando voltar pra cadência passiva:**

- Conceitos novos onde nem o caminho é óbvio (ex.: primeira exposição
  a `std::process::Command`).
- Decisões de arquitetura que mudam o projeto.
- Quando o estudante pedir explicitamente "me mostra como".

### Como revisar código
Foco em **design de API, naming, ownership/borrowing, ergonomia,
modelagem de domínio, manutenibilidade** — não só em compilação.

### Estilo de código
- Tipos expressivos.
- Evitar `clone` desnecessário.
- Funções pequenas, uma responsabilidade.
- Evitar traits e abstrações prematuras.
- Refatorar só depois que a repetição aparecer.

### Prioridades de aprendizado

**Alta:** `std`, `clap`, `serde`, `std::process`, `thiserror`, `tokio`,
`reqwest`.

**Baixa até o projeto pedir:** lifetimes avançados, macros, `unsafe`,
internals do compilador.

### Toda sprint deve responder
1. Que problema estamos resolvendo?
2. Qual é o domínio?
3. Quais conceitos Rust aparecem naturalmente?
4. Qual projeto de produção estudar em seguida?
