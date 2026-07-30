# Convenções do dev-cli Portal (design system Classical)

Portal de monitoramento (logs de containers + custos de IA) em estética
**Classical**: editorial, serifado, hairlines — **cor é traço, não
preenchimento** (bordas 1px, nunca fills sólidos de acento). Componentes,
props e classes em **português (pt-br)**.

## Setup e wrapping

- Sem ThemeProvider — os tokens vivem em `styles.css` (importado globalmente).
- `Cabecalho` usa `NavLink` do react-router: envolva-o em
  `DevCliPortal.RoteadorPreview` (MemoryRouter já embutido no bundle). Sem o
  wrapper, ele lança erro de contexto de Router.
- `Historico`, `IaCustos`, `Testes`, `Configuracao` e `PainelContainer`
  **buscam dados via `fetch('/api/...')` ao montar**. Num design, stubbe
  `window.fetch` antes de renderizar (padrão pronto nos exemplos de cada
  `.prompt.md`) — sem stub, elas mostram o banner de erro/`carregando…`.
- Os demais (`TabelaContainers`, `FeedErros`, `ListaAlertas`, `VisaoGeral`,
  `Cabecalho`) são apresentacionais: tudo por props, tipos no `.d.ts`.
  `ListaAlertas` com `alertas={[]}` renderiza `null` (some de propósito).

## Idioma de estilo: variáveis CSS + classes do styles.css

Layout próprio: use as variáveis — fundo `var(--color-bg)` `#f3f2f2`, texto
`var(--color-text)`, hairline `1px solid var(--color-divider)`, acento único
`var(--color-accent)` (ramp `--color-accent-100..900`; texto pequeno em acento
usa `--color-accent-700`), neutros `--color-neutral-100..900`, severidades
`--sev-verde/--sev-amarelo/--sev-vermelho/--sev-parado`, espaçamento
`--space-1..8`, raios `--radius-sm/md/lg`, sombras `--shadow-sm/md/lg`.

Tipografia: headings herdam `var(--font-heading)` (Cormorant Garamond 600 —
nunca bold pesado) via tags `h1..h6`; corpo é `var(--font-body)` (Lora); logs
e nomes de modelo em `var(--font-mono)`. Números em colunas/KPIs:
`font-feature-settings: 'tnum'`.

Classes prontas (as principais): `.card` / `.card-acento` / `.card-kicker`,
`.kicker` (label uppercase 11px), `.table` (header small caps, linhas
hairline, células `.num`), `.btn` / `.btn-primary` / `.btn-ghost`, `.field` /
`.input` / `.seg` / `.seg-opt`, `.nav` / `.nav-brand`, `.banner-api-fora`,
`.shell` / `.tela-header`, `.bolinha` (status 9px), `.vazio`, `.text-muted`,
`.caminho-mono`. Foco de teclado já vem no `:focus-visible` global (outline
2px em acento).

## Onde está a verdade

- `styles.css` → importa `fonts/fonts.css` (Cormorant/Lora woff2) e
  `_ds_bundle.css` (tokens + todas as classes acima). Leia `_ds_bundle.css`
  antes de inventar estilo novo.
- API de cada componente: `components/general/<Nome>/<Nome>.d.ts`; uso e
  exemplos: `<Nome>.prompt.md`.

## Exemplo idiomático

```jsx
const { VisaoGeral, RoteadorPreview, Cabecalho } = DevCliPortal;
const agora = Math.floor(Date.now() / 1000);
const containers = [{ nome: 'prezzo', status: 'running', uptime: 'Up 2 days',
  erros: 231, crits: 4, c5xx: 87, c4xx: 412, reqs: 18234, p95_seg: 2.41,
  max_seg: 8.03, total_linhas: 52310, ultima_coleta: agora - 12,
  severidade: 'Vermelho' }];

<RoteadorPreview>
  <Cabecalho containers={containers} erro={null} />
  <VisaoGeral
    containers={containers} alertas={[]} erros={[]}
    pollingOk={true} atualizadoEm={Date.now()} selecionado={null}
    aoSelecionarContainer={() => {}} aoClicarErro={() => {}} />
</RoteadorPreview>
```
