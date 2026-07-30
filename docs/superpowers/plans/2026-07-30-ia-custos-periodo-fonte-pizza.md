# IA · custos: período, fonte e pizza — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tela IA · custos com seletor de mês (`‹ mês ›`), filtro de fonte (OpenCode | Claude | Ambos) com custo do Claude calculado no servidor, e pizza de modelos por tokens em CSS puro.

**Architecture:** Abordagem A da spec (`docs/superpowers/specs/2026-07-30-ia-custos-periodo-fonte-pizza-design.md`): o cálculo de custo do Claude (tabela de preços + parse dos JSONL) muda de `crates/cli` para `nucleo::ia_claude`; o endpoint `/api/ia/custos` ganha `?fonte=` e devolve o MESMO shape `CustosIa` já agregado; a UI só escolhe mês/fonte e desenha.

**Tech Stack:** Rust (axum, rusqlite, serde, chrono, walkdir), React 19 + TS (vitest), CSS `conic-gradient` (sem libs novas).

## Global Constraints

- pt-br em structs/funções/variáveis; comentários didáticos (conceito/"porquê") em TODO código novo — é projeto de aprendizado.
- Sem `unwrap()` fora de `#[cfg(test)]`. `Box<dyn Error>` como tipo de erro.
- Clippy-clean (`cargo clippy --workspace` sem warnings); let chains da edition 2024 em vez de `if` aninhados; `cargo fmt --all` antes de cada commit.
- Testes Rust não dependem de `dados/` — fixtures em `tempfile`/strings inline.
- Web: sem dependências novas; `npm test` e `npm run build` verdes antes de dar por pronto.
- Commits: Conventional Commits pt-br (`<tipo>(<escopo>): <resumo imperativo>`).
- A working tree tem o modo escuro NÃO commitado em `web/` — NÃO tocar nem commitar esses arquivos junto (staging sempre por caminho explícito).

---

### Task 1: `nucleo::ia_claude` — preços (mudança de morada)

**Files:**
- Create: `crates/nucleo/src/ia_claude.rs`
- Modify: `crates/nucleo/src/lib.rs` (adicionar `pub mod ia_claude;`)
- Modify: `crates/nucleo/Cargo.toml` (adicionar `walkdir.workspace = true` — usado na Task 2)
- Modify: `crates/cli/src/ai/precos.rs` (vira re-export fino)

**Interfaces:**
- Produces: `nucleo::ia_claude::{Preco, preco_do_modelo, CustoDetalhado, calcular_custo_detalhado, distribuir_custo_proporcional}` — assinaturas EXATAS de `crates/cli/src/ai/precos.rs` hoje: `preco_do_modelo(modelo: &str) -> Option<Preco>`, `calcular_custo_detalhado(modelo: &str, tokens_entrada: i64, tokens_cache_escrita: i64, tokens_cache_leitura: i64, tokens_saida: i64) -> Option<CustoDetalhado>`, `CustoDetalhado::total(&self) -> f64`.

- [ ] **Step 1: Criar `crates/nucleo/src/ia_claude.rs` com o conteúdo integral de `crates/cli/src/ai/precos.rs`**

Copiar o arquivo inteiro (incluindo comentários didáticos e o bloco `#[cfg(test)]` do fim), com um cabeçalho novo:

```rust
// Custos de IA do Claude Code — parte pura, compartilhada pelo CLI
// (`dev-cli ai stats claude`) e pelo dev-server (`/api/ia/custos`).
// Mudou de morada: era `crates/cli/src/ai/precos.rs`; veio para o nucleo
// porque o servidor não pode depender do crate do CLI (a convenção do
// workspace: cálculo puro no nucleo, casca de apresentação nos bins).
```

- [ ] **Step 2: Registrar o módulo e a dependência**

Em `crates/nucleo/src/lib.rs`, na lista de `pub mod` (ordem alfabética):

```rust
pub mod ia_claude;
```

Em `crates/nucleo/Cargo.toml`, junto das outras workspace deps:

```toml
walkdir.workspace = true
```

- [ ] **Step 3: Reduzir `crates/cli/src/ai/precos.rs` a um re-export**

Substituir TODO o conteúdo por:

```rust
// Mudança de morada: o cálculo de preços vive em `nucleo::ia_claude`
// (o dev-server também precisa dele). Este re-export mantém os caminhos
// `crate::ai::precos::*` do CLI funcionando sem mudar nenhum import —
// mesmo padrão do re-export de `horas_sessao` em `render.rs`.
pub use nucleo::ia_claude::{
    CustoDetalhado, Preco, calcular_custo_detalhado, distribuir_custo_proporcional,
    preco_do_modelo,
};
```

- [ ] **Step 4: Rodar a suíte e o clippy**

