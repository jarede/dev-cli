# `dev-cli ai stats` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Portar `opcd-stats.py` e `claude_hours_report.py` para dois subcomandos Rust (`dev-cli ai stats opencode` e `dev-cli ai stats claude`), unindo o heatmap de tokens do primeiro com as tabelas/barras coloridas do segundo, e adicionando relatório de custo em US$ e R$.

**Architecture:** Núcleo puro compartilhado (`ai/render.rs`) separado da casca de IO por provedor (`ai/opencode.rs` lê SQLite via `rusqlite`, `ai/claude.rs` varre JSONL via `walkdir`+`serde_json`), mais dois módulos de apoio (`ai/cambio.rs` para câmbio USD→BRL via `reqwest::blocking`, `ai/precos.rs` para a tabela de preços por modelo usada no custo do Claude). Mesmo padrão núcleo-puro/casca-de-IO já usado em `logs.rs`.

**Tech Stack:** Rust edition 2024, `clap` (já presente), `owo-colors` (já presente), `chrono`, `rusqlite` (feature `bundled`), `reqwest` (features `blocking`, `json`), `serde`+`serde_json`, `walkdir`.

## Global Constraints

- Edition 2024, `cargo build`, `cargo test` e `cargo clippy -- -D warnings` devem passar sem warnings em cada tarefa.
- Português (pt-br) em struct/função/variável; inglês só em nomes de crate/trait externos e no nome público do subcomando (`ai`, `stats`, `opencode`, `claude`).
- **Sem `unwrap()` nem `expect()` fora de `#[cfg(test)]`** — usar `?`, `if let`/let-chains, ou `filter_map`/`Option::?` dentro de closures para descartar graciosamente casos impossíveis-mas-não-comprováveis-pelo-compilador.
- Usar let chains (edition 2024) em vez de `if` aninhado onde o clippy pedir (`collapsible_if`).
- Tipo de erro: `Box<dyn std::error::Error>`, igual ao resto da CLI (sem `thiserror` ainda).
- Comentar cada função/bloco novo explicando o conceito de Rust por trás (não só "o que faz") — projeto de aprendizado. Foco extra nos pontos novos: `rusqlite` (`Connection`, `query_map`), `serde` (`#[derive(Deserialize)]`, campos ausentes viram `Option`), `reqwest::blocking` (chamada de rede síncrona), `walkdir` (iterador recursivo), `chrono` (`NaiveDate`, `Duration`, `Weekday`).
- **Commits autorizados a cada tarefa concluída** (aprovação explícita do usuário para este plano — exceção pontual à regra geral do `CLAUDE.md` de só commitar sob pedido).
- Núcleo puro (`ai/render.rs`, `ai/precos.rs`, a função pura de `ai/cambio.rs`) é 100% testável com dados inline, sem tocar banco/disco/rede — mesmo padrão de `contar()` em `logs.rs`. Camada de IO (`ai/opencode.rs`, `ai/claude.rs`, a chamada de rede em `ai/cambio.rs`) não tem teste automatizado, mesmo tratamento que `descobrir_alvos` recebe hoje em `logs.rs`.
- Preços por modelo em `ai/precos.rs` são um valor conhecido no momento da escrita deste plano (Opus $15/$75, Sonnet $3/$15, Haiku $0.80/$4 por MTok de entrada/saída) — podem estar desatualizados quando a tarefa rodar; comentário no código deve avisar disso.

---

### Task 1: Scaffolding do comando `ai stats` (CLI, sem lógica ainda)

**Files:**
- Create: `src/ai/mod.rs`
- Create: `src/ai/stats.rs`
- Create: `src/ai/opencode.rs`
- Create: `src/ai/claude.rs`
- Modify: `src/main.rs` (adicionar `mod ai;`)
- Modify: `src/cli.rs` (adicionar variante `Ai(AiArgs)`)

**Interfaces:**
- Produces: `ai::AiArgs::execute(&self) -> Result<String, Box<dyn std::error::Error>>`; `ai::stats::StatsArgs` com subcomando `StatsCommands::{Opencode(OpencodeArgs), Claude(ClaudeArgs)}`; `OpencodeArgs::execute` e `ClaudeArgs::execute` (stubs por enquanto).

- [ ] **Step 1: Criar `src/ai/opencode.rs` com args e stub**

```rust
// Subcomando `ai stats opencode`: dashboard de tokens/custo do OpenCode,
// lido direto do SQLite local do app (~/.local/share/opencode/opencode.db).
use std::path::PathBuf;

use clap::Args;

#[derive(Args, Debug)]
pub struct OpencodeArgs {
    /// Caminho do banco SQLite do OpenCode. Se omitido, usa
    /// `~/.local/share/opencode/opencode.db`.
    #[arg(long)]
    db: Option<PathBuf>,

    /// Largura do heatmap em semanas.
    #[arg(long, default_value_t = 52, value_parser = clap::value_parser!(u32).range(4..=104))]
    weeks: u32,

    /// Desativa cores ANSI na saída (além da detecção automática de terminal).
    #[arg(long)]
    no_color: bool,

    /// Imprime os dados computados como JSON em vez do dashboard.
    #[arg(long)]
    json: bool,
}

impl OpencodeArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Placeholder: preenchido nas Tasks 5, 6 e 9. Por ora só garante que
        // o parsing de argumentos e o dispatch do clap funcionam.
        Ok("ai stats opencode: em construção".to_string())
    }
}
```

- [ ] **Step 2: Criar `src/ai/claude.rs` com args e stub**

```rust
// Subcomando `ai stats claude`: horas trabalhadas + custo estimado a partir
// dos transcritos locais do Claude Code (~/.claude/projects/**/*.jsonl).
use clap::Args;

#[derive(Args, Debug)]
pub struct ClaudeArgs {
    /// Mês no formato YYYY-MM; se omitido, usa o mês atual.
    mes: Option<String>,

    /// Quantidade de dias no ranking de mais intensos.
    #[arg(long, short, default_value_t = 5)]
    top: usize,

    /// Desativa cores ANSI na saída.
    #[arg(long)]
    no_color: bool,

    /// Imprime os dados computados como JSON em vez das tabelas.
    #[arg(long)]
    json: bool,
}

impl ClaudeArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Placeholder: preenchido nas Tasks 7, 8 e 9.
        Ok("ai stats claude: em construção".to_string())
    }
}
```

- [ ] **Step 3: Criar `src/ai/stats.rs`**

```rust
// Subcomando `ai stats`: apenas encaminha para o provedor escolhido. Mesmo
// padrão de encapsulamento de `LogsArgs`/`LogsCommands` em `src/logs.rs`.
use clap::Args;
use clap::Subcommand;

use crate::ai::claude::ClaudeArgs;
use crate::ai::opencode::OpencodeArgs;

#[derive(Args, Debug)]
pub struct StatsArgs {
    #[command(subcommand)]
    comando: StatsCommands,
}

impl StatsArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        match &self.comando {
            StatsCommands::Opencode(args) => args.execute(),
            StatsCommands::Claude(args) => args.execute(),
        }
    }
}

// Cada variante nova aqui = um provedor novo (ex: `codex`, `gemini`), sem
// tocar nas existentes — é o ponto de extensão que o design previu.
#[derive(Subcommand, Debug)]
enum StatsCommands {
    Opencode(OpencodeArgs),
    Claude(ClaudeArgs),
}
```

- [ ] **Step 4: Criar `src/ai/mod.rs`**

```rust
// Grupo de subcomandos `ai`. Hoje só tem `stats`, mas a estrutura já
// comporta crescer (ex: `ai chat`) sem precisar migrar nada.
use clap::Args;
use clap::Subcommand;

use crate::ai::stats::StatsArgs;

mod claude;
mod opencode;
pub mod stats;

#[derive(Args, Debug)]
pub struct AiArgs {
    #[command(subcommand)]
    comando: AiCommands,
}

impl AiArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        match &self.comando {
            AiCommands::Stats(args) => args.execute(),
        }
    }
}

#[derive(Subcommand, Debug)]
enum AiCommands {
    Stats(StatsArgs),
}
```

