# `dev-cli ai stats` — design

Data: 2026-07-01

## Contexto e motivação

Dois protótipos em Python já resolvem partes do problema de observar uso de
IA CLIs:

- `opcd-stats.py` — lê o SQLite do OpenCode (`~/.local/share/opencode/opencode.db`)
  e mostra um heatmap de atividade (tokens/dia) + tabela simples de modelos
  usados, com custo em USD já calculado no banco.
- `claude_hours_report.py` — lê os transcritos JSONL do Claude Code
  (`~/.claude/projects/**/*.jsonl`) e estima horas trabalhadas por sessão,
  com tabelas ricas (por semana, por dia, top N dias) e barras coloridas por
  intensidade.

Objetivo: portar os dois para Rust como subcomandos de `dev-cli`, unindo o
melhor de cada um (heatmap do opcd + tabelas/barras do claude_hours),
adicionando um relatório de custo em US$ e R$, e servindo de veículo de
aprendizado (comentários didáticos em cada função/bloco, seguindo a
convenção do projeto).

Vocação de longo prazo (fora de escopo desta iteração): estender para outros
provedores (`codex`, `gemini`, ...) — o design já deixa esse caminho aberto
via novas variantes de enum.

## CLI

Novo grupo `ai`, com a mesma cascata de subcomandos aninhados que `logs`
já usa (`LogsArgs` → `LogsCommands::Stats`):

```
dev-cli ai stats opencode [--db <path>] [--weeks 4..104] [--no-color] [--json]
dev-cli ai stats claude [YYYY-MM] [--top N] [--no-color] [--json]
```

- `AiArgs { comando: AiCommands }` — `AiCommands::Stats(StatsArgs)`.
- `StatsArgs { comando: StatsCommands }` — `StatsCommands::Opencode(OpencodeArgs) | Claude(ClaudeArgs)`.
- Cada variante carrega seus próprios args (diferentes entre si: `opencode`
  usa `--db`/`--weeks`, `claude` usa um argumento posicional de mês e `--top`).
- Adicionar um provedor novo no futuro = uma variante nova em `StatsCommands`
  + um arquivo novo, sem tocar nos existentes.

## Camada de dados (IO)

### `ai/opencode.rs`

- `rusqlite` (feature `bundled`, SQLite compilado junto — sem depender de lib
  do sistema) abre o banco e roda queries **quase idênticas** às do Python,
  inclusive usando `json_extract` direto no SQL (extensão JSON1, incluída no
  `bundled`). Não precisa de `serde_json` neste lado: o parsing do JSON
  aninhado (`data` column) acontece dentro do próprio SQLite.
- Três queries, iguais em espírito às do `opcd-stats.py`: resumo agregado
  (tokens/custo/tasks), série diária (tokens/turns por dia) e agregado por
  modelo (sessões/tokens/custo).
- Erro de banco não encontrado é tratado como erro de usuário claro (igual ao
  padrão de `descobrir_alvos` em `logs.rs`), não um `panic`.

### `ai/claude.rs`

- `walkdir` percorre `~/.claude/projects/**/*.jsonl` recursivamente (evita
  reimplementar a recursão manual do `read_dir`).
- Cada linha é um objeto JSON solto — aqui sim precisamos de `serde` +
  `serde_json` para desserializar em uma struct `RegistroTranscrito` com
  `timestamp`, `session_id` e `message.usage` (tokens de input/output/cache).
  Linhas malformadas ou sem os campos esperados são ignoradas (mesmo
  comportamento do `try/except json.JSONDecodeError` do script original).
- Sessões são agrupadas por `session_id` e viram intervalos de tempo (start/end),
  igual à lógica de `load_sessions` do Python (cap de 4h por sessão, sessão de
  1 mensagem = 5 min).

## Núcleo puro (testável, sem IO)

Vive em `ai/render.rs`, seguindo o padrão de separação de `contar()` em
`logs.rs`: recebe dados já carregados (não lê banco/disco/rede) e devolve
strings/estruturas prontas. Compartilhado pelos dois comandos:

- `calcular_streaks`, `limiares_atividade`, `nivel_atividade`,
  `renderizar_heatmap` — portado do `opcd-stats.py`.