Run: `cargo test --workspace && cargo clippy --workspace`
Expected: tudo verde (os testes de preços agora rodam dentro do nucleo); nenhum warning. Se o clippy apontar import não usado no CLI, remover o import morto.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/nucleo/src/ia_claude.rs crates/nucleo/src/lib.rs crates/nucleo/Cargo.toml crates/cli/src/ai/precos.rs
git commit -m "refactor(nucleo): move tabela de preços do Claude para nucleo::ia_claude"
```

---

### Task 2: `nucleo::ia_claude` — parse dos JSONL (carregar_sessoes)

**Files:**
- Modify: `crates/nucleo/src/ia_claude.rs`
- Test: mesmo arquivo, bloco `#[cfg(test)]` (usar `tempfile` — já é dev-dependency do workspace; se não for do nucleo, adicionar `tempfile.workspace = true` em `[dev-dependencies]` de `crates/nucleo/Cargo.toml`)

**Interfaces:**
- Consumes: `nucleo::horas_sessao::{Sessao, duracao_sessao}` (já existem).
- Produces:
  - `pub struct UsoSessao { pub modelo: String, pub tokens_entrada: i64, pub tokens_cache_escrita: i64, pub tokens_cache_leitura: i64, pub tokens_saida: i64 }`
  - `pub fn carregar_sessoes(dir: &std::path::Path, mes: &str) -> (Vec<crate::horas_sessao::Sessao>, Vec<UsoSessao>, std::collections::BTreeMap<chrono::NaiveDate, i64>)` — MESMA lógica de `crates/cli/src/ai/claude.rs::carregar_sessoes`, com o diretório como PARÂMETRO (o CLI resolve `~/.claude/projects`, o servidor resolve com a env `DEV_CLI_CLAUDE_PROJETOS_DIR`; a função pura não lê env).

- [ ] **Step 1: Escrever o teste que falha**

No bloco `#[cfg(test)]` de `ia_claude.rs`:

```rust
#[test]
fn carregar_sessoes_le_jsonl_filtra_mes_e_agrega_tokens() {
    let dir = tempfile::tempdir().unwrap();
    // Duas mensagens de julho (mesma sessão) + uma de junho (fora do mês).
    let jsonl = concat!(
        r#"{"timestamp":"2026-07-10T10:00:00-03:00","sessionId":"s1","message":{"model":"claude-sonnet-5","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":1000}}}"#, "\n",
        r#"{"timestamp":"2026-07-10T10:30:00-03:00","sessionId":"s1","message":{"model":"claude-sonnet-5","usage":{"input_tokens":200,"output_tokens":80,"cache_creation_input_tokens":0,"cache_read_input_tokens":2000}}}"#, "\n",
        r#"{"timestamp":"2026-06-01T09:00:00-03:00","sessionId":"s0","message":{"model":"claude-sonnet-5","usage":{"input_tokens":999,"output_tokens":9,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#, "\n",
        "linha invalida que o parser deve pular\n",
    );
    std::fs::create_dir(dir.path().join("projeto-a")).unwrap();
    std::fs::write(dir.path().join("projeto-a/sessao.jsonl"), jsonl).unwrap();

    let (sessoes, usos, tokens_por_dia) = carregar_sessoes(dir.path(), "2026-07");

    assert_eq!(sessoes.len(), 1, "uma sessão em julho");
    assert_eq!(usos.len(), 2, "duas mensagens de assistente em julho");
    let dia = chrono::NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
    // 100+50+10+1000 + 200+80+0+2000 = 3440 tokens no dia 10/07.
    assert_eq!(tokens_por_dia.get(&dia), Some(&3440));
    assert_eq!(usos[0].modelo, "claude-sonnet-5");
}
```

- [ ] **Step 2: Rodar para ver falhar**

Run: `cargo test -p nucleo carregar_sessoes_le_jsonl -v`
Expected: FAIL de compilação — `carregar_sessoes` não existe no nucleo.

- [ ] **Step 3: Mover a implementação do CLI**

Copiar de `crates/cli/src/ai/claude.rs` para `ia_claude.rs`, preservando os comentários didáticos:
- As structs privadas `Uso`, `Mensagem`, `Registro` (linhas ~89–134).
- A struct `UsoSessao` (tornando-a `pub` como já é).
- A função `carregar_sessoes` (linhas ~186–300), com DUAS adaptações: (1) assinatura ganha `dir: &Path` e o corpo usa `WalkDir::new(dir)` no lugar de `WalkDir::new(diretorio_projetos())`; (2) o retorno usa `crate::horas_sessao::Sessao` (no CLI, `render::Sessao` já era só um re-export desse mesmo tipo — nada muda de fato).

Imports novos no topo de `ia_claude.rs`: `std::collections::{BTreeMap, HashMap}`, `std::path::Path`, `chrono::{DateTime, Local, NaiveDate, Utc}`, `serde::Deserialize`, `walkdir::WalkDir`, `crate::horas_sessao::{self, Sessao}`.

- [ ] **Step 4: Rodar o teste até passar**

