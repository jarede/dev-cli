# Modo escuro do portal web — design

Data: 2026-07-29 · Status: aprovado em conversa (Abordagem A)

## Objetivo

Dar ao portal (`web/`) um modo escuro na mesma família editorial do design
system Classical, ativado por **toggle na nav com persistência** e, na
ausência de escolha salva, **seguindo o tema do sistema operacional**.
Escopo: só o app React — a landing estática (`web/public/landing.html`)
permanece clara.

## Decisões já tomadas

- **Abordagem A**: bloco único de override de tokens em CSS +
  `data-theme` carimbado no `<html>` pelo JS. Descartadas: `light-dark()`
  (ruído em ~30 variáveis, fallback pior) e stylesheet separado (flash de
  tema, mais arquivos).
- **O JS sempre resolve o tema** (escolha salva → senão preferência do
  sistema) e carimba `data-theme="dark"` ou `"light"`. Com isso o CSS tem
  **um único bloco escuro** (`:root[data-theme="dark"]`) — sem duplicar os
  tokens numa media query `prefers-color-scheme`.
- **Toggle simples de 2 estados** (claro ↔ escuro). O sistema vale até o
  primeiro clique; sem terceiro estado "voltar ao sistema" (YAGNI).
- Landing fora do escopo.

## 1. Tokens escuros (`web/src/index.css`)

Um bloco `:root[data-theme="dark"]` redefinindo **apenas cores e sombras**
(tipografia, espaçamento e raios não mudam):

| Grupo | Claro (atual) | Escuro |
|---|---|---|
| `--color-bg` | `#f3f2f2` | `#171614` (tinta quente) |
| `--color-surface` | `#eae9e9` | `#201f1d` |
| `--color-text` | `#201f1d` | `#e8e4de` (papel) |
| `--color-divider` | mix 16% de `#201f1d` | mix ~14% de `#e8e4de` |
| `--color-accent` | `#b68235` | `#b68235` (mantido — funciona no escuro) |

- **Ramps invertem de sentido no escuro**: `--color-neutral-100` passa a ser
  o cinza mais **escuro** (quase-fundo) e `--color-neutral-900` o mais
  **claro** (quase-texto); mesma lógica no ramp do acento
  (`--color-accent-100` escuro/terroso → `--color-accent-900` claro/areia).
  Racional: usos existentes (`.kicker` com `neutral-600`, trilha de barra com
  `neutral-200`, texto pequeno em acento com `accent-700`) continuam com o
  contraste certo **sem tocar em nenhum componente**.
- **Severidades clareadas** para contraste em fundo escuro: verde `#82a071`,
  vermelho `#c96f56`, amarelo = `var(--color-accent)`, parado =
  `var(--color-neutral-400)` (que no ramp invertido já sai mais claro).
- **Sombras**: pretas mais profundas (opacidades maiores) — em fundo escuro
  a sombra atual desaparece.
- **`--novo-bg`** (animação `pulseNovo` do feed): recalibrado sobre o
  vermelho escuro (~25% de mix, em vez de 18%).
- Ajustes OKLCH finos dos valores acima são permitidos na implementação,
  desde que mantenham a família (cinzas/dourados quentes) e contraste AA
  para texto normal.

### Hexes soltos viram tokens (pré-requisito no bloco claro)

| Onde | Hoje | Vira |
|---|---|---|
| `.banner-api-fora` borda (linhas 258, 745) | `#a2503c` | `var(--sev-vermelho)` |
| `.banner-api-fora` texto + fundo tint | `#7c3a2e` / mix de `#a2503c` | novo `--erro-texto` + mix de `var(--sev-vermelho)` |
| `.drawer-backdrop` (linha 428) | mix 35% de `#201f1d` | novo `--backdrop` |

No escuro: `--erro-texto` clareado (~`#d9a08c`), `--backdrop` com mix mais
denso (fundo já é escuro, o backdrop precisa escurecer mais para ler como
véu).

## 2. Resolução do tema e anti-flash

- **`web/index.html`**: script inline de ~3 linhas, antes do
  `<link>`/`<script>` do app, que lê `localStorage('dev-cli-tema')`, cai para
  `matchMedia('(prefers-color-scheme: dark)')` e seta
  `document.documentElement.dataset.theme` — elimina flash de tema errado
  antes do React montar.
- A chave do localStorage guarda `'claro'` ou `'escuro'`; ausência de chave =
  seguir o sistema.

## 3. Hook `useTema` + toggle na nav

- **`web/src/useTema.ts`** (novo): hook com estado `tema: 'claro'|'escuro'`
  (resolvido) e `alternar()`. Comportamento:
  - Inicializa do `dataset.theme` já carimbado (fonte única com o inline).
  - `alternar()` troca o tema, grava no localStorage e re-carimba o `<html>`.
  - Enquanto **não** houver escolha salva, escuta o evento `change` do
    `matchMedia` e acompanha o sistema; após a primeira escolha, para de
    escutar.
- **`App.tsx`** chama o hook e passa `{tema, alternar}` ao `Cabecalho`.
- **`Cabecalho.tsx`**: botão `btn-ghost` ao fim da nav (depois do resumo),
  rótulo tipográfico ◐ com `aria-label`/`title` "Alternar para tema
  claro/escuro". Props novas opcionais para não quebrar os usos existentes
  nos testes.

## 4. O que já funciona de graça (verificar, não implementar)

- `formato.ts` (`corNivel`, `COR_SEVERIDADE`, `intensidadeParaCor`) devolve
  `var(--...)` — heatmaps, bolinhas e níveis herdam o tema. Verificar na
  implementação que **nenhum** hex está hardcoded no TS; se houver, migrar
  para tokens no mesmo PR.
- Todos os componentes consomem classes/tokens — nenhuma mudança neles além
  do `Cabecalho`.

## 5. Testes e critérios de pronto

- Novos (vitest + testing-library): `useTema` (persistência, fallback ao
  sistema com mock de `matchMedia`, alternância) e toggle no `Cabecalho`
  (render, clique chama `alternar`).
- Suíte existente continua passando sem alteração de asserts (props novas do
  `Cabecalho` são opcionais).
- `npm test` e `npm run build` verdes antes de dar por pronto (convenção do
  CLAUDE.md); comentários didáticos em pt-br nos arquivos novos.
- Conferência visual manual nas 5 telas + drawer nos dois temas.

## Fora de escopo

- Landing (`public/landing.html` / `landing.css`).
- Terceiro estado do toggle ("seguir sistema" explícito).
- Persistência do tema no servidor/config.
- Re-sync do design system para o Claude Design (fazer depois do merge; o
  `.design-sync/NOTES.md` já anota o risco de classes/tokens renomeados).