- [ ] **Step 5: Declarar o módulo em `src/main.rs`**

Em `src/main.rs`, junto de `mod cli;` e `mod logs;`, adicionar:

```rust
mod ai;
```

- [ ] **Step 6: Adicionar a variante `Ai` em `src/cli.rs`**

```rust
use crate::ai::AiArgs;
use crate::logs::LogsArgs;
```

```rust
#[derive(Subcommand, Debug)]
pub enum Commands {
    Version(VersionArgs),
    Logs(LogsArgs),
    Ai(AiArgs),
}
```

```rust
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        match self {
            Commands::Version(args) => args.execute(),
            Commands::Logs(args) => args.execute(),
            Commands::Ai(args) => args.execute(),
        }
    }
```

- [ ] **Step 7: Verificar que compila e o CLI está correto**

Run: `cargo build`
Expected: compila sem erros.

Run: `cargo run -- ai stats opencode --help`
Expected: mostra `--db`, `--weeks`, `--no-color`, `--json`.

Run: `cargo run -- ai stats claude --help`
Expected: mostra o argumento posicional de mês, `--top`/`-t`, `--no-color`, `--json`.

Run: `cargo run -- ai stats opencode`
Expected: imprime `ai stats opencode: em construção`.

- [ ] **Step 8: Commit**

```bash
git add src/ai src/main.rs src/cli.rs
git commit -m "feat(ai): adiciona scaffolding do comando ai stats"
```

---

### Task 2: Núcleo puro — números compactos, horas e barras coloridas

**Files:**
- Create: `src/ai/render.rs`
- Modify: `src/ai/mod.rs` (declarar `pub mod render;`)

**Interfaces:**
- Produces: `render::numero_compacto(f64) -> String`; `render::formatar_horas(f64) -> String`; `render::nivel_intensidade(f64, f64) -> u8`; `render::renderizar_barra(f64, f64, usize, bool) -> String`.

- [ ] **Step 1: Escrever os testes que falham**

```rust
// src/ai/render.rs
// Núcleo puro compartilhado pelos dois provedores: heatmap, tabelas,
// barras coloridas, streaks. Nenhuma função aqui toca banco, disco ou rede
// — só transforma dados já carregados em texto pronto pra imprimir. Mesmo
// espírito de `contar()` em `src/logs.rs`.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numero_compacto_formata_por_ordem_de_grandeza() {
        assert_eq!(numero_compacto(999.0), "999");
        assert_eq!(numero_compacto(1_500.0), "1.5K");
        assert_eq!(numero_compacto(2_300_000.0), "2.3M");
        assert_eq!(numero_compacto(1_200_000_000.0), "1.2B");
    }

    #[test]
    fn formatar_horas_converte_fracao_em_horas_e_minutos() {
        assert_eq!(formatar_horas(1.5), "1h30m");
        assert_eq!(formatar_horas(5.0 / 60.0), "0h05m");
        assert_eq!(formatar_horas(4.0), "4h00m");
    }

    #[test]
    fn nivel_intensidade_distribui_em_seis_niveis() {
        assert_eq!(nivel_intensidade(0.0, 100.0), 0);
        assert_eq!(nivel_intensidade(20.0, 100.0), 1);
        assert_eq!(nivel_intensidade(50.0, 100.0), 3);
        assert_eq!(nivel_intensidade(80.0, 100.0), 4);
        assert_eq!(nivel_intensidade(100.0, 100.0), 5);
    }

    #[test]
    fn nivel_intensidade_sem_maximo_e_sempre_zero() {
        assert_eq!(nivel_intensidade(10.0, 0.0), 0);
    }

    #[test]
    fn renderizar_barra_sem_cor_devolve_so_os_blocos() {
        assert_eq!(renderizar_barra(50.0, 100.0, 20, false), "██████████");
        assert_eq!(renderizar_barra(0.0, 100.0, 20, false), "");
    }

    #[test]
    fn renderizar_barra_com_cor_envolve_em_escape_ansi() {
        let barra = renderizar_barra(50.0, 100.0, 20, true);
        assert!(barra.starts_with("\u{1b}["), "esperava escape ANSI, veio: {barra:?}");
        assert!(barra.contains('█'));
    }
}
```

- [ ] **Step 2: Rodar e confirmar que falha**

Run: `cargo test render:: 2>&1 | head -30`
Expected: FAIL com `cannot find function 'numero_compacto'` (e as demais).

- [ ] **Step 3: Implementar**

```rust
use owo_colors::OwoColorize;

// `valor as i64` trunca a parte fracionária: como só entramos aqui abaixo
// de 1000, não perdemos nada que importe pro relatório.
pub fn numero_compacto(valor: f64) -> String {
    if valor >= 1_000_000_000.0 {
        format!("{:.1}B", valor / 1_000_000_000.0)
    } else if valor >= 1_000_000.0 {
        format!("{:.1}M", valor / 1_000_000.0)
    } else if valor >= 1_000.0 {
        format!("{:.1}K", valor / 1_000.0)
    } else {
        format!("{}", valor as i64)
    }
}

pub fn formatar_horas(horas: f64) -> String {
    // `.round()` evita que erros de ponto flutuante (ex: 1.4999999999)
    // arredondem minutos pra baixo por acidente.
    let minutos_totais = (horas * 60.0).round() as i64;
    format!("{}h{:02}m", minutos_totais / 60, minutos_totais % 60)
}

// Paleta de 6 níveis (0 = sem atividade .. 5 = pico), espelha o `DAY_COLORS`
// do protótipo Python. Separado da cor em si pra ficar testável sem
// depender de código de terminal.
pub fn nivel_intensidade(valor: f64, maximo: f64) -> u8 {
    if maximo <= 0.0 {
        return 0;
    }
    let proporcao = valor / maximo;
    // `+ 0.5` antes de truncar arredonda pro nível mais próximo em vez de
    // sempre truncar pra baixo.
    let indice = (proporcao * 5.0 + 0.5) as i64;
    indice.clamp(0, 5) as u8
}

// `OwoColorize` (trait de extensão) dá o método `.truecolor(r, g, b)` (cor
// RGB arbitrária) e `.cyan()`/`.yellow()`/etc a qualquer `Display`. Cada
// braço do `match` devolve um tipo diferente por baixo do capô, por isso
// convertemos pra `String` já dentro do braço (mesmo truque de
// `colorir_nivel` em `src/logs.rs`).
fn aplicar_cor(nivel: u8, texto: &str) -> String {
    match nivel {
        0 => texto.truecolor(128, 128, 128).to_string(),
        1 => texto.cyan().to_string(),
        2 => texto.green().to_string(),
        3 => texto.yellow().to_string(),
        4 => texto.truecolor(255, 140, 0).to_string(),
        _ => texto.red().to_string(),
    }
}

pub fn renderizar_barra(valor: f64, maximo: f64, largura_max: usize, cores: bool) -> String {
    let comprimento = if maximo > 0.0 {
        ((valor / maximo) * largura_max as f64).round() as usize
    } else {
        0
    };
    let barra = "█".repeat(comprimento.min(largura_max));
    if cores {
        aplicar_cor(nivel_intensidade(valor, maximo), &barra)
    } else {
        barra
    }
}
```

- [ ] **Step 4: Rodar e confirmar que passa**

Run: `cargo test render::`
Expected: PASS (6 testes).

- [ ] **Step 5: Declarar o módulo em `src/ai/mod.rs`**

Adicionar `pub mod render;` junto aos outros `mod` de `src/ai/mod.rs`.

Run: `cargo build`
Expected: compila (o módulo ainda não é usado fora dos testes, então pode gerar warning de `dead_code` — aceitável nesta tarefa, some quando `opencode.rs`/`claude.rs` passarem a chamar essas funções nas Tasks 5–8).

- [ ] **Step 6: Commit**

```bash
git add src/ai/render.rs src/ai/mod.rs
git commit -m "feat(ai): nucleo puro de numeros, horas e barras coloridas"
```

---

### Task 3: Núcleo puro — heatmap e streaks