Run: `cargo test -p nucleo carregar_sessoes_le_jsonl -v`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/nucleo/src/ia_claude.rs crates/nucleo/Cargo.toml
git commit -m "feat(nucleo): carregar_sessoes do Claude Code em ia_claude (dir como parâmetro)"
```

---

### Task 3: CLI consome o nucleo (remover duplicação)

**Files:**
- Modify: `crates/cli/src/ai/claude.rs`
- Modify: `crates/servidor/src/ia.rs` (trocar `carregar_sessoes_claude` local pelo do nucleo)

**Interfaces:**
- Consumes: `nucleo::ia_claude::{carregar_sessoes, UsoSessao}` (Task 2).

- [ ] **Step 1: Adaptar o CLI**

Em `crates/cli/src/ai/claude.rs`:
- Apagar as structs `Uso`, `Mensagem`, `Registro`, `UsoSessao` e a função `carregar_sessoes` locais.
- No lugar, `pub use nucleo::ia_claude::UsoSessao;` e um wrapper fino que preserva o call-site atual (o `execute()`/`carregar_dados` chamam `carregar_sessoes(mes)` sem dir):

```rust
// A resolução do diretório (~/.claude/projects) é responsabilidade da
// casca; a função pura do nucleo recebe o caminho pronto — assim o
// dev-server pode apontar para outro diretório (env de teste) sem que a
// lógica de parse mude de comportamento.
fn carregar_sessoes(
    mes: &str,
) -> (
    Vec<render::Sessao>,
    Vec<UsoSessao>,
    BTreeMap<NaiveDate, i64>,
) {
    nucleo::ia_claude::carregar_sessoes(&diretorio_projetos(), mes)
}
```

`diretorio_projetos()` continua onde está. Ajustar imports que sobrarem/faltarem.

- [ ] **Step 2: Adaptar o servidor (só a parte de horas — o custo vem na Task 4)**

Em `crates/servidor/src/ia.rs`: apagar `RegistroClaude` e `carregar_sessoes_claude`; em `calcular_horas_claude`, trocar a chamada por:

```rust
let (sessoes, _usos, _tokens_por_dia) =
    nucleo::ia_claude::carregar_sessoes(&diretorio_projetos_claude(), mes);
```

(`diretorio_projetos_claude()` com a env `DEV_CLI_CLAUDE_PROJETOS_DIR` continua — é exatamente o motivo do parâmetro `dir`.)

- [ ] **Step 3: Suíte + clippy do workspace**

Run: `cargo test --workspace && cargo clippy --workspace`
Expected: verde e sem warnings. A saída de `cargo run -p dev-cli -- ai stats claude` deve permanecer idêntica (mudança de morada, não de comportamento) — rodar uma vez e conferir a olho que o dashboard imprime.

- [ ] **Step 4: Commit**

```bash
cargo fmt --all
git add crates/cli/src/ai/claude.rs crates/servidor/src/ia.rs
git commit -m "refactor(ai): CLI e servidor consomem carregar_sessoes do nucleo"
```

---

### Task 4: API — `?fonte=` com agregação do Claude e mescla em "ambos"

**Files:**
- Modify: `crates/servidor/src/ia.rs`
- Test: `crates/servidor/src/api.rs` (seção `#[cfg(test)]` existente — seguir o padrão `get_json(criar_rotas(estado_teste()), ...)`)

**Interfaces:**
- Consumes: `nucleo::ia_claude::{carregar_sessoes, calcular_custo_detalhado, UsoSessao}`; `nucleo::metricas::intensidade_log`; helpers existentes `montar_heatmap`, `calcular_streak`, `calcular_melhor_streak`, `resposta_vazia`.
- Produces: `GET /api/ia/custos?mes=YYYY-MM&fonte=opencode|claude|ambos` (default `ambos`), mesmo shape `CustosIa`. `fonte` inválida → `400`.

- [ ] **Step 1: Testes que falham (API)**

No `#[cfg(test)]` de `api.rs`, seguindo o padrão dos testes vizinhos (que já setam `DEV_CLI_OPENCODE_DB` para caminho inexistente). Fixture do Claude: um tempdir com um `.jsonl` mínimo apontado via `DEV_CLI_CLAUDE_PROJETOS_DIR`:

```rust
#[tokio::test]
async fn ia_custos_fonte_claude_calcula_custo_dos_transcritos() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("s.jsonl"),
        concat!(
            r#"{"timestamp":"2026-07-10T10:00:00-03:00","sessionId":"s1","message":{"model":"claude-sonnet-5","usage":{"input_tokens":1000000,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            "\n",
        ),
    )
    .unwrap();
    // SAFETY dos testes vizinhos: env por processo; seguir o mesmo padrão
    // de serialização de testes usado no arquivo (ver como os testes de
    // ia_custos existentes lidam com env — imitar).
    unsafe { std::env::set_var("DEV_CLI_CLAUDE_PROJETOS_DIR", dir.path()) };
    unsafe { std::env::set_var("DEV_CLI_OPENCODE_DB", "/caminho/inexistente.db") };

    let (status, json) =
        get_json(criar_rotas(estado_teste()), "/api/ia/custos?mes=2026-07&fonte=claude").await;
    assert_eq!(status, 200);
    assert_eq!(json["disponivel"], true);
    assert_eq!(json["tokens"], 1_000_000);
    // 1M tokens de entrada do sonnet: custo > 0 vindo da tabela de preços.
    assert!(json["custo_usd"].as_f64().unwrap() > 0.0);
    assert_eq!(json["modelos"][0]["provedor"], "claude-code");
}

#[tokio::test]
async fn ia_custos_fonte_invalida_devolve_400() {
    let (status, _) =
        get_json(criar_rotas(estado_teste()), "/api/ia/custos?fonte=banana").await;
    assert_eq!(status, 400);
}
```

