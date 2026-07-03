# `ai stats` combinado — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rodar `dev-cli ai stats` sem subcomando e ver um único dashboard combinando os dados de OpenCode e Claude Code.

**Architecture:** Extrair a agregação (tokens/dia, modelos, custo, "sem preço") que hoje vive dentro de `ClaudeArgs::execute()` e `OpencodeArgs::execute()` para funções `carregar_dados` reaproveitáveis, devolvendo um novo struct compartilhado `render::DadosProvedor`. `StatsArgs` ganha um subcomando opcional (`Option<StatsCommands>`) e os mesmos campos de período/flags dos comandos individuais; quando nenhum provedor é passado, carrega os dois `DadosProvedor`, mescla com `render::mesclar_dados` (pulando o provedor que não tiver dados) e delega para `render::renderizar_dashboard` de sempre.

**Tech Stack:** Rust (edition 2024), clap (derive), chrono, rusqlite, serde/serde_json — nenhuma dependência nova.

## Global Constraints

- Português (pt-br) em struct/função/variável — ver `CLAUDE.md`.
- Sem `unwrap()` fora de `#[cfg(test)]`.
- Código clippy-clean: `cargo clippy` sem warnings antes de considerar pronto.
- Usar let chains (edition 2024) em vez de `if` aninhado.
- Comentários didáticos nos trechos novos, focando no "porquê"/conceito Rust, sem parafrasear o óbvio.
- `ai stats opencode` e `ai stats claude` devem produzir exatamente a mesma saída de hoje — a refatoração não pode mudar o comportamento observável desses dois subcomandos.
- Spec de referência: `docs/superpowers/specs/2026-07-02-ai-stats-combinado-design.md`.

---

### Task 1: `DadosProvedor` + `mesclar_dados` em `render.rs`

**Files:**
- Modify: `src/ai/render.rs:433` (logo após o `impl ModeloUso` e antes do doc-comment de `renderizar_dashboard`)
- Test: `src/ai/render.rs` (dentro do `mod tests` existente, a partir da linha 811)

**Interfaces:**
- Produces: `pub struct DadosProvedor { pub sessoes: Vec<Sessao>, pub modelos: Vec<ModeloUso>, pub tokens_por_dia: BTreeMap<NaiveDate, i64>, pub custo_total: f64, pub sem_preco: Vec<String> }` e `pub fn mesclar_dados(a: DadosProvedor, b: DadosProvedor) -> DadosProvedor` — usados pelas Tasks 2, 3 e 4.

- [ ] **Step 1: Escrever o teste de mesclagem (vai falhar: tipos ainda não existem)**

Adicionar ao final do `mod tests` em `src/ai/render.rs` (depois do último `#[test]` do arquivo):

```rust
    #[test]
    fn mesclar_dados_soma_tokens_e_custo_e_concatena_modelos_e_sessoes() {
        let dia1 = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let dia2 = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();

        let a = DadosProvedor {
            sessoes: vec![Sessao {
                dia: dia1,
                duracao_horas: 1.0,
            }],
            modelos: vec![ModeloUso {
                modelo: "claude-x".to_string(),
                provedor: "anthropic".to_string(),
                sessoes: 1,
                tokens_entrada: 10,
                tokens_cache_escrita: 0,
                tokens_cache_leitura: 0,
                tokens_saida: 5,
                custo_entrada: 0.1,
                custo_cache_escrita: 0.0,
                custo_cache_leitura: 0.0,
                custo_saida: 0.05,
            }],
            tokens_por_dia: BTreeMap::from([(dia1, 100)]),
            custo_total: 0.15,
            sem_preco: vec!["modelo-a".to_string()],
        };
        let b = DadosProvedor {
            sessoes: vec![Sessao {
                dia: dia2,
                duracao_horas: 2.0,
            }],
            modelos: vec![ModeloUso {
                modelo: "grok-y".to_string(),
                provedor: "opencode".to_string(),
                sessoes: 1,
                tokens_entrada: 20,
                tokens_cache_escrita: 0,
                tokens_cache_leitura: 0,
                tokens_saida: 8,
                custo_entrada: 0.2,
                custo_cache_escrita: 0.0,
                custo_cache_leitura: 0.0,
                custo_saida: 0.08,
            }],
            tokens_por_dia: BTreeMap::from([(dia1, 50), (dia2, 200)]),
            custo_total: 0.28,
            sem_preco: vec!["modelo-a".to_string(), "modelo-b".to_string()],
        };

        let mesclado = mesclar_dados(a, b);

        assert_eq!(mesclado.tokens_por_dia.get(&dia1), Some(&150));
        assert_eq!(mesclado.tokens_por_dia.get(&dia2), Some(&200));
        assert_eq!(mesclado.sessoes.len(), 2);
        assert_eq!(mesclado.modelos.len(), 2);
        assert!((mesclado.custo_total - 0.43).abs() < 1e-9);
        assert_eq!(
            mesclado.sem_preco,
            vec!["modelo-a".to_string(), "modelo-b".to_string()]
        );
    }
```