**Files:**
- Modify: `src/ai/render.rs`
- Modify: `Cargo.toml` (adicionar `chrono`)

**Interfaces:**
- Consumes: nada de tasks anteriores (funções novas, independentes).
- Produces: `render::Streaks { atual: u32, recorde: u32 }`; `render::calcular_streaks(&BTreeSet<NaiveDate>, NaiveDate) -> Streaks`; `render::limiares_atividade(&BTreeMap<NaiveDate, i64>) -> [i64; 3]`; `render::nivel_atividade(Option<i64>, &[i64; 3]) -> u8`; `render::renderizar_heatmap(&BTreeMap<NaiveDate, i64>, u32, NaiveDate, bool) -> Vec<String>`.

- [ ] **Step 1: Adicionar a dependência**

Em `Cargo.toml`, na seção `[dependencies]`:

```toml
chrono = "0.4"
```

- [ ] **Step 2: Escrever os testes que falham**

Adicionar ao final de `src/ai/render.rs` (fora do `mod tests`, os `use` no topo do arquivo):

```rust
use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
```

E dentro de `mod tests`:

```rust
    fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        // `expect` aqui é seguro: as datas nos testes são sempre válidas,
        // escritas à mão — não é entrada externa que possa falhar.
        NaiveDate::from_ymd_opt(ano, mes, dia).expect("data de teste válida")
    }

    #[test]
    fn calcular_streaks_conta_sequencia_atual_e_recorde() {
        let dias: BTreeSet<NaiveDate> = [
            data(2026, 6, 28),
            data(2026, 6, 29),
            data(2026, 6, 30),
            data(2026, 7, 2), // quebra a sequência (pulou dia 1º)
        ]
        .into_iter()
        .collect();

        let streaks = calcular_streaks(&dias, data(2026, 7, 2));
        assert_eq!(streaks.recorde, 3); // 28, 29, 30
        assert_eq!(streaks.atual, 1); // só o dia 2 é contíguo até "hoje"
    }

    #[test]
    fn calcular_streaks_sem_dias_ativos_e_zero() {
        let dias: BTreeSet<NaiveDate> = BTreeSet::new();
        let streaks = calcular_streaks(&dias, data(2026, 7, 2));
        assert_eq!(streaks, Streaks::default());
    }

    #[test]
    fn limiares_atividade_ignora_dias_zerados() {
        let tokens: BTreeMap<NaiveDate, i64> = [
            (data(2026, 6, 1), 0),
            (data(2026, 6, 2), 10),
            (data(2026, 6, 3), 20),
            (data(2026, 6, 4), 30),
            (data(2026, 6, 5), 40),
        ]
        .into_iter()
        .collect();

        let limiares = limiares_atividade(&tokens);
        assert_eq!(limiares, [10, 20, 30]);
    }

    #[test]
    fn nivel_atividade_classifica_pelos_limiares() {
        let limiares = [10, 20, 30];
        assert_eq!(nivel_atividade(None, &limiares), 0);
        assert_eq!(nivel_atividade(Some(0), &limiares), 1);
        assert_eq!(nivel_atividade(Some(10), &limiares), 1);
        assert_eq!(nivel_atividade(Some(20), &limiares), 2);
        assert_eq!(nivel_atividade(Some(30), &limiares), 3);
        assert_eq!(nivel_atividade(Some(31), &limiares), 4);
    }

    #[test]
    fn renderizar_heatmap_tem_uma_linha_por_dia_da_semana_mais_cabecalho_e_legenda() {
        let tokens: BTreeMap<NaiveDate, i64> = [(data(2026, 7, 1), 100)].into_iter().collect();
        let linhas = renderizar_heatmap(&tokens, 4, data(2026, 7, 1), false);
        // 1 linha de meses + 7 linhas de dias da semana + 1 linha de legenda.
        assert_eq!(linhas.len(), 9);
        assert!(linhas.last().expect("legenda sempre é a última linha").contains("Menos"));
    }
```

- [ ] **Step 3: Rodar e confirmar que falha**

Run: `cargo test render:: 2>&1 | head -40`
Expected: FAIL — `cannot find type 'Streaks'`, `cannot find function 'calcular_streaks'` etc.

- [ ] **Step 4: Implementar**

```rust
use chrono::{Datelike, Duration};

// `Copy`: a struct é só dois `u32`, mais barato copiar do que emprestar.
// `Default`: dá `Streaks::default()` com os dois campos zerados — usado
// tanto no teste "sem dias ativos" quanto como valor inicial do cálculo.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Streaks {
    pub atual: u32,
    pub recorde: u32,
}

pub fn calcular_streaks(dias_ativos: &BTreeSet<NaiveDate>, hoje: NaiveDate) -> Streaks {
    let mut recorde = 0u32;
    let mut sequencia = 0u32;
    let mut anterior: Option<NaiveDate> = None;

    // `BTreeSet` já itera em ordem crescente — não precisamos ordenar.
    for &dia in dias_ativos {
        match anterior {
            Some(dia_anterior) if dia == dia_anterior + Duration::days(1) => sequencia += 1,
            _ => sequencia = 1,
        }
        recorde = recorde.max(sequencia);
        anterior = Some(dia);
    }

    // Sequência atual: anda pra trás a partir de "hoje" enquanto o dia
    // estiver no conjunto.
    let mut atual = 0u32;
    let mut cursor = hoje;
    while dias_ativos.contains(&cursor) {
        atual += 1;
        cursor -= Duration::days(1);
    }

    Streaks { atual, recorde }
}

pub fn limiares_atividade(tokens_por_dia: &BTreeMap<NaiveDate, i64>) -> [i64; 3] {
    let mut valores: Vec<i64> = tokens_por_dia
        .values()
        .copied()
        .filter(|&tokens| tokens > 0)
        .collect();
    valores.sort_unstable();

    if valores.is_empty() {
        return [0, 0, 0];
    }

    let n = valores.len();
    // Mesmos percentis (25/50/75) do protótipo Python, aplicados sobre os
    // dias com atividade real.
    let indice_percentil = |p: f64| valores[(((n - 1) as f64) * p) as usize];
    [
        indice_percentil(0.25),
        indice_percentil(0.50),
        indice_percentil(0.75),
    ]
}

pub fn nivel_atividade(tokens: Option<i64>, limiares: &[i64; 3]) -> u8 {
    // `match` com guardas (`if tokens <= ...`): cada braço testa uma faixa,
    // na ordem, até achar a primeira que bate.
    match tokens {
        None => 0,
        Some(tokens) if tokens <= 0 => 1,
        Some(tokens) if tokens <= limiares[0] => 1,
        Some(tokens) if tokens <= limiares[1] => 2,
        Some(tokens) if tokens <= limiares[2] => 3,
        Some(_) => 4,
    }
}

// Meses abreviados em pt-br, indexados por `NaiveDate::month0()` (0 = jan).
const MESES: [&str; 12] = [
    "Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez",
];

fn domingo_da_semana(dia: NaiveDate) -> NaiveDate {
    // `num_days_from_sunday`: domingo=0 .. sábado=6 — exatamente quanto
    // subtrair pra voltar ao domingo daquela semana.
    dia - Duration::days(dia.weekday().num_days_from_sunday() as i64)
}

fn linha_dos_meses(primeiro_domingo: NaiveDate, semanas: u32) -> String {
    let mut celulas = vec![' '; semanas as usize];
    let mut mes_anterior: Option<u32> = None;

    for semana in 0..semanas {
        let dia = primeiro_domingo + Duration::weeks(semana as i64);
        if Some(dia.month()) != mes_anterior {
            let rotulo = MESES[dia.month0() as usize];
            for (deslocamento, letra) in rotulo.chars().enumerate() {
                let coluna = semana as usize + deslocamento;
                if coluna < semanas as usize {
                    celulas[coluna] = letra;
                }
            }
            mes_anterior = Some(dia.month());
        }
    }

    celulas.into_iter().collect()
}

fn celula_heatmap(nivel: u8, cores: bool) -> String {
    if cores {
        // Paleta verde crescente (5 tons), independente da paleta de barras
        // de `nivel_intensidade` — heatmap e barras representam conceitos
        // diferentes (atividade por dia vs. horas por dia).
        const PALETA: [(u8, u8, u8); 5] = [
            (88, 88, 88),
            (30, 90, 40),
            (40, 120, 60),
            (50, 160, 80),
            (60, 210, 110),
        ];
        let (r, g, b) = PALETA[nivel as usize];
        "■".truecolor(r, g, b).to_string()
    } else {
        ["□", "░", "▒", "▓", "█"][nivel as usize].to_string()
    }
}

pub fn renderizar_heatmap(
    tokens_por_dia: &BTreeMap<NaiveDate, i64>,
    semanas: u32,
    hoje: NaiveDate,
    cores: bool,
) -> Vec<String> {
    let domingo_atual = domingo_da_semana(hoje);
    let primeiro_domingo = domingo_atual - Duration::weeks((semanas - 1) as i64);
    let limiares = limiares_atividade(tokens_por_dia);

    let mut linhas = vec![format!("      {}", linha_dos_meses(primeiro_domingo, semanas))];

    // Domingo=0 .. sábado=6, igual ao `weekday()` do chrono com
    // `num_days_from_sunday`.
    let rotulos_dias = ["   ", "Seg", "   ", "Qua", "   ", "Sex", "   "];
    for deslocamento_dia in 0..7u32 {
        let mut linha = format!("  {} ", rotulos_dias[deslocamento_dia as usize]);
        for semana in 0..semanas {
            let dia = primeiro_domingo
                + Duration::weeks(semana as i64)
                + Duration::days(deslocamento_dia as i64);
            if dia > hoje {
                linha.push(' ');
                continue;
            }
            let nivel = nivel_atividade(tokens_por_dia.get(&dia).copied(), &limiares);
            linha.push_str(&celula_heatmap(nivel, cores));
        }
        linhas.push(linha);
    }

    let legenda: String = (0..5).map(|nivel| celula_heatmap(nivel, cores)).collect();
    linhas.push(format!("      Menos {legenda} Mais"));
    linhas
}
```

