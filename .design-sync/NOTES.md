# Notas do design-sync (dev-cli portal)

- O `web/` é um app Vite, não uma lib empacotada: não há dist/entry de lib. O
  config usa `entry: "web/dist-lib/index.js"` (caminho propositalmente
  inexistente) só para ancorar o PKG_DIR em `web/` — o build cai no modo
  synth-entry a partir de `srcDir`.
- `srcDir` é `src/componentes` (não `src`): o synth-entry importa TODOS os
  .tsx do srcDir, e `src/main.tsx` importa `index.css`, cujo `@font-face`
  com url absoluta `/fontes/…` quebra o esbuild.
- Fork `overrides/css.mjs`: resolve urls absolutas `/fontes/…` (padrão Vite
  `public/`) tentando `<root>/public/<url>` — sem ele, [FONT_DANGLING] nas 8
  faces de Cormorant Garamond/Lora.
- **OK do usuário (2026-07-29): "Cascadia Code" fica sem @font-face** — é o 3º
  fallback do stack mono (`ui-monospace, Menlo, "Cascadia Code", monospace`),
  nunca foi distribuída pelo app; substitutos de sistema aceitos.

- As telas com fetch interno rendem via **stub de `window.fetch` no próprio
  preview** (`previews/<Nome>.tsx`) com payloads espelhando `tipos.ts` —
  mudou a API, atualize o stub junto.
- `extraEntries` embute `.design-sync/extra/roteador-preview.tsx`
  (`RoteadorPreview` = MemoryRouter da MESMA instância do react-router-dom do
  bundle); é também o `cfg.provider` global. Sem ele o `Cabecalho` (NavLink)
  quebra por falta de contexto de Router.
- Overrides de card: `PainelContainer` single 900x640 (drawer fixed);
  `VisaoGeral`/`IaCustos` column 1240x900 (grid largo clipava a 900);
  `TabelaContainers` column (flagrado por GRID_OVERFLOW); demais telas column.

## Known render warns

- `[FONT_MISSING] "Cascadia Code"` — aceito (ver OK acima), esperado em todo
  sync.

## Re-sync risks

- O truque do `entry` inexistente depende do walk-up até `web/package.json`;
  se um dia existir `web/dist-lib/`, o build passa a usá-lo como entry real.
- Componentes `Historico`, `IaCustos`, `Testes` e `Configuracao` buscam dados
  via fetch interno (`/api/...`) — previews dependem do estado de
  carregamento/erro renderizar algo decente sem rede.
- `conventions.md` enumera classes/tokens do `_ds_bundle.css` — se renomear
  classes no `index.css` do portal, revalide os nomes citados lá (o passo de
  header do re-sync já faz isso).
- Estados interativos não renderizáveis estaticamente (suíte expandida com
  falhas em Testes, form "Nova suíte", hover/drag) ficaram fora dos previews.
- Playwright pinado em 1.60.0 no `.ds-sync/` (chromium-1223 do cache do
  usuário em ~/Library/Caches/ms-playwright); `.ds-sync/` é gitignored, então
  um clone novo reinstala e pode precisar re-casar a versão com o cache.