- [ ] **Step 2: Rodar para confirmar que falha (tipos inexistentes)**

Run: `cargo test mesclar_dados -- --nocapture`
Expected: FAIL com erro de compilação `cannot find struct, variant or union type 'DadosProvedor'` (ou `cannot find function 'mesclar_dados'`).

- [ ] **Step 3: Implementar `DadosProvedor` e `mesclar_dados`**

Inserir em `src/ai/render.rs`, logo antes do doc-comment `/// Monta o dashboard unificado...` (linha 435 do arquivo atual):

```rust
/// Pacote de dados já carregados e agregados de um provedor (OpenCode ou
/// Claude), pronto para virar dashboard ou JSON. Existe para que `ai
/// stats` sem subcomando (`stats.rs`) possa carregar os dois provedores e
/// mesclá-los com `mesclar_dados`, sem duplicar a lógica de agregação que
/// já vive em `claude::carregar_dados` e `opencode::carregar_dados`.
#[derive(Debug)]
pub struct DadosProvedor {
    pub sessoes: Vec<Sessao>,
    pub modelos: Vec<ModeloUso>,
    pub tokens_por_dia: BTreeMap<NaiveDate, i64>,
    pub custo_total: f64,
    pub sem_preco: Vec<String>,
}

/// Mescla dois `DadosProvedor` num só: tokens por dia somados por chave
/// (`entry().or_insert(0) +=`, mesmo idiom usado no resto do arquivo),
/// sessões e modelos concatenados (a coluna `provedor` de `ModeloUso` já
/// distingue as linhas na tabela renderizada, então não precisa reagrupar)
/// e custo somado. `sem_preco` passa por um `BTreeSet` só para ordenar e
/// remover duplicatas entre os dois provedores.
pub fn mesclar_dados(mut a: DadosProvedor, b: DadosProvedor) -> DadosProvedor {
    for (dia, tokens) in b.tokens_por_dia {
        *a.tokens_por_dia.entry(dia).or_insert(0) += tokens;
    }
    a.sessoes.extend(b.sessoes);
    a.modelos.extend(b.modelos);
    a.custo_total += b.custo_total;

    let sem_preco: BTreeSet<String> = a.sem_preco.into_iter().chain(b.sem_preco).collect();
    a.sem_preco = sem_preco.into_iter().collect();

    a
}
```

- [ ] **Step 4: Rodar o teste e confirmar que passa**

Run: `cargo test mesclar_dados -- --nocapture`
Expected: PASS (`test ai::render::tests::mesclar_dados_soma_tokens_e_custo_e_concatena_modelos_e_sessoes ... ok`)

- [ ] **Step 5: Rodar a suíte inteira e o clippy antes de commitar**

Run: `cargo test && cargo clippy -- -D warnings`
Expected: todos os testes passam (29 no total, o novo incluso); clippy sem warnings.

- [ ] **Step 6: Commit**

```bash
git add src/ai/render.rs
git commit -m "$(cat <<'EOF'
feat(ai): adiciona DadosProvedor e mesclar_dados em render.rs

