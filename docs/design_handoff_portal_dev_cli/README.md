# Handoff: Portal dev-cli (redesign) + Landing page

## Overview
Redesign completo do portal web do **dev-cli** (monitor de logs de containers + custos de IA) e uma landing page do projeto, ambos no design system **Classical** (editorial, serifado, hairlines, cor como traço). O alvo é o codebase existente em `web/` (React 19 + Vite + TypeScript, componentes e nomes em português).

## Sobre os arquivos de design
Os `.dc.html` deste pacote são **referências de design em HTML** — protótipos que mostram aparência e comportamento pretendidos, **não código de produção**. A tarefa é **recriar estas telas no app React existente** (`web/src/`), seguindo os padrões já estabelecidos ali: componentes apresentacionais que recebem props, `App.tsx` dono do estado global e do polling de 15s, tipos em `tipos.ts` espelhando o JSON da API, formatação pura em `formato.ts`, CSS com variáveis em `index.css`. Sem novas dependências — o projeto não usa lib de UI nem de gráficos; tudo aqui é HTML/CSS puro.

## Fidelidade
**Alta (hifi).** Recriar pixel-perfect: cores, tipografia, espaçamentos e estados vêm dos tokens em `classical-tokens.css` (incluído). Copiar os tokens necessários para o `index.css` do portal (substituindo o tema escuro atual) ou importar o arquivo inteiro.

## Design tokens (essenciais)
Fonte completa: `classical-tokens.css` (variáveis `--color-*`, `--space-*`, `--radius-*`, `--shadow-*`, `--font-*`).
- Fundo `--color-bg: #f3f2f2` · texto `--color-text: #201f1d` · divisor `--color-divider`
- Acento (único) `--color-accent: #b68235` com ramp 100–900; para texto pequeno em acento usar `--color-accent-700`
- Severidade (semânticas, fora do DS mas harmonizadas em OKLCH): **Verde `#5f7a52`**, **Amarelo `var(--color-accent)`**, **Vermelho/erro `#a2503c`**, **Parado `var(--color-neutral-400)`**
- Headings: Cormorant Garamond (`--font-heading`, peso `--font-heading-weight`, nunca bold pesado); corpo: Lora (`--font-body`); logs e nomes de modelo: `ui-monospace, Menlo, monospace`
- Números em colunas/KPIs: `font-feature-settings: 'tnum'`
- Cor é traço, não preenchimento: cards e botões com borda 1px, sem fills sólidos de acento; sombras só `--shadow-sm/md/lg`
- Foco de teclado: `outline: 2px solid var(--color-accent); outline-offset: 2px` em `:focus-visible`
- Intensidade (histórico/heatmap), níveis 0–5: `neutral-200, accent-100, accent-200, accent-400, accent-600, #a2503c`

## Telas

### 1. Nav global (todas as telas)
Barra sticky no topo (fundo `--color-bg`, hairline embaixo): marca "dev-cli · portal" (heading serif), links **Visão geral / Histórico / IA · custos / Configuração** (ativo com `aria-current="page"`, sublinhado em acento), e à direita o resumo global em 12px neutro: `"N problemas · 5 containers · 36.9k reqs · 277 erros"` (derivado dos containers, como o `Cabecalho.tsx` atual). Roteamento: pode ser estado no `App` (como hoje) ou react-router se preferirem — o protótipo usa estado.

Banner de API fora (substitui `.erro-conexao`): borda 1px `#a2503c`, texto `#7c3a2e`, fundo tint 7%, radius `--radius-md`: "Sem resposta da API — o dev-server está rodando? Mostrando os últimos dados conhecidos."

### 2. Visão geral
Header da tela: h1 serif 34px "Visão geral" + subtítulo 13px "janela de 60 minutos · coleta contínua · docker local" + "atualizado há Xs" à direita (usa `formatarHaQuanto`).

