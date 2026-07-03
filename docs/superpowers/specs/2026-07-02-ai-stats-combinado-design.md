# `dev-cli ai stats` combinado — design

Data: 2026-07-02

## Contexto e motivação

Hoje `ai stats` exige um subcomando (`opencode` ou `claude`) — ver
[[2026-07-01-ai-stats-design]]. O usuário quer que `dev-cli ai stats`, sem
subcomando, já mostre as estatísticas de **todos** os agentes (OpenCode +
Claude Code) num único dashboard, sem precisar rodar os dois comandos
separadamente.

Decisões já validadas com o usuário:

- O combinado é **um único dashboard mesclado** (heatmap, tabela de modelos e
  custo somados), não dois dashboards um atrás do outro.
- `ai stats opencode` e `ai stats claude` continuam existindo, inalterados,
  para ver só um provedor.
- O combinado aceita as mesmas opções que os individuais: período posicional,
  `--historico`, `--json`, `--weeks`, `--top`, `--no-color`.
- Se um provedor não tiver dados (banco do OpenCode ausente, ou nenhuma
  sessão Claude no período), o combinado **pula esse provedor e mostra o
  resto**, com uma nota indicando o que foi pulado. Só falha se nenhum dos
  dois tiver dados.

## Arquitetura

### Extrair carga+agregação para uma função reutilizável

Hoje a lógica de agregação (montar `Vec<ModeloUso>`, somar custo, coletar
`modelos_sem_preco`) vive dentro do `execute()` de `ClaudeArgs` e
`OpencodeArgs`, misturada com o corte JSON/dashboard. Para o combinado
precisar dos mesmos dados sem duplicar essa lógica, cada módulo ganha uma
função pública de carga:

- `claude::carregar_dados(periodo: &str) -> DadosProvedor`
- `opencode::carregar_dados(periodo: &str) -> Result<DadosProvedor, Box<dyn Error>>`
  (pode falhar: banco pode não existir ou `--db` custom inválido)

`DadosProvedor` é um novo struct público em `render.rs` (mesmo lugar de
`Sessao`/`ModeloUso`, que ele agrega):

```rust
pub struct DadosProvedor {
    pub sessoes: Vec<Sessao>,
    pub modelos: Vec<ModeloUso>,
    pub tokens_por_dia: BTreeMap<NaiveDate, i64>,
    pub custo_total: f64,
    pub sem_preco: Vec<String>,
}
```

Os `execute()` existentes de `ClaudeArgs`/`OpencodeArgs` passam a chamar
`carregar_dados` e usar os campos do struct para montar o JSON ou chamar
`renderizar_dashboard` — sem mudar nenhum comportamento observável dos
subcomandos individuais (mesma saída de hoje).

### `StatsArgs` — subcomando vira opcional

```rust
pub struct StatsArgs {
    #[command(subcommand)]
    comando: Option<StatsCommands>,

    // Mesmos campos que hoje existem em OpencodeArgs/ClaudeArgs,
    // usados só quando `comando` é `None`:
    periodo: Option<String>,
    #[arg(long, conflicts_with = "periodo")]
    historico: bool,
    #[arg(long, default_value_t = 52, value_parser = ...)]
    weeks: u32,
    #[arg(long, short, default_value_t = 5)]
    top: usize,
    #[arg(long)]
    no_color: bool,
    #[arg(long)]
    json: bool,
}
```

`execute()`:

```rust
match &self.comando {
    Some(StatsCommands::Opencode(args)) => args.execute(),
    Some(StatsCommands::Claude(args)) => args.execute(),
    None => self.execute_combinado(),
}
```

### Fluxo do combinado (`execute_combinado`)

1. Resolve `periodo` (mesma regra dos individuais: `--historico` → string
   vazia, senão `periodo` explícito ou mês atual).
2. Carrega `claude::carregar_dados(&periodo)` (sempre `Ok` — só ignora
   arquivos ilegíveis) e `opencode::carregar_dados(&periodo)` (pode dar
   `Err` se o banco não existir).
3. Cada provedor entra na lista de "pulados" se: `opencode` retornou `Err`,
   ou os dados carregados vieram vazios (`sessoes.is_empty() &&
   tokens_por_dia.is_empty()`, mesmo critério que hoje decide "nenhuma
   sessão encontrada").
4. Se os dois foram pulados, retorna a mesma mensagem de hoje: `"Nenhuma
   sessão encontrada para {periodo}"`.
5. Mescla os `DadosProvedor` restantes:
   - `tokens_por_dia`: soma por dia (`BTreeMap` merge, `+=` em chaves
     coincidentes).
   - `sessoes`: concatenação simples dos dois `Vec<Sessao>` (o heatmap de
     horas e o ranking de top dias não distinguem provedor).
   - `modelos`: concatenação simples — a coluna `provedor` de `ModeloUso`
     (já existe: `"anthropic"` vs o provedor do OpenCode) já diferencia as
     linhas na tabela renderizada, sem precisar de agregação extra.
   - `custo_total`: soma.
   - `sem_preco`: concatenação + dedup (`BTreeSet` para ordenar e remover
     repetição).
6. Se `--json`, serializa o `DadosProvedor` mesclado (mesmo formato de
   campos que os individuais usam para modelos/tokens/dias, mais um campo
   `provedores_pulados: Vec<String>`).
7. Senão, chama `render::renderizar_dashboard("IA atividade", &subtitulo,
   ...)` com os dados mesclados. Se algum provedor foi pulado, o subtítulo
   ganha uma nota (ex: `"2026-07 (sem dados do OpenCode)"`).

Nenhuma mudança em `render.rs` além do novo struct `DadosProvedor` — a
função `renderizar_dashboard` já é agnóstica a quantos provedores estão
misturados na lista de `modelos`.

## Testes

- Teste novo em `render.rs` (ou num módulo de teste em `stats.rs`, puro, sem
  IO): dado dois `DadosProvedor` sintéticos (construídos à mão, sem tocar
  banco/disco), a função de mesclagem soma tokens/custo corretamente e
  concatena `sessoes`/`modelos` sem perder linhas.
- Os testes existentes de `render.rs`/`precos.rs`/`cambio.rs` continuam
  válidos sem alteração — a refatoração de `carregar_dados` não muda a
  lógica interna, só extrai o que já existia para uma função nomeada.

## Fora de escopo (esta iteração)

- Mudar o comportamento dos subcomandos `ai stats opencode` / `ai stats
  claude` — devem produzir exatamente a mesma saída de hoje.
- Suporte a outros provedores no combinado (`codex`, etc.) — quando algum
  for adicionado, entra na mesma lista de `carregar_dados` + merge, mas
  isso é trabalho futuro.
- Colorir/distinguir visualmente linhas de provedores diferentes na tabela
  de modelos além da coluna `provedor` que já existe.