Struct e função puros que vão permitir que ai stats (sem subcomando)
carregue OpenCode e Claude e mescle os dois num só dashboard.
EOF
)"
```

---

### Task 2: Extrair `claude::carregar_dados`

**Files:**
- Modify: `src/ai/claude.rs:221-385` (`impl ClaudeArgs { pub fn execute(...) }`)

**Interfaces:**
- Consumes: `carregar_sessoes(mes: &str) -> (Vec<render::Sessao>, Vec<UsoSessao>, BTreeMap<NaiveDate, i64>)` (já existe, sem mudança); `render::DadosProvedor` (Task 1); `crate::ai::precos::calcular_custo_detalhado`.
- Produces: `pub fn carregar_dados(periodo: &str) -> render::DadosProvedor` — consumido pela Task 4 (`stats.rs`).

- [ ] **Step 1: Extrair a agregação para `carregar_dados`, mantendo `execute()` com o mesmo comportamento**

Em `src/ai/claude.rs`, substituir o corpo de `impl ClaudeArgs { pub fn execute(...) }` (linhas 221-385) por:

```rust
// ── carregar_dados ──────────────────────────────────────────────────
// Carrega as sessões do período e agrega tokens/custo por modelo,
// devolvendo o pacote compartilhado `DadosProvedor`. Extraído de
// `execute()` para ser reaproveitado pelo dashboard combinado (`ai
// stats`, sem subcomando, em `stats.rs`) sem duplicar esta lógica.
pub fn carregar_dados(periodo: &str) -> render::DadosProvedor {
    let (sessoes, usos, tokens_por_dia) = carregar_sessoes(periodo);

    let mut custo_usd_total = 0.0;
    let mut modelos_sem_preco: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut por_modelo: BTreeMap<String, render::ModeloUso> = BTreeMap::new();

    for uso in &usos {
        // "<synthetic>" é o placeholder interno do Claude Code para
        // mensagens de erro/rate-limit — não é uso real de um modelo
        // (tokens sempre zerados), então não entra na tabela.
        if uso.modelo == "<synthetic>" {
            continue;
        }

        let entry = por_modelo
            .entry(uso.modelo.clone())
            .or_insert(render::ModeloUso {
                modelo: uso.modelo.clone(),
                provedor: "anthropic".to_string(),
                sessoes: 0,
                tokens_entrada: 0,
                tokens_cache_escrita: 0,
                tokens_cache_leitura: 0,
                tokens_saida: 0,
                custo_entrada: 0.0,
                custo_cache_escrita: 0.0,
                custo_cache_leitura: 0.0,
                custo_saida: 0.0,
            });
        entry.tokens_entrada += uso.tokens_entrada;
        entry.tokens_cache_escrita += uso.tokens_cache_escrita;
        entry.tokens_cache_leitura += uso.tokens_cache_leitura;
        entry.tokens_saida += uso.tokens_saida;
        entry.sessoes += 1;

        if let Some(custo) = crate::ai::precos::calcular_custo_detalhado(
            &uso.modelo,
            uso.tokens_entrada,
            uso.tokens_cache_escrita,
            uso.tokens_cache_leitura,
            uso.tokens_saida,
        ) {
            custo_usd_total += custo.total();
            entry.custo_entrada += custo.entrada;
            entry.custo_cache_escrita += custo.cache_escrita;
            entry.custo_cache_leitura += custo.cache_leitura;
            entry.custo_saida += custo.saida;
        } else {
            modelos_sem_preco.insert(uso.modelo.clone());
        }
    }

    render::DadosProvedor {
        sessoes,
        modelos: por_modelo.into_values().collect(),
        tokens_por_dia,
        custo_total: custo_usd_total,
        sem_preco: modelos_sem_preco.into_iter().collect(),
    }
}

// ── execute() ───────────────────────────────────────────────────────
// Resolve o período, carrega os dados via `carregar_dados` e delega
// para JSON ou `render::renderizar_dashboard`.
impl ClaudeArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        let periodo = if self.historico {
            String::new()
        } else {
            self.periodo
                .clone()
                .unwrap_or_else(|| Local::now().format("%Y-%m").to_string())
        };

        let dados = carregar_dados(&periodo);
        if dados.sessoes.is_empty() && dados.tokens_por_dia.is_empty() {
            return Ok(format!("Nenhuma sessão encontrada para {periodo}"));
        }

        let por_dia = render::agregar_por_dia(&dados.sessoes);
        let total_horas: f64 = por_dia.values().map(|(h, _)| h).sum();
        let subtitulo = if self.historico {
            format!(
                "{:.1}h totais em {} sessões",
                total_horas,
                dados.sessoes.len()
            )
        } else {
            periodo.clone()
        };

        if self.json {
            #[derive(serde::Serialize)]
            struct LinhaDia {
                dia: String,
                horas: f64,
                sessoes: u32,
            }
            #[derive(serde::Serialize)]
            struct LinhaDiaTokens {
                dia: String,
                tokens: i64,
            }
            #[derive(serde::Serialize)]
            struct Saida {
                historico: bool,
                mes: String,
                total_horas: f64,
                dias: Vec<LinhaDia>,
                custo_usd_total: f64,
                modelos: Vec<render::ModeloUso>,
                modelos_sem_preco: Vec<String>,
                tokens_por_dia: Vec<LinhaDiaTokens>,
            }
            let saida_json = Saida {
                historico: self.historico,
                mes: periodo.clone(),
                total_horas,
                dias: por_dia
                    .iter()
                    .map(|(dia, (horas, sessoes))| LinhaDia {
                        dia: dia.to_string(),
                        horas: *horas,
                        sessoes: *sessoes,
                    })
                    .collect(),
                custo_usd_total: dados.custo_total,
                modelos: dados.modelos,
                modelos_sem_preco: dados.sem_preco,
                tokens_por_dia: dados
                    .tokens_por_dia
                    .iter()
                    .map(|(dia, tokens)| LinhaDiaTokens {
                        dia: dia.to_string(),
                        tokens: *tokens,
                    })
                    .collect(),
            };
            return Ok(serde_json::to_string_pretty(&saida_json)?);
        }

        Ok(render::renderizar_dashboard(
            "Claude Code atividade",
            &subtitulo,
            &dados.tokens_por_dia,
            &dados.sessoes,
            &dados.modelos,
            dados.custo_total,
            &dados.sem_preco,
            self.weeks,
            !self.no_color,
            Some(self.top),
        ))
    }
}
```

- [ ] **Step 2: Rodar a suíte inteira e comparar a saída manual com o comportamento atual**

Run: `cargo test && cargo clippy -- -D warnings`
Expected: todos os testes passam, sem warnings (a lógica interna não mudou, só foi movida).

Run: `cargo run -- ai stats claude --historico`
Expected: mesma saída (dashboard "Claude Code atividade") que rodava antes da mudança — comparar visualmente com uma execução do binário antes desta task (ou confiar que a lógica é idêntica, já que só foi movida de lugar).

- [ ] **Step 3: Commit**

```bash
git add src/ai/claude.rs
git commit -m "$(cat <<'EOF'
refactor(ai): extrai claude::carregar_dados de ClaudeArgs::execute