- [ ] **Step 5: Rodar e confirmar que passa**

Run: `cargo test render::`
Expected: PASS (11 testes no total incluindo os da Task 2).

- [ ] **Step 6: Commit**

```bash
git add src/ai/render.rs Cargo.toml Cargo.lock
git commit -m "feat(ai): nucleo puro do heatmap e calculo de streaks"
```

---

### Task 4: Núcleo puro — agregação de sessões por dia/semana

**Files:**
- Modify: `src/ai/render.rs`

**Interfaces:**
- Produces: `render::Sessao { dia: NaiveDate, duracao_horas: f64 }`; `render::agregar_por_dia(&[Sessao]) -> BTreeMap<NaiveDate, (f64, u32)>`; `render::agregar_por_semana(&[Sessao]) -> BTreeMap<NaiveDate, (f64, u32, BTreeSet<NaiveDate>)>`.
- Consumes: nada de tasks anteriores diretamente, mas `Sessao` é o tipo que `ai/claude.rs` vai produzir na Task 7 — o nome e os campos aqui são o contrato pra aquela tarefa.

- [ ] **Step 1: Escrever os testes que falham**

```rust
    #[test]
    fn agregar_por_dia_soma_horas_e_conta_sessoes() {
        let sessoes = vec![
            Sessao { dia: data(2026, 6, 1), duracao_horas: 1.0 },
            Sessao { dia: data(2026, 6, 1), duracao_horas: 2.5 },
            Sessao { dia: data(2026, 6, 2), duracao_horas: 0.5 },
        ];

        let por_dia = agregar_por_dia(&sessoes);
        assert_eq!(por_dia.get(&data(2026, 6, 1)), Some(&(3.5, 2)));
        assert_eq!(por_dia.get(&data(2026, 6, 2)), Some(&(0.5, 1)));
        assert_eq!(por_dia.len(), 2);
    }

    #[test]
    fn agregar_por_semana_agrupa_pela_segunda_feira() {
        // 2026-06-01 é segunda; 2026-06-05 é sexta da mesma semana.
        let sessoes = vec![
            Sessao { dia: data(2026, 6, 1), duracao_horas: 1.0 },
            Sessao { dia: data(2026, 6, 5), duracao_horas: 2.0 },
            Sessao { dia: data(2026, 6, 8), duracao_horas: 4.0 }, // semana seguinte
        ];

        let por_semana = agregar_por_semana(&sessoes);
        assert_eq!(por_semana.len(), 2);

        let semana1 = &por_semana[&data(2026, 6, 1)];
        assert_eq!(semana1.0, 3.0); // horas
        assert_eq!(semana1.1, 2); // sessões
        assert_eq!(semana1.2.len(), 2); // dias distintos

        let semana2 = &por_semana[&data(2026, 6, 8)];
        assert_eq!(semana2.0, 4.0);
    }
```

- [ ] **Step 2: Rodar e confirmar que falha**

Run: `cargo test render:: 2>&1 | head -30`
Expected: FAIL — `cannot find struct 'Sessao'`, `cannot find function 'agregar_por_dia'`.

- [ ] **Step 3: Implementar**

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Sessao {
    pub dia: NaiveDate,
    pub duracao_horas: f64,
}

// Mapa dia -> (soma de horas, quantidade de sessões).
pub fn agregar_por_dia(sessoes: &[Sessao]) -> BTreeMap<NaiveDate, (f64, u32)> {
    let mut mapa: BTreeMap<NaiveDate, (f64, u32)> = BTreeMap::new();
    for sessao in sessoes {
        // `entry(...).or_insert((0.0, 0))`: pega a entrada existente ou
        // cria zerada, sem precisar de `if contains_key`.
        let entrada = mapa.entry(sessao.dia).or_insert((0.0, 0));
        entrada.0 += sessao.duracao_horas;
        entrada.1 += 1;
    }
    mapa
}

// Mapa segunda-feira-da-semana -> (soma de horas, quantidade de sessões,
// conjunto de dias distintos com atividade naquela semana).
pub fn agregar_por_semana(
    sessoes: &[Sessao],
) -> BTreeMap<NaiveDate, (f64, u32, BTreeSet<NaiveDate>)> {
    let mut mapa: BTreeMap<NaiveDate, (f64, u32, BTreeSet<NaiveDate>)> = BTreeMap::new();
    for sessao in sessoes {
        // `num_days_from_monday`: segunda=0 .. domingo=6.
        let segunda =
            sessao.dia - Duration::days(sessao.dia.weekday().num_days_from_monday() as i64);
        let entrada = mapa.entry(segunda).or_insert((0.0, 0, BTreeSet::new()));
        entrada.0 += sessao.duracao_horas;
        entrada.1 += 1;
        entrada.2.insert(sessao.dia);
    }
    mapa
}
```

- [ ] **Step 4: Rodar e confirmar que passa**

Run: `cargo test render::`
Expected: PASS (13 testes no total).

- [ ] **Step 5: Commit**

```bash
git add src/ai/render.rs
git commit -m "feat(ai): nucleo puro de agregacao de sessoes por dia e semana"
```

---

### Task 5: `ai stats opencode` — leitura do SQLite e dashboard em US$

**Files:**
- Modify: `src/ai/opencode.rs`
- Modify: `Cargo.toml` (adicionar `rusqlite`)

**Interfaces:**
- Consumes: `render::{numero_compacto, renderizar_barra, calcular_streaks, renderizar_heatmap, Streaks}` (Tasks 2–3).
- Produces: `OpencodeArgs::execute` funcional (sem conversão pra R$, isso é a Task 6).

- [ ] **Step 1: Adicionar a dependência**

Em `Cargo.toml`:

```toml
rusqlite = { version = "0.40", features = ["bundled"] }
```

- [ ] **Step 2: Implementar a camada de IO em `src/ai/opencode.rs`**

Substituir o conteúdo do arquivo por:

```rust
// Subcomando `ai stats opencode`: dashboard de tokens/custo do OpenCode,
// lido direto do SQLite local do app (~/.local/share/opencode/opencode.db).
//
// Mesma separação de responsabilidades de `src/logs.rs`: aqui só tem
// CASCA DE IO (abrir banco, rodar query, montar texto). O cálculo em si
// (heatmap, streaks, barras) mora no núcleo puro de `src/ai/render.rs`.
use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use clap::Args;
use owo_colors::OwoColorize;
use rusqlite::Connection;