Grid `1fr 380px`, gap 36px:
- **Esquerda — tabela de containers** (evolução da `TabelaContainers.tsx`): kicker uppercase 11px "Containers" + "piores primeiros". Colunas: bolinha de severidade (9px), container (semibold), status, err, crit, 5xx, 4xx, p95, máx, reqs — numéricas alinhadas à direita, tnum. `err > 50` e `crit > 0` em `#a2503c`. Linha clicável abre o drawer; linha selecionada com fundo tint de acento 8%. Header da tabela: small caps, hairline rows (classe `.table` do DS). Nota 12px abaixo: "Clique numa linha para abrir as linhas de log da janela."
- **Direita — coluna lateral**: (a) card **Alertas** (borda com mistura de acento, kicker "Alertas" em `--color-accent-700`, itens com marcador "§" em acento + mensagem + "há Xh") — some quando vazio, como hoje; (b) **Erros ao vivo** (evolução do `FeedErros.tsx`): kicker + bolinha de status (verde `#5f7a52` = polling ok; neutra = parado), lista com hairline entre itens, cada item em duas linhas: [NÍVEL colorido 11px semibold · container 11.5px neutro · "há Xs" à direita] em cima, linha de log em mono 11.5px truncada com ellipsis embaixo. Item novo: animação `pulseNovo` 2.2s (fundo tint vermelho 18% → transparente) — manter a lógica atual do destaque de 2s isolado em `ItemErro`. Clique abre o drawer já filtrado no nível. `max-height: 520px`, scroll.

### 3. Drawer de drill-down (evolução do `PainelContainer.tsx`)
Slide-over à direita, 560px (max 92vw), fundo `--color-bg`, borda esquerda hairline, `--shadow-lg`, backdrop `rgba(32,31,29,.35)` que fecha ao clique; **manter Esc para fechar e a transição de entrada** do componente atual. Barra: nome do container em serif 24px + `<select>` de nível (todos / CRITICAL / ERROR / WARNING / INFO / DEBUG — manter a lista `NIVEIS` completa com abreviações) + botão ghost "Fechar". Kicker "Linhas da janela · N linhas". Linhas: grid [nível 64px semibold colorido | texto mono 11.5px `pre-wrap`/`break-all`], hairline fraca entre linhas. Vazio: "Nenhuma linha na janela com esse filtro."

### 4. Histórico (tela nova)
Requer endpoint novo no dev-server (ex.: `/api/containers/historico?horas=24` → contagem de erros+críticos por hora por container). UI: uma linha por container: [nome 110px semibold | strip de 24 células (grid 24 colunas, gap 3px, altura 22px, radius 2px, borda hairline, cor pela escala de intensidade 0–5) | total à direita tnum]. Rodapé: "00h ——— agora" + legenda de 5 quadrados (menos → mais). Tooltip por célula via `title`.

### 5. IA · custos (tela nova — versão web do `dev-cli ai stats`)
Requer expor os dados dos comandos `ai stats opencode|claude` na API (eles já saem em `--json`).
- **Faixa de KPIs**: 4 células numa moldura única (borda 1px, divisórias internas hairline): Custo no mês (serif 34px tnum, ex. "US$ 186,40", linha secundária "≈ R$ 1.010,29"), Tokens ("412.6M" / "86% em cache"), Horas com Claude ("61h20m" / média por dia ativo), Streak ("14 dias" / "melhor: 23 dias"). Kickers uppercase 11px.
- **Heatmap do mês** (GitHub-style): grid `grid-auto-flow: column`, 7 linhas de 18px, gap 4px; rótulos seg/qua/sex/dom à esquerda; células 18px radius 3px com borda hairline, cores pela escala de intensidade; offset de células transparentes para o dia da semana do dia 1; futuro transparente. Tooltip "D/jul — nível N".
- **Horas por semana**: linhas [rótulo 90px | barra 10px (trilha `neutral-200`, preenchimento acento, largura % do máximo) | valor "18h18m" à direita].
- **Por modelo**: tabela `.table` [modelo em mono 12px | tokens | custo | barra 8px colorida por modelo: sonnet=acento, opus=`#a2503c`, haiku=`#5f7a52`, outros=neutral-500], largura % do maior custo. Nota justificada 12px sobre estimativa de preços/câmbio e teto de 4h por sessão.
- Moeda US$/R$ conforme preferência (o usuário escolheu **R$** como default ao revisar — custo principal em R$, secundário em US$; câmbio ao vivo já existe em `cambio.rs`).