Isola a agregação de tokens/custo por modelo numa função reaproveitável
pelo dashboard combinado de ai stats, sem mudar o comportamento do
subcomando `ai stats claude`.
EOF
)"
```

---

### Task 3: Extrair `opencode::carregar_dados` (e `agregar` interno)

**Files:**
- Modify: `src/ai/opencode.rs:301-481` (`impl OpencodeArgs { pub fn execute(...) }`)

**Interfaces:**
- Consumes: `carregar_tokens_por_dia`, `carregar_modelos`, `carregar_sessoes_opencode`, `carregar_resumo`, `caminho_padrao_db` (já existem, sem mudança); `render::DadosProvedor` (Task 1); `crate::ai::precos::distribuir_custo_proporcional`.
- Produces: `pub fn carregar_dados(periodo: &str) -> Result<render::DadosProvedor, Box<dyn std::error::Error>>` — consumido pela Task 4 (`stats.rs`). (Função privada auxiliar `agregar(conn: &Connection, periodo: &str) -> rusqlite::Result<render::DadosProvedor>` usada tanto por `carregar_dados` quanto por `OpencodeArgs::execute()`.)

- [ ] **Step 1: Extrair a agregação (com estimativa de preço `-free`) para `agregar` + wrapper público `carregar_dados`, e atualizar `execute()`**

Em `src/ai/opencode.rs`, logo depois da função `carregar_sessoes_opencode` (antes do comentário `// ── execute() ───...`, linha 293 do arquivo atual), inserir:

```rust
// ── agregar ─────────────────────────────────────────────────────────
// Carrega tokens/dia, modelos (com estimativa de custo para `-free`) e
// sessões a partir de uma conexão já aberta, devolvendo o pacote
// compartilhado `DadosProvedor`. Privada porque só tem sentido com uma
// `Connection` já validada — quem chama de fora usa `carregar_dados`
// (caminho padrão) ou, no caso do subcomando `opencode` com `--db`
// customizado, abre a conexão em `execute()` e chama esta função direto.
fn agregar(conn: &Connection, periodo: &str) -> rusqlite::Result<render::DadosProvedor> {
    let tokens_por_dia = carregar_tokens_por_dia(conn, periodo)?;
    let mut modelos = carregar_modelos(conn, periodo)?;
    let sessoes = carregar_sessoes_opencode(conn, periodo)?;

    // Estimativa de custo para modelos `-free`: usam a taxa efetiva do
    // modelo não-free equivalente presente no próprio banco (taxa =
    // custo_real / tokens), redistribuída entre entrada/cache/saída na
    // mesma proporção do modelo free. Sem equivalente não-free no
    // período, o modelo free fica sem estimativa (entra em `sem_preco`).
    let mut modelos_sem_preco: Vec<String> = Vec::new();
    let nao_free: BTreeMap<String, (f64, i64)> = modelos
        .iter()
        .filter(|m| !m.modelo.ends_with("-free"))
        .map(|m| (m.modelo.clone(), (m.custo_total(), m.tokens_totais())))
        .collect();
    for m in &mut modelos {
        if m.modelo.ends_with("-free") {
            let base = m.modelo.trim_end_matches("-free");
            if let Some(&(custo_pago, tokens_pagos)) = nao_free.get(base)
                && tokens_pagos > 0
            {
                let custo_estimado = m.tokens_totais() as f64 * custo_pago / tokens_pagos as f64;
                let custo = crate::ai::precos::distribuir_custo_proporcional(
                    custo_estimado,
                    m.tokens_entrada,
                    m.tokens_cache_escrita,
                    m.tokens_cache_leitura,
                    m.tokens_saida,
                );
                m.custo_entrada = custo.entrada;
                m.custo_cache_escrita = custo.cache_escrita;
                m.custo_cache_leitura = custo.cache_leitura;
                m.custo_saida = custo.saida;
            } else {
                modelos_sem_preco.push(m.modelo.clone());
            }
        }
    }

    let custo_total: f64 = modelos.iter().map(|m| m.custo_total()).sum();

    Ok(render::DadosProvedor {
        sessoes,
        modelos,
        tokens_por_dia,
        custo_total,
        sem_preco: modelos_sem_preco,
    })
}

// ── carregar_dados ──────────────────────────────────────────────────
// Wrapper público usado pelo dashboard combinado (`stats.rs`): abre o
// banco no caminho padrão e delega para `agregar`. O subcomando
// `opencode` (que aceita `--db` customizado) não passa por aqui — abre
// sua própria conexão em `execute()` e chama `agregar` direto.
pub fn carregar_dados(periodo: &str) -> Result<render::DadosProvedor, Box<dyn std::error::Error>> {
    let caminho_db = caminho_padrao_db();
    if !caminho_db.exists() {
        return Err(format!(
            "banco do OpenCode não encontrado: '{}'",
            caminho_db.display()
        )
        .into());
    }
    let conn = Connection::open(&caminho_db)?;
    Ok(agregar(&conn, periodo)?)
}
```

Em seguida, substituir o corpo de `impl OpencodeArgs { pub fn execute(...) }` (linhas 301-481 do arquivo atual) por:

```rust
impl OpencodeArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // ── Conexão com o banco ──────────────────────────────────────
        // Usa o caminho customizado (`--db`) ou o padrão
        // (`~/.local/share/opencode/opencode.db`).
        let caminho_db = self.db.clone().unwrap_or_else(caminho_padrao_db);
        if !caminho_db.exists() {
            return Err(format!(
                "banco do OpenCode não encontrado: '{}'",
                caminho_db.display()
            )
            .into());
        }

        let conn = Connection::open(&caminho_db)?;

        // ── Define o filtro de período ────────────────────────────────
        let periodo = if self.historico {
            String::new()
        } else {
            self.periodo
                .clone()
                .unwrap_or_else(|| Local::now().format("%Y-%m").to_string())
        };

        // `resumo` (tarefas/tokens brutos da tabela `message`) só é usado
        // pelo subtítulo/JSON deste comando — não faz parte do pacote
        // compartilhado `DadosProvedor`, que vem de `agregar`.
        let resumo = carregar_resumo(&conn, &periodo)?;
        let dados = agregar(&conn, &periodo)?;

        // ── Subtítulo do dashboard ───────────────────────────────────
        let subtitulo = if self.historico {
            format!(
                "{} tokens totais ({} tarefas)",
                render::numero_compacto(resumo.tokens_totais as f64),
                resumo.tarefas
            )
        } else {
            periodo.clone()
        };

        // ── JSON (early return) ──────────────────────────────────────
        if self.json {
            #[derive(serde::Serialize)]
            struct LinhaDiaTokens {
                dia: String,
                tokens: i64,
            }
            #[derive(serde::Serialize)]
            struct LinhaDiaSessao {
                dia: String,
                horas: f64,
                sessoes: u32,
            }
            #[derive(serde::Serialize)]
            struct SaidaSessao {
                total_horas: f64,
                sessoes: usize,
                dias: Vec<LinhaDiaSessao>,
            }
            #[derive(serde::Serialize)]
            struct Saida {
                historico: bool,
                mes: String,
                resumo_tarefas: i64,
                tokens_totais: i64,
                custo_usd: f64,
                modelos_sem_preco: Vec<String>,
                tokens_por_dia: Vec<LinhaDiaTokens>,
                modelos: Vec<render::ModeloUso>,
                sessoes_horas: SaidaSessao,
            }
            let por_dia_horas = render::agregar_por_dia(&dados.sessoes);
            let total_horas: f64 = por_dia_horas.values().map(|(h, _)| h).sum();
            let saida_json = Saida {
                historico: self.historico,
                mes: subtitulo.clone(),
                resumo_tarefas: resumo.tarefas,
                tokens_totais: resumo.tokens_totais,
                custo_usd: dados.custo_total,
                modelos_sem_preco: dados.sem_preco,
                tokens_por_dia: dados
                    .tokens_por_dia
                    .iter()
                    .map(|(dia, tokens)| LinhaDiaTokens {
                        dia: dia.to_string(),
                        tokens: *tokens,
                    })
                    .collect(),
                modelos: dados.modelos,
                sessoes_horas: SaidaSessao {
                    total_horas,
                    sessoes: dados.sessoes.len(),
                    dias: por_dia_horas
                        .iter()
                        .map(|(dia, (horas, sessoes))| LinhaDiaSessao {
                            dia: dia.to_string(),
                            horas: *horas,
                            sessoes: *sessoes,
                        })
                        .collect(),
                },
            };
            return Ok(serde_json::to_string_pretty(&saida_json)?);
        }

        // ── Dashboard visual (delega para render.rs) ─────────────────
        Ok(render::renderizar_dashboard(
            "OpenCode atividade",
            &subtitulo,
            &dados.tokens_por_dia,
            &dados.sessoes,
            &dados.modelos,
            dados.custo_total,
            &dados.sem_preco,
            self.weeks,
            !self.no_color,
            None, // sem top dias
        ))
    }
}
```