use crate::ai::render;

#[derive(Args, Debug)]
pub struct OpencodeArgs {
    /// Caminho do banco SQLite do OpenCode. Se omitido, usa
    /// `~/.local/share/opencode/opencode.db`.
    #[arg(long)]
    db: Option<PathBuf>,

    /// Largura do heatmap em semanas.
    #[arg(long, default_value_t = 52, value_parser = clap::value_parser!(u32).range(4..=104))]
    weeks: u32,

    /// Desativa cores ANSI na saída.
    #[arg(long)]
    no_color: bool,

    /// Imprime os dados computados como JSON em vez do dashboard.
    #[arg(long)]
    json: bool,
}

// Resumo agregado de todo o histórico (não filtrado por período).
struct Resumo {
    tarefas: i64,
    tokens_totais: i64,
    custo_usd: f64,
}

// Uma linha da tabela "modelos usados".
struct LinhaModelo {
    modelo: String,
    provedor: String,
    sessoes: i64,
    tokens: i64,
    custo_usd: f64,
}

fn caminho_padrao_db() -> PathBuf {
    // `HOME` não definido é um cenário exótico o bastante (shell quebrado)
    // pra só cair num caminho relativo em vez de propagar erro aqui.
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share/opencode/opencode.db")
}

fn carregar_resumo(conn: &Connection) -> rusqlite::Result<Resumo> {
    // `json_extract` roda dentro do próprio SQLite (extensão JSON1, incluída
    // no feature `bundled`) — não precisamos de `serde_json` neste lado,
    // igual ao protótipo Python.
    conn.query_row(
        "SELECT
            COUNT(*) AS tarefas,
            COALESCE(SUM(
                COALESCE(CAST(json_extract(data, '$.tokens.input') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.output') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.reasoning') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.cache.read') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.cache.write') AS INTEGER), 0)
            ), 0) AS tokens_totais,
            COALESCE(SUM(COALESCE(CAST(json_extract(data, '$.cost') AS REAL), 0)), 0) AS custo_usd
         FROM message
         WHERE json_extract(data, '$.role') = 'assistant'",
        [],
        |linha| {
            Ok(Resumo {
                tarefas: linha.get(0)?,
                tokens_totais: linha.get(1)?,
                custo_usd: linha.get(2)?,
            })
        },
    )
}

fn carregar_tokens_por_dia(conn: &Connection) -> rusqlite::Result<BTreeMap<NaiveDate, i64>> {
    let mut stmt = conn.prepare(
        "SELECT
            date(CAST(json_extract(data, '$.time.created') AS INTEGER) / 1000, 'unixepoch', 'localtime') AS dia,
            SUM(
                COALESCE(CAST(json_extract(data, '$.tokens.input') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.output') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.reasoning') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.cache.read') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.cache.write') AS INTEGER), 0)
            ) AS tokens
         FROM message
         WHERE json_extract(data, '$.role') = 'assistant'
         GROUP BY dia",
    )?;

    // `query_map` devolve um iterador de `Result<T>`: cada linha pode
    // falhar (tipo errado, coluna nula onde não esperava) independente das
    // outras.
    let linhas = stmt.query_map([], |linha| {
        let dia_texto: String = linha.get(0)?;
        let tokens: i64 = linha.get(1)?;
        Ok((dia_texto, tokens))
    })?;

    let mut mapa = BTreeMap::new();
    for linha in linhas {
        let (dia_texto, tokens) = linha?;
        // Data malformada (não deveria acontecer, mas SQLite não garante
        // isso em tempo de compilação) é descartada em vez de derrubar o
        // comando inteiro.
        if let Ok(dia) = NaiveDate::parse_from_str(&dia_texto, "%Y-%m-%d") {
            mapa.insert(dia, tokens);
        }
    }
    Ok(mapa)
}

fn carregar_modelos(conn: &Connection) -> rusqlite::Result<Vec<LinhaModelo>> {
    let mut stmt = conn.prepare(
        "SELECT
            json_extract(model, '$.id') AS modelo,
            COALESCE(json_extract(model, '$.providerID'), 'desconhecido') AS provedor,
            COUNT(*) AS sessoes,
            SUM(tokens_input + tokens_output + tokens_reasoning + tokens_cache_read + tokens_cache_write) AS tokens,
            SUM(cost) AS custo_usd
         FROM session
         WHERE model IS NOT NULL
         GROUP BY modelo, provedor
         ORDER BY sessoes DESC",
    )?;

    let linhas = stmt.query_map([], |linha| {
        Ok(LinhaModelo {
            modelo: linha.get(0)?,
            provedor: linha.get(1)?,
            sessoes: linha.get(2)?,
            tokens: linha.get(3)?,
            custo_usd: linha.get(4)?,
        })
    })?;

    linhas.collect()
}

impl OpencodeArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        let caminho_db = self.db.clone().unwrap_or_else(caminho_padrao_db);
        if !caminho_db.exists() {
            return Err(format!(
                "banco do OpenCode não encontrado: '{}'",
                caminho_db.display()
            )
            .into());
        }

        let conn = Connection::open(&caminho_db)?;
        let resumo = carregar_resumo(&conn)?;
        let tokens_por_dia = carregar_tokens_por_dia(&conn)?;
        let modelos = carregar_modelos(&conn)?;

        let cores = !self.no_color;
        let hoje = Local::now().date_naive();
        let dias_ativos = tokens_por_dia.keys().copied().collect();
        let streaks = render::calcular_streaks(&dias_ativos, hoje);

        let mut saida = String::new();
        saida.push_str(&format!(
            "\n  {} — {} tokens totais ({} tarefas)\n\n",
            "OpenCode activity".bold(),
            render::numero_compacto(resumo.tokens_totais as f64),
            resumo.tarefas
        ));
        for linha in render::renderizar_heatmap(&tokens_por_dia, self.weeks, hoje, cores) {
            saida.push_str(&linha);
            saida.push('\n');
        }
        saida.push_str(&format!(
            "  {} dias ativos  |  streak atual: {}  |  recorde: {}\n\n",
            dias_ativos.len(),
            streaks.atual,
            streaks.recorde
        ));

        saida.push_str("  [Modelos usados]\n");
        let tokens_maximo = modelos.iter().map(|m| m.tokens as f64).fold(0.0, f64::max);
        for modelo in &modelos {
            let barra = render::renderizar_barra(modelo.tokens as f64, tokens_maximo, 20, cores);
            saida.push_str(&format!(
                "    {:36} {:14} {:>8} tokens  US$ {:.4}  ({} sessões)  {}\n",
                modelo.modelo,
                modelo.provedor,
                render::numero_compacto(modelo.tokens as f64),
                modelo.custo_usd,
                modelo.sessoes,
                barra
            ));
        }
        saida.push_str(&format!("\n  Custo total: US$ {:.2}\n", resumo.custo_usd));

        Ok(saida.trim_end().to_string())
    }
}
```

- [ ] **Step 3: Testar manualmente contra o banco real**

Run: `cargo build`
Expected: compila sem erros.

Run: `cargo run -- ai stats opencode`
Expected: se `~/.local/share/opencode/opencode.db` existir, imprime o heatmap + tabela de modelos + custo total em US$. Se não existir, imprime o erro `banco do OpenCode não encontrado: '...'` (sem panic).

- [ ] **Step 4: Commit**

```bash
git add src/ai/opencode.rs Cargo.toml Cargo.lock
git commit -m "feat(ai): implementa ai stats opencode lendo o SQLite do OpenCode"
```

---

### Task 6: Câmbio USD→BRL e custo em R$ no relatório do OpenCode

**Files:**
- Create: `src/ai/cambio.rs`
- Modify: `src/ai/mod.rs` (declarar `mod cambio;`)
- Modify: `src/ai/opencode.rs` (usar a taxa no relatório)
- Modify: `Cargo.toml` (adicionar `reqwest`, `serde`)

**Interfaces:**
- Produces: `cambio::buscar_taxa_usd_brl() -> Result<f64, Box<dyn std::error::Error>>`; `cambio::converter_para_brl(f64, f64) -> f64`.

- [ ] **Step 1: Adicionar as dependências**

Em `Cargo.toml`:

```toml
reqwest = { version = "0.13", features = ["blocking", "json"] }
serde = { version = "1", features = ["derive"] }
```

- [ ] **Step 2: Escrever o teste que falha (parte pura)**

```rust
// src/ai/cambio.rs
// Câmbio USD -> BRL: parte de IO (chamada HTTP, sem teste automatizado,
// mesmo tratamento que a leitura de arquivos recebe em `src/logs.rs`) +
// uma função pura de conversão, essa sim testável.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converter_para_brl_multiplica_pela_taxa() {
        assert_eq!(converter_para_brl(10.0, 5.0), 50.0);
        assert_eq!(converter_para_brl(0.0, 5.0), 0.0);
    }
}
```

- [ ] **Step 3: Rodar e confirmar que falha**

Run: `cargo test cambio:: 2>&1 | head -20`
Expected: FAIL — `cannot find function 'converter_para_brl'`.

- [ ] **Step 4: Implementar**

```rust
use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

