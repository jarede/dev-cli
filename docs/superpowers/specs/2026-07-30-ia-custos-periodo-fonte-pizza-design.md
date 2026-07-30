# IA · custos: seletor de período, filtro de fonte e pizza de modelos — design

Data: 2026-07-30 · Status: aprovado em conversa (Abordagem A — filtro no servidor)

## Objetivo

Na tela **IA · custos** do portal: navegar entre meses, escolher a fonte dos
dados (**OpenCode | Claude | Ambos**) — incluindo o **custo do Claude Code**
calculado como no `dev-cli ai stats claude` — e um **gráfico de pizza dos
modelos por tokens**, como o do CLI.

## Decisões já tomadas

- **Abordagem A**: filtro e agregação no **servidor**. A API mantém o shape
  `CustosIa` e ganha `?fonte=`; a UI continua "burra" (refetch por troca —
  dados são SQLite/JSONL locais, barato) e a escala de intensidade continua
  única em `nucleo::metricas::intensidade_log`. Descartada a resposta
  segmentada com combinação no cliente (duplicaria a curva de intensidade em
  TS).
- Período: **setas `‹ mês ›`** (sem dropdown). Fonte: segmented com default
  **Ambos**. Em "ambos", **soma tudo** (KPIs somados, modelos mesclados numa
  tabela só, heatmap soma tokens por dia antes da escala).
- Pizza **por tokens** (mesma métrica do CLI), fatias ordenadas desc, CSS
  `conic-gradient` — sem lib de gráficos.

## 1. Rust — custo do Claude vai para o `nucleo`

Pré-requisito: o `servidor` não pode depender de `crates/cli`. Mover para um
novo módulo `nucleo::ia_claude` (sem dependência de terminal):

- A tabela de preços e `calcular_custo_detalhado` de
  `crates/cli/src/ai/precos.rs`.
- O parse **completo** dos JSONL do Claude Code (a struct `Registro` de
  `crates/cli/src/ai/claude.rs`, com `message.usage`/modelo — não só
  timestamp+sessionId como o `RegistroClaude` atual do servidor), e a
  agregação sessões → tokens/custo por modelo.

`crates/cli` passa a importar do nucleo; **comportamento idêntico** (saída do
`ai stats claude` não muda), testes existentes movem junto. O
`carregar_sessoes_claude` do `crates/servidor/src/ia.rs` é substituído pelo
carregador do nucleo (horas continuam vindo de `nucleo::horas_sessao`, com o
mesmo clamp).

O que fica no CLI: renderização (pizza ASCII, heatmap ANSI, cores) — casca de
apresentação, como manda a arquitetura.

## 2. API — `GET /api/ia/custos?mes=YYYY-MM&fonte=opencode|claude|ambos`

- `fonte` default = `ambos`. Valor inválido → 400 com mensagem clara.
- Mesmo shape `CustosIa`; os campos **refletem a fonte pedida**:
  - `tokens`, `custo_usd`, `cache_pct` — da fonte (Claude: tokens/custo dos
    transcritos via `nucleo::ia_claude`; cache_pct = cache read+write sobre o
    total, como no CLI).
  - `heatmap`/`streak_dias`/`melhor_streak_dias` — tokens por dia da fonte;
    em `ambos`, soma por dia ANTES de `intensidade_log`.
  - `modelos` — ranking por tokens desc; em `ambos`, mescla das duas fontes
    (chave = modelo+provedor; Claude entra com provedor `"claude-code"` para
    não colidir com o `anthropic` do OpenCode).
  - `disponivel` — true se a(s) fonte(s) pedida(s) têm dados no mês:
    `opencode` → banco lido; `claude` → há sessões; `ambos` → qualquer uma.
  - `claude_disponivel`/`claude_horas_*` — inalterados (sempre calculados;
    a UI decide exibir).
- `mes` já existe e continua igual.
- Testes (padrão dos testes atuais de `api.rs`): fixtures pequenos — SQLite
  do OpenCode e diretório de JSONL forjados via `DEV_CLI_OPENCODE_DB` /
  `DEV_CLI_CLAUDE_PROJETOS_DIR` — cobrindo cada `fonte`, a mescla de modelos
  em `ambos`, e `fonte` inválida (400).

## 3. UI — header da tela (`IaCustos.tsx`)

- **Seletor de período**: `‹ julho de 2026 ›` no header. `›` desabilitada
  quando o mês exibido é o atual; sem limite para trás. Formato por extenso
  via `Intl.DateTimeFormat('pt-BR', { month: 'long', year: 'numeric' })`.
  Estado `mes` ("YYYY-MM") no componente.