Acrescentar também um teste `ia_custos_fonte_ambos_soma_e_mescla_modelos` no mesmo molde (OpenCode inexistente + JSONL presente → `ambos` devolve os números do Claude; `disponivel == true` porque UMA das fontes tem dados).

- [ ] **Step 2: Rodar para ver falhar**

Run: `cargo test -p servidor ia_custos_fonte -v`
Expected: FAIL — `fonte` é ignorada hoje (não existe no `ParamsCustos`), então `fonte=banana` devolve 200 e `fonte=claude` devolve números do OpenCode.

- [ ] **Step 3: Implementar em `ia.rs`**

1. Params + enum:

```rust
/// Fonte dos dados: qual(is) origem(ns) entra(m) na agregação.
#[derive(Clone, Copy, PartialEq)]
enum Fonte {
    Opencode,
    Claude,
    Ambos,
}

impl Fonte {
    /// Parse do query param. `None` (ausente) = Ambos, o default da spec.
    /// Valor desconhecido → `Err` com a mensagem que vira o corpo do 400.
    fn parse(texto: Option<&str>) -> Result<Fonte, String> {
        match texto {
            None | Some("ambos") => Ok(Fonte::Ambos),
            Some("opencode") => Ok(Fonte::Opencode),
            Some("claude") => Ok(Fonte::Claude),
            Some(outro) => Err(format!(
                "fonte inválida: {outro:?} (esperado: opencode, claude ou ambos)"
            )),
        }
    }
}
```

`ParamsCustos` ganha `pub fonte: Option<String>`. No handler `custos`, `Fonte::parse(params.fonte.as_deref()).map_err(|m| (StatusCode::BAD_REQUEST, m))?`.

2. Agregado do Claude (nova função, ao lado de `agregar_opencode`):

```rust
/// Agregado do mês da fonte Claude Code: tokens, custo (tabela de preços
/// do nucleo — a MESMA do `dev-cli ai stats claude`), cache %, tokens por
/// dia (para o heatmap) e ranking de modelos com provedor "claude-code"
/// (distingue do "anthropic" do OpenCode na mescla de `ambos`).
struct AgregadoClaude {
    tokens: i64,
    custo_usd: f64,
    cache_pct: f64,
    tokens_por_dia: BTreeMap<NaiveDate, i64>,
    modelos: Vec<ModeloCusto>,
}

fn agregar_claude(mes: &str) -> AgregadoClaude {
    let (_sessoes, usos, tokens_por_dia) =
        nucleo::ia_claude::carregar_sessoes(&diretorio_projetos_claude(), mes);

    let mut tokens_total = 0i64;
    let mut cache_total = 0i64;
    let mut custo_total = 0.0f64;
    // Agrupamento por modelo: BTreeMap para saída determinística nos testes.
    let mut por_modelo: BTreeMap<String, ModeloCusto> = BTreeMap::new();

    for uso in &usos {
        let tokens_uso = uso.tokens_entrada
            + uso.tokens_cache_escrita
            + uso.tokens_cache_leitura
            + uso.tokens_saida;
        tokens_total += tokens_uso;
        cache_total += uso.tokens_cache_escrita + uso.tokens_cache_leitura;

        let custo_uso = nucleo::ia_claude::calcular_custo_detalhado(
            &uso.modelo,
            uso.tokens_entrada,
            uso.tokens_cache_escrita,
            uso.tokens_cache_leitura,
            uso.tokens_saida,
        )
        .map(|c| c.total())
        .unwrap_or(0.0);
        custo_total += custo_uso;

        let entrada = por_modelo.entry(uso.modelo.clone()).or_insert_with(|| ModeloCusto {
            modelo: uso.modelo.clone(),
            provedor: "claude-code".to_string(),
            sessoes: 0,
            tokens: 0,
            custo_usd: 0.0,
        });
        entrada.sessoes += 1; // uma "entrada" por mensagem, como o CLI conta usos
        entrada.tokens += tokens_uso;
        entrada.custo_usd += custo_uso;
    }

    let cache_pct = if tokens_total > 0 {
        cache_total as f64 * 100.0 / tokens_total as f64
    } else {
        0.0
    };
    let mut modelos: Vec<ModeloCusto> = por_modelo.into_values().collect();
    modelos.sort_by(|a, b| b.tokens.cmp(&a.tokens));

    AgregadoClaude { tokens: tokens_total, custo_usd: custo_total, cache_pct, tokens_por_dia, modelos }
}
```

3. `agregar_opencode` passa a DEVOLVER também o `tokens_por_dia` cru (guardar o `BTreeMap` que hoje é consumido por `montar_heatmap` dentro dela) — mover a chamada de `montar_heatmap` para o caller, porque em `ambos` a soma por dia acontece ANTES da escala de intensidade.