// Só o campo que nos interessa da resposta da API; `serde` ignora o resto
// do JSON automaticamente (não precisamos declarar cada campo da resposta).
#[derive(Deserialize)]
struct RespostaCambio {
    rates: HashMap<String, f64>,
}

// `reqwest::blocking` dá um cliente HTTP síncrono: por baixo dos panos usa
// um runtime assíncrono, mas a API que a gente vê é `Result` comum, sem
// `.await` nem `#[tokio::main]` — não precisamos tornar o resto da CLI
// assíncrona só por causa desta chamada.
pub fn buscar_taxa_usd_brl() -> Result<f64, Box<dyn std::error::Error>> {
    let cliente = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let resposta: RespostaCambio = cliente
        .get("https://api.frankfurter.dev/v1/latest?from=USD&to=BRL")
        .send()?
        .error_for_status()?
        .json()?;

    resposta
        .rates
        .get("BRL")
        .copied()
        .ok_or_else(|| "resposta da API de câmbio não trouxe a taxa BRL".into())
}

// Núcleo puro: dado um valor em USD e uma taxa, devolve o valor em BRL.
pub fn converter_para_brl(valor_usd: f64, taxa: f64) -> f64 {
    valor_usd * taxa
}
```

- [ ] **Step 5: Rodar e confirmar que passa**

Run: `cargo test cambio::`
Expected: PASS (1 teste).

- [ ] **Step 6: Declarar o módulo em `src/ai/mod.rs`**

Adicionar `mod cambio;`.

- [ ] **Step 7: Integrar no relatório do OpenCode**

Em `src/ai/opencode.rs`, dentro de `OpencodeArgs::execute`, logo antes de montar a linha de "Custo total":

```rust
        // Câmbio é best-effort: se a rede falhar, mostramos só US$ com um
        // aviso em vez de derrubar o comando inteiro (chamada de rede é um
        // cenário real de falha, diferente de um bug interno).
        let taxa_brl = crate::ai::cambio::buscar_taxa_usd_brl().ok();
```

E trocar a linha final de custo por:

```rust
        match taxa_brl {
            Some(taxa) => saida.push_str(&format!(
                "\n  Custo total: US$ {:.2}  (R$ {:.2})\n",
                resumo.custo_usd,
                crate::ai::cambio::converter_para_brl(resumo.custo_usd, taxa)
            )),
            None => saida.push_str(&format!(
                "\n  Custo total: US$ {:.2}  (cotação indisponível, R$ não calculado)\n",
                resumo.custo_usd
            )),
        }
```

- [ ] **Step 8: Testar manualmente**

Run: `cargo build`
Expected: compila.

Run: `cargo run -- ai stats opencode`
Expected: linha final mostra `US$ x.xx (R$ y.yy)` com rede disponível, ou o aviso de cotação indisponível sem rede.

- [ ] **Step 9: Commit**

```bash
git add src/ai/cambio.rs src/ai/mod.rs src/ai/opencode.rs Cargo.toml Cargo.lock
git commit -m "feat(ai): busca cotacao USD/BRL e mostra custo em R$ no opencode"
```

---

### Task 7: `ai stats claude` — leitura dos transcritos JSONL e tabelas de horas

**Files:**
- Modify: `src/ai/claude.rs`
- Modify: `src/ai/mod.rs` (tornar `claude` acessível se necessário — já é `mod claude;` privado, sem mudança)
- Modify: `Cargo.toml` (adicionar `serde_json`, `walkdir`)

**Interfaces:**
- Consumes: `render::{Sessao, agregar_por_dia, agregar_por_semana, formatar_horas, renderizar_barra}` (Tasks 2 e 4).
- Produces: `ClaudeArgs::execute` funcional mostrando tabelas de horas (sem custo ainda — isso é a Task 8). Também produz `UsoSessao { modelo: String, tokens_entrada: i64, tokens_saida: i64 }`, consumido pela Task 8.

- [ ] **Step 1: Adicionar as dependências**

Em `Cargo.toml`:

```toml
serde_json = "1"
walkdir = "2.5"
```

- [ ] **Step 2: Implementar**

Substituir o conteúdo de `src/ai/claude.rs` por:

```rust
// Subcomando `ai stats claude`: horas trabalhadas + custo estimado a partir
// dos transcritos locais do Claude Code (~/.claude/projects/**/*.jsonl).
use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Local, Utc};
use clap::Args;
use owo_colors::OwoColorize;
use serde::Deserialize;
use walkdir::WalkDir;

use crate::ai::render;

#[derive(Args, Debug)]
pub struct ClaudeArgs {
    /// Mês no formato YYYY-MM; se omitido, usa o mês atual.
    mes: Option<String>,

    /// Quantidade de dias no ranking de mais intensos.
    #[arg(long, short, default_value_t = 5)]
    top: usize,

    /// Desativa cores ANSI na saída.
    #[arg(long)]
    no_color: bool,

    /// Imprime os dados computados como JSON em vez das tabelas.
    #[arg(long)]
    json: bool,
}

// Só os campos que nos interessam de cada linha do transcrito. `Option`
// modela "esse campo pode não vir" (ex: uma linha sem `usage`) sem precisar
// de valores sentinela.
#[derive(Debug, Deserialize)]
struct Uso {
    input_tokens: i64,
    output_tokens: i64,
    #[serde(default)]
    cache_creation_input_tokens: i64,
    #[serde(default)]
    cache_read_input_tokens: i64,
}

#[derive(Debug, Deserialize)]
struct Mensagem {
    model: Option<String>,
    usage: Option<Uso>,
}

#[derive(Debug, Deserialize)]
struct Registro {
    timestamp: String,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    message: Option<Mensagem>,
}

pub struct UsoSessao {
    pub modelo: String,
    pub tokens_entrada: i64,
    pub tokens_saida: i64,
}

const TETO_HORAS: f64 = 4.0;
const MINIMO_HORAS: f64 = 1.0 / 60.0;

fn diretorio_projetos() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".claude/projects")
}