- **Filtro de fonte**: segmented control (classes `.seg`/`.seg-opt`, mesmo
  padrão da tela Configuração) com `OpenCode | Claude | Ambos`, default
  **Ambos**. Estado `fonte` no componente.
- Trocar mês ou fonte refaz o fetch (`buscarCustosIa(mes, fonte)` — assinatura
  de `api.ts` ganha o segundo parâmetro). Durante o fetch, dados stale
  permanecem na tela (padrão do portal); erro → banner, como hoje.
- O subtítulo do header (hoje "2026-07 · OpenCode · câmbio …") passa a
  refletir a fonte selecionada.
- Seções **"Horas com Claude"** (KPI) e **"Horas por semana"** aparecem só
  quando `fonte !== 'opencode'` e `claude_disponivel` (horas são dado
  exclusivo do Claude). Com `fonte === 'claude'` e OpenCode fora da conta, os
  KPIs de custo/tokens mostram os números do Claude — mesma moldura.
- `tipos.ts`: sem campo novo (o shape não muda); `api.ts` ganha o parâmetro
  `fonte`.
- **Estado vazio por fonte**: a mensagem de `!disponivel` (hoje fixa sobre o
  banco do OpenCode) passa a depender da fonte — `opencode` → mensagem atual;
  `claude` → "nenhuma sessão do Claude Code no mês"; `ambos` → genérica
  ("nenhuma das fontes tem dados neste mês"). O seletor de mês continua
  visível no estado vazio (senão não dá para sair de um mês sem dados).

## 4. UI — pizza de modelos (`conic-gradient`)

- Novo bloco na seção "Por modelo", ao lado da tabela (grid; em telas
  estreitas empilha): círculo de ~160px desenhado com
  `background: conic-gradient(...)` construído a partir das fatias.
- Fatias = **tokens por modelo**, ordenadas desc (mesma regra do CLI:
  modelos com 0 tokens não geram fatia).
- Legenda: bolinha da cor da fatia + nome do modelo em mono 12px +
  percentual tnum.
- **Cores por variável CSS** (funcionam nos dois temas): sonnet =
  `var(--color-accent)`, opus = `var(--sev-vermelho)`, haiku =
  `var(--sev-verde)`; demais modelos ciclam `--color-accent-600`,
  `--color-neutral-500`, `--color-accent-300`, `--color-neutral-700`,
  `--color-accent-800` (paleta de 8, como o CLI). Mesmo mapeamento das
  barras existentes da tabela — extrair a função de cor para reuso entre
  barra e pizza.
- Detalhe técnico: `conic-gradient` não aceita `var()` resolvida por fatia
  dentro de string dinâmica montada em JS — montar o gradiente com
  `getComputedStyle`? **Não**: usar as próprias variáveis inline no template
  (`conic-gradient(var(--color-accent) 0 62%, ...)`) — CSS resolve `var()`
  dentro de `conic-gradient` normalmente.
- Componente pequeno e testável: `PizzaModelos.tsx` (props: `modelos`),
  exportando também a função pura `fatiasPizza(modelos)` → `[{modelo, pct,
  cor}]` para teste sem DOM.

## 5. Testes e critérios de pronto

- **Rust**: `cargo test --workspace` (incluindo os testes movidos para
  `nucleo::ia_claude` e os novos de `fonte`), `cargo clippy --workspace`
  limpo, `cargo fmt --all`.
- **Web** (vitest): seletor de mês (formatação pt-BR, `›` desabilitada no
  mês atual, clique nas setas muda o fetch), segmented de fonte (refetch com
  `fonte` certa, seções de horas somem com `opencode`), pizza
  (`fatiasPizza`: percentuais, ordenação, exclusão de tokens 0). `npm test`
  e `npm run build` verdes.
- Comentários didáticos pt-br nos dois lados; conferência visual nos dois
  temas (a pizza usa variáveis, deve reagir ao modo escuro sem ajuste).

## Fora de escopo

- Dropdown de meses / salto direto de período.
- Custo do Claude no CLI mudar de comportamento (é só mudança de morada).
- Persistir a escolha de fonte/mês (estado local da tela; reset ao navegar).
- Re-sync do design system (fazer após merge; atualizar o stub de fetch do
  preview `IaCustos.tsx` em `.design-sync/previews/` junto).