### 6. Configuração (tela nova)
Coluna única max 720px. Subtítulo referenciando `/etc/dev-cli/config.toml` e `DEV_CLI_*` (paths em mono). Campos (classes `.field`/`.input`/`.seg` do DS — ver `classical-tokens.css`): segmented "Origem da coleta" (docker local | SSH remoto); "Host SSH" (desabilitado quando docker local — no protótipo está sempre disabled, na implementação habilitar quando SSH); grid 2×2: Intervalo de coleta (s) 30, Janela de análise (min) 60, Retenção do banco (dias) 14, Porta da API 8787; "Diretório do portal" `/var/lib/dev-cli/portal`. Rodapé com hairline: botão primário (contorno acento) "Salvar alterações" + ghost "Restaurar padrão". Persistência: a decidir (o servidor hoje só lê TOML — pode começar somente-leitura).

### 7. Landing (`Landing dev-cli.dc.html`)
Página estática separada (pode ser um HTML à parte, fora do app React): nav do DS com CTA "Ver o portal"; hero centrado (kicker uppercase em acento, h1 serif 64px "Os seus logs e os seus custos de IA, num só binário.", parágrafo justificado, botões contorno); **plate** 16:8 com placeholder do screenshot do dashboard TUI (substituir por captura real); seção "Três frentes, um binário" — 3 colunas separadas por hairlines verticais (Logs ao vivo / Portal web / Custos de IA), texto justificado 14px; seção Instalação — 3 cards com `<pre>` mono 13px (brew tap+install, scoop bucket+install, cargo install --git); footer hairline "MIT © 2026 Jarede Silva".

## Interações e comportamento
- Polling de 15s (manter `App.tsx`: Promise.all, cursor de erros em ref, trava de corrida, teto de 50 itens no feed)
- Destaque "novo" no feed: 2s, animação de fundo (não re-renderizar a lista pai)
- Clique em linha da tabela: toggle do drawer; clique em item do feed: abre drawer filtrado no nível
- Drawer: Esc fecha, backdrop fecha, transição translateX 0.2s ease
- Hovers: tint de acento ~6% em linhas/itens clicáveis; nunca deixar estados default do browser
- Erro de API: manter dados stale na tela + banner (comportamento atual)

## Estado
Igual ao atual (`App.tsx`) + `tela: 'visao' | 'hist' | 'ia' | 'conf'`. Novos dados: histórico por hora e stats de IA (novos endpoints ou fetch condicional ao entrar na tela).

## Dados de exemplo usados nos protótipos
Containers reais do projeto: `prezzo` (vermelho: 231 err, 4 crit, p95 2.41s), `ecomm` (amarelo), `supply` (parado), `bapi`, `intranet` (verdes). Linhas de log no estilo Loguru/Oracle do Prezzo. Meses/valores de IA são plausíveis mas fictícios.

## Arquivos
- `Portal dev-cli.dc.html` — protótipo interativo das 4 telas + drawer (abrir no browser; a lógica está no `<script>` do fim do arquivo, o markup no `<x-dc>`)
- `Landing dev-cli.dc.html` — landing estática
- `classical-tokens.css` — o stylesheet completo do design system (tokens + classes `.btn/.card/.table/.field/.nav/.tag/.plate`)
- `screenshots/` — capturas de referência: 01 Visão geral, 02 drawer de drill-down, 03 Histórico, 04 IA · custos, 05 Configuração, 06 landing

## Prompt sugerido para o Claude Code (Sonnet)
> Leia design_handoff_portal_dev_cli/README.md e os dois .dc.html. Recrie o redesign no app React em web/src/, seguindo os padrões existentes (componentes em português, App.tsx dono do estado/polling, formato.ts, tipos.ts, CSS com variáveis em index.css — sem novas dependências). Comece pela troca de tema (tokens do classical-tokens.css) e pela tela Visão geral + drawer, mantendo os testes existentes passando; depois adicione as telas Histórico, IA · custos e Configuração (novos endpoints no crate servidor conforme o README). A landing é um HTML estático separado.