4. No handler `custos`, montar por fonte:
   - `Opencode`: comportamento atual (heatmap do mapa do OpenCode; `disponivel` = banco lido).
   - `Claude`: campos vindos de `agregar_claude`; `disponivel` = `!usos.is_empty()` (equivalente: `tokens > 0` OU sessões não vazias).
   - `Ambos`: somar `tokens`, `custo_usd`; `cache_pct` recalculado sobre os totais somados (cache somado / tokens somados — NÃO média de percentuais); `tokens_por_dia` mesclado com `entry().or_insert(0) +=`; `modelos` = concatenação (provedores distintos garantem chaves distintas) reordenada por tokens desc; `disponivel` = OpenCode OU Claude com dados.
   - Heatmap/streaks: `montar_heatmap(&mapa_da_fonte, &mes)` + `calcular_streak`/`calcular_melhor_streak` sobre o resultado, para qualquer fonte.
   - `claude_horas_*`/`offset_semana_dia1`: inalterados (sempre calculados, como hoje).

- [ ] **Step 4: Rodar até passar + suíte inteira**

Run: `cargo test -p servidor ia_custos -v && cargo test --workspace && cargo clippy --workspace`
Expected: PASS, sem warnings.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/servidor/src/ia.rs crates/servidor/src/api.rs
git commit -m "feat(servidor): parametro fonte em /api/ia/custos com custo do Claude e mescla ambos"
```

---

### Task 5: Web — `api.ts` com `fonte` + tipo `FonteIa`

**Files:**
- Modify: `web/src/tipos.ts`
- Modify: `web/src/api.ts`
- Test: `web/src/api.test.ts`

**Interfaces:**
- Produces: `export type FonteIa = 'opencode' | 'claude' | 'ambos'` (tipos.ts); `buscarCustosIa(mes?: string, fonte?: FonteIa): Promise<CustosIa>` — monta a query com os params presentes.

- [ ] **Step 1: Teste que falha (padrão dos testes existentes de `api.test.ts` — mock de fetch)**

```ts
it('buscarCustosIa monta a query com mes e fonte', async () => {
  const fetchMock = vi.fn().mockResolvedValue({
    ok: true,
    json: () => Promise.resolve({}),
  })
  vi.stubGlobal('fetch', fetchMock)

  await buscarCustosIa('2026-06', 'claude')
  expect(fetchMock).toHaveBeenCalledWith('/api/ia/custos?mes=2026-06&fonte=claude')

  await buscarCustosIa()
  expect(fetchMock).toHaveBeenLastCalledWith('/api/ia/custos')
})
```

- [ ] **Step 2: Rodar para ver falhar**

Run: `cd web && npx vitest run src/api.test.ts`
Expected: FAIL — assinatura atual só tem `mes` e sempre formata `?mes=`.

- [ ] **Step 3: Implementar**

Em `tipos.ts` (perto de `CustosIa`):

```ts
/// Fonte dos dados da tela IA · custos — espelha o query param `fonte`
/// de /api/ia/custos ('ambos' é o default do servidor E da UI).
export type FonteIa = 'opencode' | 'claude' | 'ambos'
```

Em `api.ts`, trocar `buscarCustosIa`:

```ts
/// Pacote da tela IA · custos. `mes` = "YYYY-MM" (default: mês atual no
/// servidor); `fonte` filtra a origem (default 'ambos' no servidor —
/// omitimos o param quando o caller não passa, mantendo a URL mínima).
/// URLSearchParams cuida do encoding dos dois params.
/// docs: https://developer.mozilla.org/docs/Web/API/URLSearchParams
export function buscarCustosIa(mes?: string, fonte?: FonteIa): Promise<CustosIa> {
  const params = new URLSearchParams()
  if (mes !== undefined) params.set('mes', mes)
  if (fonte !== undefined) params.set('fonte', fonte)
  const query = params.size > 0 ? `?${params}` : ''
  return buscarJson(`/api/ia/custos${query}`)
}
```

(Import de `FonteIa` na lista de tipos do topo.)

- [ ] **Step 4: Rodar até passar**

Run: `cd web && npx vitest run src/api.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/tipos.ts web/src/api.ts web/src/api.test.ts
git commit -m "feat(web): buscarCustosIa aceita mes e fonte (FonteIa)"
```

---

### Task 6: Web — seletor `‹ mês ›` e segmented de fonte em `IaCustos.tsx`

**Files:**
- Modify: `web/src/componentes/IaCustos.tsx`
- Modify: `web/src/formato.ts` (helper novo de mês)
- Test: `web/src/componentes/IaCustos.test.tsx`, `web/src/formato.test.ts`

**Interfaces:**
- Consumes: `buscarCustosIa(mes, fonte)` (Task 5).
- Produces (formato.ts): `mesAnterior(mes: string): string`, `mesSeguinte(mes: string): string`, `mesPorExtenso(mes: string): string` ("2026-07" → "julho de 2026"), `mesAtual(): string` ("YYYY-MM" local). Funções puras de string — testáveis sem DOM.

- [ ] **Step 1: Testes que falham (formato)**

Em `formato.test.ts`:

```ts
describe('navegação de meses', () => {
  it('mesAnterior e mesSeguinte cruzam a virada de ano', () => {
    expect(mesAnterior('2026-01')).toBe('2025-12')
    expect(mesSeguinte('2025-12')).toBe('2026-01')
    expect(mesAnterior('2026-07')).toBe('2026-06')
  })
  it('mesPorExtenso formata em pt-BR', () => {
    expect(mesPorExtenso('2026-07')).toBe('julho de 2026')
  })
})
```

Run: `cd web && npx vitest run src/formato.test.ts` — Expected: FAIL (funções não existem).

- [ ] **Step 2: Implementar os helpers em `formato.ts`**

```ts
/// Navegação de meses no formato "YYYY-MM" — aritmética direta em números
/// para não depender de Date (fuso/dia do mês não importam aqui).
export function mesAnterior(mes: string): string {
  const [ano, m] = mes.split('-').map(Number)
  return m === 1 ? `${ano - 1}-12` : `${ano}-${String(m - 1).padStart(2, '0')}`
}