- `agregar_por_dia`, `agregar_por_semana`, `cor_intensidade`,
  `formatar_horas` (`hm`) — portado do `claude_hours_report.py`.
- `numero_compacto` (`1.2K`/`3.4M`) e `renderizar_barra` (barra colorida por
  intensidade) — hoje cada um só existe num script; viram helpers
  compartilhados, usados pelos dois comandos (esse é o "unir o melhor dos
  dois" na prática: `opencode` ganha tabelas/barras bonitas, `claude` ganha
  formatação de número compacto).

Todas essas funções são 100% testáveis com dados inline (`Vec`/`HashMap`
construídos à mão nos testes), sem fixtures de banco ou arquivo.

## Custo em US$ e R$

- **opencode**: a coluna `cost` (USD) já vem calculada no banco — sem
  cálculo adicional.
- **claude**: os transcritos só têm tokens, não custo pronto. `ai/precos.rs`
  guarda uma tabela estática (`match` ou `HashMap` const) de preço por modelo
  (US$/MTok de input, output, cache write, cache read). Modelo ausente da
  tabela não quebra o relatório: a linha é marcada como "custo não estimado".
- **câmbio USD→BRL**: `ai/cambio.rs` usa `reqwest` com a feature `blocking`
  (cliente HTTP síncrono — sem `tokio`, sem mudar a assinatura de nenhum
  `execute()` na CLI) contra `api.frankfurter.dev` (gratuita, sem chave). Se a
  chamada falhar (rede indisponível, timeout, etc.), o comando **não quebra**:
  imprime o relatório só em US$ com um aviso, em vez de propagar erro.

## Layout de arquivos

```
src/
├── ai/
│   ├── mod.rs        # AiArgs, AiCommands::Stats, dispatch
│   ├── stats.rs      # StatsArgs, StatsCommands::{Opencode,Claude}, dispatch
│   ├── opencode.rs   # IO: rusqlite + queries
│   ├── claude.rs     # IO: walkdir + serde_json
│   ├── precos.rs     # tabela estática de preços por modelo (Claude)
│   ├── cambio.rs     # reqwest blocking -> taxa USD/BRL, com fallback gracioso
│   └── render.rs     # núcleo puro compartilhado (heatmap, tabelas, barras, streaks, número compacto)
```

Novas dependências em `Cargo.toml`:

- `rusqlite` (feature `bundled`)
- `reqwest` (features `blocking`, `json`)
- `serde` (feature `derive`) + `serde_json`
- `walkdir`
- `chrono` (aritmética de datas/dias da semana — tanto o cálculo de streak
  quanto o agrupamento por semana fazem bastante disso, e `std` sozinho fica
  verboso)

## Testes

O núcleo puro em `render.rs` ganha testes com dados inline (heatmap,
streaks, cor por intensidade, barra, número compacto), sem tocar
banco/disco/rede — mesmo espírito de `logs.rs::tests`. `opencode.rs`,
`claude.rs` e `cambio.rs` são casca de IO e ficam sem teste automatizado
(mesmo tratamento que `descobrir_alvos` recebe hoje em `logs.rs`).

## Comentários

Por ser projeto de aprendizado, cada função/bloco novo é comentado
explicando o conceito de Rust por trás (não só "o que faz"), com foco nos
pontos novos para o autor: `rusqlite` (`Connection`, `query_map`), `serde`
(`#[derive(Deserialize)]`, campos ausentes viram `Option`), `reqwest::blocking`
(`Result` de chamada de rede), `walkdir` (iterador recursivo).

## Fora de escopo (esta iteração)

- Suporte a outros provedores (`codex`, `gemini`, ...) — arquitetura aberta
  para isso, mas não implementado agora.
- Busca de câmbio assíncrona/`tokio` — usamos `reqwest::blocking` para não
  forçar toda a CLI a virar assíncrona; migração para async fica para o
  Sprint 5 do roadmap (`docs/dev-cli-mentorship.md`), quando fizer sentido
  para outros comandos também.
- `thiserror`/`enum CliError` — mantemos `Box<dyn Error>` como o resto da
  CLI hoje (Sprint 3 do roadmap trata disso globalmente, não só aqui).