// Lê todos os `.jsonl` sob `~/.claude/projects`, filtra pelo mês pedido e
// devolve: (a) uma `Sessao` por `session_id` — pro núcleo puro de
// `render.rs` calcular horas — e (b) um `UsoSessao` por mensagem de
// assistente — pro cálculo de custo da Task 8.
pub fn carregar_sessoes(mes: &str) -> (Vec<render::Sessao>, Vec<UsoSessao>) {
    let mut horarios_por_sessao: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();
    let mut usos: Vec<UsoSessao> = Vec::new();

    // `WalkDir` itera recursivamente (todas as subpastas de projeto) sem
    // precisarmos escrever a recursão manual do `read_dir`.
    let arquivos = WalkDir::new(diretorio_projetos())
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entrada| entrada.path().extension().is_some_and(|ext| ext == "jsonl"));

    for entrada in arquivos {
        let Ok(conteudo) = std::fs::read_to_string(entrada.path()) else {
            continue; // arquivo ilegível (permissão etc.) — pula, não derruba o comando
        };

        for linha in conteudo.lines() {
            let Ok(registro) = serde_json::from_str::<Registro>(linha) else {
                continue; // linha malformada — mesmo comportamento do protótipo Python
            };
            if !registro.timestamp.starts_with(mes) {
                continue;
            }
            let Some(session_id) = registro.session_id else {
                continue;
            };
            let Ok(instante) = DateTime::parse_from_rfc3339(&registro.timestamp) else {
                continue;
            };
            horarios_por_sessao
                .entry(session_id)
                .or_default()
                .push(instante.with_timezone(&Utc));

            if let Some(mensagem) = registro.message
                && let Some(uso) = mensagem.usage
            {
                usos.push(UsoSessao {
                    modelo: mensagem.model.unwrap_or_else(|| "desconhecido".to_string()),
                    tokens_entrada: uso.input_tokens
                        + uso.cache_creation_input_tokens
                        + uso.cache_read_input_tokens,
                    tokens_saida: uso.output_tokens,
                });
            }
        }
    }

    let sessoes = horarios_por_sessao
        .into_values()
        .filter_map(|mut horarios| {
            horarios.sort();
            // `?` dentro do `filter_map`: se `first()`/`last()` vier `None`
            // (impossível na prática, já que só inserimos vetores não
            // vazios, mas o compilador não sabe disso), a sessão é
            // descartada em vez de dar panic.
            let inicio = *horarios.first()?;
            let dia = inicio.with_timezone(&Local).date_naive();
            if horarios.len() < 2 {
                return Some(render::Sessao {
                    dia,
                    duracao_horas: 5.0 / 60.0,
                });
            }
            let fim = *horarios.last()?;
            let horas_brutas = (fim - inicio).num_seconds() as f64 / 3600.0;
            Some(render::Sessao {
                dia,
                duracao_horas: horas_brutas.clamp(MINIMO_HORAS, TETO_HORAS),
            })
        })
        .collect();

    (sessoes, usos)
}

impl ClaudeArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        let mes = self
            .mes
            .clone()
            .unwrap_or_else(|| Local::now().format("%Y-%m").to_string());

        let (sessoes, _usos) = carregar_sessoes(&mes);
        if sessoes.is_empty() {
            return Ok(format!("Nenhuma sessão encontrada para {mes}"));
        }

        let cores = !self.no_color;
        let por_dia = render::agregar_por_dia(&sessoes);
        let por_semana = render::agregar_por_semana(&sessoes);

        let total_horas: f64 = por_dia.values().map(|(horas, _)| horas).sum();
        let maximo_dia = por_dia.values().map(|(horas, _)| *horas).fold(0.0, f64::max);
        let maximo_semana = por_semana
            .values()
            .map(|(horas, _, _)| *horas)
            .fold(0.0, f64::max);

        let mut saida = format!(
            "\n  {} — {}\n\n",
            "Claude Code — horas de trabalho".bold(),
            mes
        );

        saida.push_str(&format!(
            "  Total estimado: {}  ({} dias ativos, {} sessões)\n\n",
            render::formatar_horas(total_horas),
            por_dia.len(),
            sessoes.len()
        ));

        saida.push_str("  [Por semana]\n");
        for (segunda, (horas, sessoes_semana, dias)) in &por_semana {
            let barra = render::renderizar_barra(*horas, maximo_semana, 20, cores);
            saida.push_str(&format!(
                "    semana de {segunda}   {:>3} dias   {:>3} sessões   {:>8}   {barra}\n",
                dias.len(),
                sessoes_semana,
                render::formatar_horas(*horas)
            ));
        }

        saida.push_str("\n  [Por dia]\n");
        for (dia, (horas, sessoes_dia)) in &por_dia {
            let barra = render::renderizar_barra(*horas, maximo_dia, 25, cores);
            saida.push_str(&format!(
                "    {dia}   {:>3} sessões   {:>8}   {barra}\n",
                sessoes_dia,
                render::formatar_horas(*horas)
            ));
        }

        saida.push_str(&format!("\n  [Top {} dias mais intensos]\n", self.top));
        let mut dias_ordenados: Vec<_> = por_dia.iter().collect();
        dias_ordenados.sort_by(|a, b| b.1.0.partial_cmp(&a.1.0).unwrap_or(std::cmp::Ordering::Equal));
        for (dia, (horas, sessoes_dia)) in dias_ordenados.into_iter().take(self.top) {
            saida.push_str(&format!(
                "    {dia}   {}   {} sessões\n",
                render::formatar_horas(*horas),
                sessoes_dia
            ));
        }

        Ok(saida.trim_end().to_string())
    }
}
```

- [ ] **Step 3: Testar manualmente**

Run: `cargo build`
Expected: compila.

Run: `cargo run -- ai stats claude 2026-06`
Expected: mostra total estimado, tabela por semana, por dia e top N dias (usando dados reais de `~/.claude/projects`, se existirem para o mês).

- [ ] **Step 4: Commit**

```bash
git add src/ai/claude.rs Cargo.toml Cargo.lock
git commit -m "feat(ai): implementa ai stats claude lendo transcritos jsonl"
```

---

### Task 8: Tabela de preços e custo estimado no relatório do Claude

**Files:**
- Create: `src/ai/precos.rs`
- Modify: `src/ai/mod.rs` (declarar `mod precos;`)
- Modify: `src/ai/claude.rs` (usar `_usos` pra calcular e mostrar custo)

**Interfaces:**
- Produces: `precos::preco_do_modelo(&str) -> Option<Preco>`; `precos::calcular_custo_usd(&str, i64, i64) -> Option<f64>`.
- Consumes: `UsoSessao` (Task 7), `cambio::{buscar_taxa_usd_brl, converter_para_brl}` (Task 6).

- [ ] **Step 1: Escrever os testes que falham**

```rust
// src/ai/precos.rs
// Tabela de preços por modelo (USD por milhão de tokens). Valores
// conhecidos no momento em que este código foi escrito — a Anthropic muda
// preços e lança modelos novos, então esta tabela precisa de manutenção
// manual (ver skill `claude-api` do harness pra conferir os valores atuais
// antes de confiar cegamente num número antigo).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calcula_custo_para_modelo_conhecido() {
        let custo = calcular_custo_usd("claude-sonnet-5", 1_000_000, 1_000_000);
        assert_eq!(custo, Some(18.0)); // $3 entrada + $15 saída, por MTok
    }

    #[test]
    fn modelo_desconhecido_nao_estima_custo() {
        assert_eq!(calcular_custo_usd("modelo-inexistente", 100, 100), None);
    }
}
```

- [ ] **Step 2: Rodar e confirmar que falha**

Run: `cargo test precos:: 2>&1 | head -20`
Expected: FAIL — `cannot find function 'calcular_custo_usd'`.

- [ ] **Step 3: Implementar**

```rust
pub struct Preco {
    pub entrada_por_mtok: f64,
    pub saida_por_mtok: f64,
}

pub fn preco_do_modelo(modelo: &str) -> Option<Preco> {
    match modelo {
        "claude-opus-4-8" | "claude-opus-4-7" => Some(Preco {
            entrada_por_mtok: 15.0,
            saida_por_mtok: 75.0,
        }),
        "claude-sonnet-5" | "claude-sonnet-4-6" | "claude-sonnet-4-5" => Some(Preco {
            entrada_por_mtok: 3.0,
            saida_por_mtok: 15.0,
        }),
        "claude-haiku-4-5" | "claude-haiku-4-5-20251001" => Some(Preco {
            entrada_por_mtok: 0.80,
            saida_por_mtok: 4.0,
        }),
        _ => None,
    }
}