export function mesSeguinte(mes: string): string {
  const [ano, m] = mes.split('-').map(Number)
  return m === 12 ? `${ano + 1}-01` : `${ano}-${String(m + 1).padStart(2, '0')}`
}

/// "2026-07" → "julho de 2026". Intl faz a tradução do nome do mês; o
/// dia 2 evita qualquer surpresa de fuso (dia 1 UTC pode cair no mês
/// anterior em fusos negativos como o do Brasil).
/// docs: https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Intl/DateTimeFormat
export function mesPorExtenso(mes: string): string {
  const data = new Date(`${mes}-02T12:00:00`)
  return new Intl.DateTimeFormat('pt-BR', { month: 'long', year: 'numeric' }).format(data)
}

/// Mês atual local em "YYYY-MM" — o estado inicial do seletor da tela IA.
export function mesAtual(): string {
  const agora = new Date()
  return `${agora.getFullYear()}-${String(agora.getMonth() + 1).padStart(2, '0')}`
}
```

Run: `cd web && npx vitest run src/formato.test.ts` — Expected: PASS.

- [ ] **Step 3: Testes que falham (tela)**

Em `IaCustos.test.tsx` (seguir o padrão de mock de fetch dos testes existentes do arquivo):

```ts
it('seta ‹ busca o mês anterior e › fica desabilitada no mês atual', async () => {
  render(<IaCustos />)
  await screen.findByText(/carregando|custo no mês/i)
  const voltar = screen.getByRole('button', { name: /mês anterior/i })
  const avancar = screen.getByRole('button', { name: /mês seguinte/i })
  expect(avancar).toBeDisabled()
  fireEvent.click(voltar)
  await waitFor(() => {
    const chamadas = (fetch as Mock).mock.calls.map((c) => String(c[0]))
    expect(chamadas.some((u) => u.includes(`mes=${mesAnterior(mesAtual())}`))).toBe(true)
  })
})

it('segmented de fonte refaz o fetch e esconde horas com opencode', async () => {
  render(<IaCustos />)
  fireEvent.click(await screen.findByLabelText('OpenCode'))
  await waitFor(() => {
    const chamadas = (fetch as Mock).mock.calls.map((c) => String(c[0]))
    expect(chamadas.some((u) => u.includes('fonte=opencode'))).toBe(true)
  })
  expect(screen.queryByText(/horas com claude/i)).not.toBeInTheDocument()
})
```

Run: `cd web && npx vitest run src/componentes/IaCustos.test.tsx` — Expected: FAIL.

- [ ] **Step 4: Implementar na tela**

Em `IaCustos.tsx`:
- Estados novos: `const [mes, setMes] = useState(mesAtual())` e `const [fonte, setFonte] = useState<FonteIa>('ambos')`.
- O `useEffect` do fetch ganha `[mes, fonte]` nas deps e chama `buscarCustosIa(mes, fonte)`; manter o padrão `ativo` de cancelamento e NÃO limpar `dados` ao refazer (dados stale ficam na tela durante o fetch — padrão do portal).
- Header: no lugar do `mes` cru no subtítulo, o seletor:

```tsx
<span className="seletor-mes">
  <button type="button" className="btn btn-ghost" aria-label="Mês anterior"
    onClick={() => setMes(mesAnterior(mes))}>‹</button>
  <span className="seletor-mes-rotulo">{mesPorExtenso(mes)}</span>
  <button type="button" className="btn btn-ghost" aria-label="Mês seguinte"
    disabled={mes === mesAtual()}
    onClick={() => setMes(mesSeguinte(mes))}>›</button>
</span>
```

- Segmented de fonte (mesmo padrão `.seg`/`.seg-opt`/`.sr-only` com radios da tela Configuração — inclusive o radio focável por teclado):

```tsx
<div className="seg" role="radiogroup" aria-label="Fonte dos dados">
  {(['opencode', 'claude', 'ambos'] as const).map((op) => (
    <label key={op} className={`seg-opt ${fonte === op ? 'selected' : ''}`}>
      <input type="radio" name="fonte-ia" className="sr-only" value={op}
        checked={fonte === op} onChange={() => setFonte(op)}
        aria-label={op === 'opencode' ? 'OpenCode' : op === 'claude' ? 'Claude' : 'Ambos'} />
      {op === 'opencode' ? 'OpenCode' : op === 'claude' ? 'Claude' : 'Ambos'}
    </label>
  ))}