- [ ] **Step 2: Rodar a suíte inteira e comparar a saída manual com o comportamento atual**

Run: `cargo test && cargo clippy -- -D warnings`
Expected: todos os testes passam, sem warnings.

Run: `cargo run -- ai stats opencode --historico` (ou sem `--historico`, se não houver banco do OpenCode na máquina — nesse caso confirmar que o erro `banco do OpenCode não encontrado: '...'` continua idêntico ao de antes)
Expected: mesma saída de antes da mudança.

- [ ] **Step 3: Commit**

```bash
git add src/ai/opencode.rs
git commit -m "$(cat <<'EOF'
refactor(ai): extrai opencode::agregar/carregar_dados de OpencodeArgs

Isola a agregação de tokens/custo/modelos (incluindo a estimativa de
preço para modelos -free) numa função reaproveitável pelo dashboard
combinado de ai stats, sem mudar o comportamento do subcomando
`ai stats opencode`.
EOF
)"
```

---

### Task 4: `ai stats` sem subcomando → dashboard combinado

**Files:**
- Modify: `src/help.rs` (novo template)
- Modify: `src/ai/stats.rs` (reescrita completa do arquivo)

**Interfaces:**
- Consumes: `claude::carregar_dados(periodo: &str) -> render::DadosProvedor` (Task 2); `opencode::carregar_dados(periodo: &str) -> Result<render::DadosProvedor, Box<dyn std::error::Error>>` (Task 3); `render::mesclar_dados(a, b) -> DadosProvedor` (Task 1); `render::renderizar_dashboard` (já existe).
- Produces: `StatsArgs::execute(&self) -> Result<String, Box<dyn std::error::Error>>` continua sendo o ponto de entrada chamado por `AiArgs::execute()` em `src/ai/mod.rs:29` — sem mudança de assinatura.

- [ ] **Step 1: Adicionar o template de help combinado em `src/help.rs`**

Adicionar ao final de `src/help.rs`:

```rust
// Template para comandos que têm subcomando opcional + argumentos
// próprios (ex: `ai stats`, que sem subcomando mostra um dashboard
// combinado, e com um provedor explícito encaminha só para ele).
pub const ARGUMENTOS_SUBCOMANDOS: &str =
    "{about}\nUso: {usage}\n\n{all-args}\n\nComandos:\n{subcommands}";
```

- [ ] **Step 2: Reescrever `src/ai/stats.rs` com o subcomando opcional e o caminho combinado**

Substituir todo o conteúdo de `src/ai/stats.rs` por:

```rust
// Subcomando `ai stats`: sem argumento, mostra um dashboard combinado de
// todos os provedores (OpenCode + Claude Code); com um provedor explícito
// (`ai stats opencode` / `ai stats claude`), encaminha só para aquele —
// mesmo comportamento de hoje, sem mudança.
use chrono::Local;
use clap::Args;
use clap::Subcommand;

use crate::ai::claude;
use crate::ai::claude::ClaudeArgs;
use crate::ai::opencode;
use crate::ai::opencode::OpencodeArgs;
use crate::ai::render;

/// Estatísticas de uso de IA: todos os provedores combinados (padrão) ou
/// um provedor específico (`opencode`/`claude`).
#[derive(Args, Debug)]
#[command(
    help_template = crate::help::ARGUMENTOS_SUBCOMANDOS,
    next_help_heading = crate::help::OPCOES
)]
pub struct StatsArgs {
    #[command(subcommand)]
    comando: Option<StatsCommands>,

    /// Período: mês (YYYY-MM) ou dia (YYYY-MM-DD). Se omitido, usa o mês
    /// atual. Só usado quando nenhum provedor é passado (combinado).
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    periodo: Option<String>,

    /// Mostra todo o histórico (ignora o filtro de período).
    #[arg(long, conflicts_with = "periodo", help_heading = crate::help::OPCOES)]
    historico: bool,

    /// Largura do heatmap em semanas (4-104, padrão 52).
    #[arg(long, default_value_t = 52, value_parser = clap::value_parser!(u32).range(4..=104), help_heading = crate::help::OPCOES)]
    weeks: u32,

    /// Quantos dias mostrar no ranking dos mais intensos (padrão 5).
    #[arg(long, short, default_value_t = 5, help_heading = crate::help::OPCOES)]
    top: usize,

    /// Desativa cores ANSI (útil para pipes/arquivos).
    #[arg(long, help_heading = crate::help::OPCOES)]
    no_color: bool,

    /// Em vez do dashboard, imprime JSON com os dados brutos.
    #[arg(long, help_heading = crate::help::OPCOES)]
    json: bool,
}

impl StatsArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `match` sobre a referência do enum. Cada variante carrega um tipo de
        // args diferente (`OpencodeArgs` vs `ClaudeArgs`). Sem subcomando
        // (`None`), roda o dashboard combinado.
        match &self.comando {
            Some(StatsCommands::Opencode(args)) => args.execute(),
            Some(StatsCommands::Claude(args)) => args.execute(),
            None => self.execute_combinado(),
        }
    }

    // ── execute_combinado ────────────────────────────────────────────
    // Carrega os dois provedores e mescla num só dashboard. Um provedor
    // sem dados no período (banco do OpenCode ausente, ou nenhuma sessão
    // Claude naquele mês) é pulado silenciosamente — o comando só falha
    // se os dois estiverem vazios, igual ao "nenhuma sessão encontrada"
    // que cada subcomando individual já retorna hoje.
    fn execute_combinado(&self) -> Result<String, Box<dyn std::error::Error>> {
        let periodo = if self.historico {
            String::new()
        } else {
            self.periodo
                .clone()
                .unwrap_or_else(|| Local::now().format("%Y-%m").to_string())
        };

        let dados_claude = claude::carregar_dados(&periodo);
        let claude_vazio =
            dados_claude.sessoes.is_empty() && dados_claude.tokens_por_dia.is_empty();
        let resultado_opencode = opencode::carregar_dados(&periodo);

        // Cada provedor vira `None` (e entra em `pulados`) se não tiver
        // dados no período ou se a carga falhou (banco ausente, etc.).
        let mut pulados: Vec<&str> = Vec::new();
        let claude_opt = if claude_vazio {
            pulados.push("Claude Code");
            None
        } else {
            Some(dados_claude)
        };
        let opencode_opt = match resultado_opencode {
            Ok(dados) if !(dados.sessoes.is_empty() && dados.tokens_por_dia.is_empty()) => {
                Some(dados)
            }
            _ => {
                pulados.push("OpenCode");
                None
            }
        };

        let dados = match (claude_opt, opencode_opt) {
            (None, None) => return Ok(format!("Nenhuma sessão encontrada para {periodo}")),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (Some(a), Some(b)) => render::mesclar_dados(a, b),
        };

        let por_dia = render::agregar_por_dia(&dados.sessoes);
        let total_horas: f64 = por_dia.values().map(|(h, _)| h).sum();
        let nota_pulados = if pulados.is_empty() {
            String::new()
        } else {
            format!(" (sem dados do {})", pulados.join(" e do "))
        };
        let subtitulo = if self.historico {
            format!(
                "{:.1}h totais em {} sessões{}",
                total_horas,
                dados.sessoes.len(),
                nota_pulados
            )
        } else {
            format!("{periodo}{nota_pulados}")
        };

        if self.json {
            #[derive(serde::Serialize)]
            struct LinhaDia {
                dia: String,
                horas: f64,
                sessoes: u32,
            }
            #[derive(serde::Serialize)]
            struct LinhaDiaTokens {
                dia: String,
                tokens: i64,
            }
            #[derive(serde::Serialize)]
            struct Saida {
                historico: bool,
                mes: String,
                provedores_pulados: Vec<String>,
                total_horas: f64,
                dias: Vec<LinhaDia>,
                custo_usd_total: f64,
                modelos: Vec<render::ModeloUso>,
                modelos_sem_preco: Vec<String>,
                tokens_por_dia: Vec<LinhaDiaTokens>,
            }
            let saida_json = Saida {
                historico: self.historico,
                mes: periodo.clone(),
                provedores_pulados: pulados.iter().map(|p| p.to_string()).collect(),
                total_horas,
                dias: por_dia
                    .iter()
                    .map(|(dia, (horas, sessoes))| LinhaDia {
                        dia: dia.to_string(),
                        horas: *horas,
                        sessoes: *sessoes,
                    })
                    .collect(),
                custo_usd_total: dados.custo_total,
                modelos: dados.modelos,
                modelos_sem_preco: dados.sem_preco,
                tokens_por_dia: dados
                    .tokens_por_dia
                    .iter()
                    .map(|(dia, tokens)| LinhaDiaTokens {
                        dia: dia.to_string(),
                        tokens: *tokens,
                    })
                    .collect(),
            };
            return Ok(serde_json::to_string_pretty(&saida_json)?);
        }

        Ok(render::renderizar_dashboard(
            "IA atividade",
            &subtitulo,
            &dados.tokens_por_dia,
            &dados.sessoes,
            &dados.modelos,
            dados.custo_total,
            &dados.sem_preco,
            self.weeks,
            !self.no_color,
            Some(self.top),
        ))
    }
}

// Enum dos provedores de IA para o subcomando `ai stats`. Cada variante
// carrega seu próprio `*Args` (e portanto seus campos e defaults).
/// Provedores de IA disponíveis para estatísticas.
#[derive(Subcommand, Debug)]
enum StatsCommands {
    /// Estatísticas do OpenCode (tokens, custo, heatmap).
    Opencode(OpencodeArgs),
    /// Estatísticas do Claude Code (horas, custo, heatmap).
    Claude(ClaudeArgs),
}
```