// Núcleo puro: tokens -> custo em USD, ou `None` se o modelo não estiver na
// tabela (o relatório mostra "não estimado" em vez de inventar um número).
pub fn calcular_custo_usd(modelo: &str, tokens_entrada: i64, tokens_saida: i64) -> Option<f64> {
    let preco = preco_do_modelo(modelo)?;
    let custo_entrada = tokens_entrada as f64 / 1_000_000.0 * preco.entrada_por_mtok;
    let custo_saida = tokens_saida as f64 / 1_000_000.0 * preco.saida_por_mtok;
    Some(custo_entrada + custo_saida)
}
```

- [ ] **Step 4: Rodar e confirmar que passa**

Run: `cargo test precos::`
Expected: PASS (2 testes).

- [ ] **Step 5: Declarar o módulo em `src/ai/mod.rs`**

Adicionar `mod precos;`.

- [ ] **Step 6: Integrar no relatório do Claude**

Em `src/ai/claude.rs`, renomear `_usos` para `usos` na assinatura de `execute` e adicionar, antes do `Ok(saida.trim_end().to_string())`:

```rust
        let mut custo_usd_total = 0.0;
        let mut modelos_sem_preco = std::collections::BTreeSet::new();
        for uso in &usos {
            match crate::ai::precos::calcular_custo_usd(&uso.modelo, uso.tokens_entrada, uso.tokens_saida) {
                Some(custo) => custo_usd_total += custo,
                None => {
                    modelos_sem_preco.insert(uso.modelo.clone());
                }
            }
        }

        let taxa_brl = crate::ai::cambio::buscar_taxa_usd_brl().ok();
        saida.push_str("\n  [Custo estimado]\n");
        match taxa_brl {
            Some(taxa) => saida.push_str(&format!(
                "    US$ {:.2}  (R$ {:.2})\n",
                custo_usd_total,
                crate::ai::cambio::converter_para_brl(custo_usd_total, taxa)
            )),
            None => saida.push_str(&format!(
                "    US$ {:.2}  (cotação indisponível, R$ não calculado)\n",
                custo_usd_total
            )),
        }
        if !modelos_sem_preco.is_empty() {
            saida.push_str(&format!(
                "    modelos sem preço na tabela (não estimados): {}\n",
                modelos_sem_preco.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
```

E trocar a linha `let (sessoes, _usos) = carregar_sessoes(&mes);` por `let (sessoes, usos) = carregar_sessoes(&mes);`.

- [ ] **Step 7: Testar manualmente**

Run: `cargo build`
Expected: compila.

Run: `cargo run -- ai stats claude 2026-06`
Expected: mostra a seção `[Custo estimado]` com US$/R$ (ou aviso de cotação indisponível), e a lista de modelos sem preço na tabela, se houver.

- [ ] **Step 8: Commit**

```bash
git add src/ai/precos.rs src/ai/mod.rs src/ai/claude.rs
git commit -m "feat(ai): estima custo do claude por tabela de precos e mostra em US$/R$"
```

---

### Task 9: Saída `--json` em ambos os comandos

**Files:**
- Modify: `src/ai/opencode.rs`
- Modify: `src/ai/claude.rs`

**Interfaces:**
- Nenhuma nova função pública além dos structs `Serialize`; `--json` já existe como flag desde a Task 1.

- [ ] **Step 1: Serializar a saída do opencode**

Em `src/ai/opencode.rs`, adicionar `#[derive(serde::Serialize)]` ao struct `LinhaModelo` (junto de qualquer derive já existente), e no início de `execute`, logo depois de carregar `resumo`/`tokens_por_dia`/`modelos`:

```rust
        if self.json {
            // `chrono::NaiveDate` só implementa `Serialize` se a feature
            // `serde` do chrono estiver ligada — em vez de adicionar mais
            // uma feature só pra isso, convertemos a data pra `String` (via
            // `to_string()`, que usa o `Display` do `NaiveDate`) antes de
            // serializar. Mesma solução usada no `--json` do `claude`.
            #[derive(serde::Serialize)]
            struct LinhaDiaTokens {
                dia: String,
                tokens: i64,
            }
            #[derive(serde::Serialize)]
            struct Saida<'a> {
                resumo_tarefas: i64,
                tokens_totais: i64,
                custo_usd: f64,
                tokens_por_dia: Vec<LinhaDiaTokens>,
                modelos: &'a [LinhaModelo],
            }
            let saida_json = Saida {
                resumo_tarefas: resumo.tarefas,
                tokens_totais: resumo.tokens_totais,
                custo_usd: resumo.custo_usd,
                tokens_por_dia: tokens_por_dia
                    .iter()
                    .map(|(dia, tokens)| LinhaDiaTokens {
                        dia: dia.to_string(),
                        tokens: *tokens,
                    })
                    .collect(),
                modelos: &modelos,
            };
            return Ok(serde_json::to_string_pretty(&saida_json)?);
        }
```

- [ ] **Step 2: Serializar a saída do claude**

Em `src/ai/claude.rs`, logo depois de calcular `por_dia`/`por_semana`/`custo_usd_total` (ou seja, perto do fim de `execute`, antes de montar o texto), adicionar um bloco equivalente:

```rust
        if self.json {
            #[derive(serde::Serialize)]
            struct LinhaDia {
                dia: String,
                horas: f64,
                sessoes: u32,
            }
            #[derive(serde::Serialize)]
            struct Saida {
                mes: String,
                total_horas: f64,
                dias: Vec<LinhaDia>,
            }
            let saida_json = Saida {
                mes: mes.clone(),
                total_horas,
                dias: por_dia
                    .iter()
                    .map(|(dia, (horas, sessoes))| LinhaDia {
                        dia: dia.to_string(),
                        horas: *horas,
                        sessoes: *sessoes,
                    })
                    .collect(),
            };
            return Ok(serde_json::to_string_pretty(&saida_json)?);
        }
```

Posicionar esse bloco logo após a linha `let maximo_semana = ...;` e antes de montar `let mut saida = ...`.

- [ ] **Step 3: Testar manualmente**

Run: `cargo build`
Expected: compila.

Run: `cargo run -- ai stats opencode --json | head -20`
Expected: JSON válido (pode conferir com `| python3 -m json.tool` ou `| jq .`).

Run: `cargo run -- ai stats claude 2026-06 --json | head -20`
Expected: JSON válido com `mes`, `total_horas`, `dias`.

- [ ] **Step 4: Commit**

```bash
git add src/ai/opencode.rs src/ai/claude.rs
git commit -m "feat(ai): adiciona saida --json em ai stats opencode e claude"
```

---

### Task 10: Polimento final

**Files:**
- Modify: `CLAUDE.md` (seção "Comandos")
- Nenhum arquivo de código novo — só ajustes de lint e documentação.

- [ ] **Step 1: Rodar clippy e corrigir tudo que ele apontar**

Run: `cargo clippy -- -D warnings`
Expected: sem warnings. Se houver (ex: `needless_range_loop`, `collapsible_if`, `too_many_arguments`), corrigir seguindo o idiom que o clippy sugerir, preferindo let chains onde aplicável (convenção do `CLAUDE.md`).

- [ ] **Step 2: Rodar a suíte completa**

Run: `cargo test`
Expected: PASS em todos os testes (os 4 de `logs.rs` + os novos de `render.rs`/`precos.rs`/`cambio.rs`). Anotar a contagem total exibida (`test result: ok. N passed`).

- [ ] **Step 3: Atualizar `CLAUDE.md`**

Na seção `## Comandos`, adicionar as duas linhas novas junto das já existentes:

```bash
cargo run -- ai stats opencode   # dashboard de tokens/custo do OpenCode (heatmap + modelos)
cargo run -- ai stats claude     # horas trabalhadas + custo estimado do Claude Code (mês atual)
```

E atualizar a linha `cargo test                  # roda a suíte (4 testes em src/logs.rs)` para refletir a contagem real observada no Step 2 e os novos arquivos com teste (ex: `roda a suíte (N testes em src/logs.rs, src/ai/render.rs, src/ai/precos.rs e src/ai/cambio.rs)` — usar o N real, não um placeholder).

- [ ] **Step 4: Commit final**

```bash
git add CLAUDE.md
git commit -m "docs: documenta ai stats opencode/claude no CLAUDE.md"
```