</div>
```

- Condicionais: KPI "Horas com Claude" e seção "Horas por semana" só quando `fonte !== 'opencode' && dados.claude_disponivel`. Subtítulo do header reflete a fonte (`OpenCode`, `Claude Code` ou `OpenCode + Claude`).
- Estado vazio por fonte (spec §3): `!dados.disponivel` → mensagem conforme `fonte` (opencode → texto atual do banco; claude → "nenhuma sessão do Claude Code no mês"; ambos → "nenhuma das fontes tem dados neste mês"), SEM esconder o seletor de mês/fonte (o header da tela renderiza sempre).
- CSS: classe `.seletor-mes` (flex, gap 6px, rótulo tnum) em `index.css` — usar tokens (`var(--color-...)`), sem hexes.

- [ ] **Step 5: Rodar até passar + suíte web inteira**

Run: `cd web && npm test`
Expected: PASS (novos e antigos).

- [ ] **Step 6: Commit**

```bash
git add web/src/componentes/IaCustos.tsx web/src/componentes/IaCustos.test.tsx web/src/formato.ts web/src/formato.test.ts web/src/index.css
git commit -m "feat(web): seletor de mês e filtro de fonte na tela IA · custos"
```

---

### Task 7: Web — pizza de modelos (`PizzaModelos.tsx`)

**Files:**
- Create: `web/src/componentes/PizzaModelos.tsx`
- Test: `web/src/componentes/PizzaModelos.test.tsx`
- Modify: `web/src/componentes/IaCustos.tsx` (renderizar a pizza na seção "Por modelo"), `web/src/formato.ts` (extrair `corDoModelo`), `web/src/index.css` (classes da pizza)

**Interfaces:**
- Consumes: `ModeloCusto` de `tipos.ts`.
- Produces: `corDoModelo(modelo: string, indice: number): string` (formato.ts — usada pela pizza E pelas barras já existentes da tabela, extraída da lógica inline atual de `IaCustos.tsx`); `fatiasPizza(modelos: ModeloCusto[]): { modelo: string; pct: number; cor: string }[]`; componente `<PizzaModelos modelos={ModeloCusto[]} />`.

- [ ] **Step 1: Testes que falham (função pura)**

```ts
import { fatiasPizza } from './PizzaModelos'

const modelo = (nome: string, tokens: number) =>
  ({ modelo: nome, provedor: 'x', sessoes: 1, tokens, custo_usd: 0 })

describe('fatiasPizza', () => {
  it('ordena por tokens desc e calcula percentuais', () => {
    const fatias = fatiasPizza([modelo('haiku', 250), modelo('sonnet', 750)])
    expect(fatias.map((f) => f.modelo)).toEqual(['sonnet', 'haiku'])
    expect(fatias.map((f) => f.pct)).toEqual([75, 25])
  })
  it('modelo com 0 tokens não gera fatia (regra do CLI)', () => {
    expect(fatiasPizza([modelo('a', 100), modelo('b', 0)])).toHaveLength(1)
  })
  it('sem tokens, sem fatias', () => {
    expect(fatiasPizza([])).toEqual([])
  })
  it('sonnet/opus/haiku têm cores fixas do DS', () => {
    const fatias = fatiasPizza([modelo('claude-sonnet-5', 1), modelo('claude-opus-5', 1)])
    expect(fatias.find((f) => f.modelo.includes('sonnet'))?.cor).toBe('var(--color-accent)')
    expect(fatias.find((f) => f.modelo.includes('opus'))?.cor).toBe('var(--sev-vermelho)')
  })
})
```

Run: `cd web && npx vitest run src/componentes/PizzaModelos.test.tsx` — Expected: FAIL.

- [ ] **Step 2: Implementar**

Em `formato.ts` (extraindo a lógica de cor que hoje vive inline nas barras de `IaCustos.tsx` — substituir o uso inline pela função):

```ts
/// Cor de um modelo nos gráficos (barras e pizza): os três da casa têm
/// cor fixa (handoff §6); os demais ciclam uma paleta de variáveis do DS
/// pelo índice — variáveis, nunca hexes, para o modo escuro herdar.
const CICLO_CORES_MODELO = [
  'var(--color-accent-600)',
  'var(--color-neutral-500)',
  'var(--color-accent-300)',
  'var(--color-neutral-700)',
  'var(--color-accent-800)',
]

export function corDoModelo(modelo: string, indice: number): string {
  if (modelo.includes('sonnet')) return 'var(--color-accent)'
  if (modelo.includes('opus')) return 'var(--sev-vermelho)'
  if (modelo.includes('haiku')) return 'var(--sev-verde)'
  return CICLO_CORES_MODELO[indice % CICLO_CORES_MODELO.length]
}
```

`PizzaModelos.tsx`:

```tsx
// Pizza de modelos por tokens — versão web do `renderizar_pizza` do CLI
// (mesma métrica e mesma regra: fatias por tokens, desc, 0 tokens fora).
// Sem lib de gráficos: um único <div> redondo com `conic-gradient`, que
// pinta setores por ângulo — CSS resolve `var()` dentro do gradiente,
// então as cores seguem o tema claro/escuro sozinhas.
// docs: https://developer.mozilla.org/docs/Web/CSS/gradient/conic-gradient