- [ ] **Step 3: Rodar build, testes e clippy**

Run: `cargo build && cargo test && cargo clippy -- -D warnings`
Expected: build limpo, todos os testes passam, clippy sem warnings.

- [ ] **Step 4: Verificação manual do comando combinado**

Run: `cargo run -- ai stats --help`
Expected: help mostra tanto as opções (`periodo`, `--historico`, `--weeks`, `--top`, `--no-color`, `--json`) quanto os subcomandos (`opencode`, `claude`).

Run: `cargo run -- ai stats`
Expected: dashboard único "IA atividade" com dados combinados (ou nota "(sem dados do OpenCode)"/"(sem dados do Claude Code)" se um dos dois não tiver dados na máquina).

Run: `cargo run -- ai stats --json`
Expected: JSON válido (`python3 -m json.tool` ou `jq .` não falha) com o campo `provedores_pulados`.

Run: `cargo run -- ai stats claude` e `cargo run -- ai stats opencode`
Expected: saída idêntica à de antes desta task (subcomandos individuais não mudaram).

- [ ] **Step 5: Commit**

```bash
git add src/help.rs src/ai/stats.rs
git commit -m "$(cat <<'EOF'
feat(ai): ai stats sem subcomando mostra dashboard combinado

`dev-cli ai stats` (sem opencode/claude) agora mescla os dados dos dois
provedores num só dashboard, pulando o que não tiver dados no período.
Os subcomandos `ai stats opencode`/`ai stats claude` continuam iguais.
EOF
)"
```

---

## Self-Review

**Spec coverage:**
- Dashboard único mesclado → Task 4 (`execute_combinado` + `mesclar_dados`).
- Subcomandos individuais mantidos e inalterados → Tasks 2 e 3 (refatoração sem mudança de comportamento) + verificação manual explícita no Step 4 de cada task.
- Mesmas opções (período, `--historico`, `--json`, `--weeks`, `--top`, `--no-color`) no combinado → `StatsArgs` na Task 4 replica os campos.
- Provedor sem dados é pulado, nota no subtítulo, falha só se os dois vazios → `execute_combinado` na Task 4.
- `DadosProvedor` compartilhado, `carregar_dados` em cada provedor → Tasks 1, 2, 3.
- Teste de mesclagem sem tocar IO → Task 1.

Sem lacunas encontradas.