import type { ModeloCusto } from '../tipos'
import { corDoModelo, formatarNumero } from '../formato'

export interface FatiaPizza {
  modelo: string
  pct: number
  cor: string
}

/// Parte pura: modelos -> fatias com percentual e cor, ordenadas desc.
/// Exportada separada do componente para testar sem DOM.
export function fatiasPizza(modelos: ModeloCusto[]): FatiaPizza[] {
  const comTokens = modelos.filter((m) => m.tokens > 0)
  const total = comTokens.reduce((soma, m) => soma + m.tokens, 0)
  if (total === 0) return []
  return comTokens
    .slice()
    .sort((a, b) => b.tokens - a.tokens)
    .map((m, i) => ({
      modelo: m.modelo,
      pct: (m.tokens * 100) / total,
      cor: corDoModelo(m.modelo, i),
    }))
}

export function PizzaModelos({ modelos }: { modelos: ModeloCusto[] }) {
  const fatias = fatiasPizza(modelos)
  if (fatias.length === 0) return null

  // conic-gradient recebe paradas cumulativas: cada fatia vai do fim da
  // anterior até `acumulado + pct` — o reduce monta a lista de setores.
  let acumulado = 0
  const setores = fatias.map((f) => {
    const inicio = acumulado
    acumulado += f.pct
    return `${f.cor} ${inicio}% ${acumulado}%`
  })

  return (
    <div className="pizza-wrap">
      <div
        className="pizza"
        role="img"
        aria-label={`Distribuição de tokens: ${fatias.map((f) => `${f.modelo} ${f.pct.toFixed(0)}%`).join(', ')}`}
        style={{ background: `conic-gradient(${setores.join(', ')})` }}
      />
      <ul className="pizza-legenda">
        {fatias.map((f) => (
          <li key={f.modelo}>
            <span className="pizza-cor" style={{ background: f.cor }} />
            <span className="pizza-nome">{f.modelo}</span>
            <span className="pizza-pct">{f.pct.toFixed(1).replace('.', ',')}%</span>
          </li>
        ))}
      </ul>
    </div>
  )
}
```

`index.css` (com tokens, hairlines — estética Classical):

```css
/* ─── Pizza de modelos (IA · custos) ─────────────────────────────────
   conic-gradient no círculo; legenda ao lado com bolinha/nome/percentual.
   Cores vêm por var() — o modo escuro troca os tokens e a pizza segue. */
.pizza-wrap { display: flex; align-items: center; gap: var(--space-4); }
.pizza {
  width: 160px; height: 160px; border-radius: 50%;
  border: 1px solid var(--color-divider);
  flex-shrink: 0;
}
.pizza-legenda { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
.pizza-legenda li { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.pizza-cor { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }
.pizza-nome { font-family: var(--font-mono); }
.pizza-pct { color: var(--color-neutral-600); font-feature-settings: 'tnum'; margin-left: auto; }
```

Em `IaCustos.tsx`, na seção "Por modelo": envolver tabela + pizza num grid (`display: grid; grid-template-columns: 1fr auto; gap: var(--space-4)` via classe `.modelos-grid`; em `max-width: 900px` empilha com `grid-template-columns: 1fr`) e renderizar `<PizzaModelos modelos={dados.modelos} />`. Substituir a lógica de cor inline das barras pela `corDoModelo` (mesmo índice pós-ordenação da lista — a API já manda desc, manter).

- [ ] **Step 3: Rodar até passar + suíte + build**

Run: `cd web && npm test && npm run build`
Expected: PASS e build ok.

- [ ] **Step 4: Commit**

```bash
git add web/src/componentes/PizzaModelos.tsx web/src/componentes/PizzaModelos.test.tsx web/src/componentes/IaCustos.tsx web/src/formato.ts web/src/index.css
git commit -m "feat(web): pizza de modelos por tokens na tela IA · custos"
```

---

### Task 8: Verificação final de ponta a ponta

**Files:** nenhum novo (só conferência).

- [ ] **Step 1: Gates completos**

Run:
```bash
cargo fmt --all -- --check && cargo test --workspace && cargo clippy --workspace -- -D warnings
cd web && npm test && npm run build
```
Expected: tudo verde, zero warnings.

- [ ] **Step 2: Conferência manual**

Subir `cargo run -p servidor -- --db /tmp/dev.db` + `cd web && npm run dev`; na tela IA · custos conferir: navegação de mês (‹ ativa, › desabilitada no atual), os três valores do segmented refazendo o fetch, horas do Claude sumindo com OpenCode, pizza coerente com a tabela, e tudo legível **nos dois temas** (toggle ◐). Conferir também `curl 'localhost:8787/api/ia/custos?fonte=banana'` → 400.

- [ ] **Step 3: Nota de re-sync**

Não executar o re-sync agora; apenas conferir que `.design-sync/NOTES.md` menciona atualizar o stub de fetch de `previews/IaCustos.tsx` quando o re-sync acontecer (a spec §Fora de escopo cobre; se a menção não existir, adicionar um bullet).
