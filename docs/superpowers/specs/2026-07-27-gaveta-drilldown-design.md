# Painel de detalhes vira gaveta lateral — design

Data: 2026-07-27

## Contexto e motivação

O feed de erros ao vivo (ver [[2026-07-27-feed-erros-portal-design]]) ficou
bom, mas o painel de detalhes (`PainelContainer`, aberto ao clicar num
container da tabela ou num item do feed) hoje renderiza no fluxo normal da
página, **depois** da `TabelaContainers` — ou seja, embaixo de todos os
containers listados. Numa lista longa, o usuário clica em algo no topo (um
item do feed, por exemplo) e o painel de resultado abre fora da tela,
exigindo rolar pra baixo pra ver o que pediu.

Decisão validada com o usuário: o painel vira uma **gaveta lateral fixa**
(drawer), ancorada à direita, sempre visível ao abrir — sem precisar rolar,
independente de onde a página estava.

## Arquitetura

### CSS: de "seção da página" para "camada fixa"

`PainelContainer` continua o mesmo componente (mesma lógica de busca,
filtro de nível, `nivelInicial`) — só a apresentação muda. A `<section
className="painel-container">` ganha (via CSS, não via `createPortal`, já
que `position: fixed` já tira o elemento do fluxo do documento
independente de onde ele é montado no JSX):

```css
.painel-container {
  position: fixed;
  top: 0;
  right: 0;
  height: 100vh;
  width: 420px;
  max-width: 90vw;
  background: var(--painel);
  border-left: 1px solid var(--borda);
  box-shadow: -4px 0 24px rgba(0, 0, 0, 0.3);
  z-index: 20;
  display: flex;
  flex-direction: column;
  transform: translateX(0);
  transition: transform 0.2s ease;
}
```

A lista de linhas (`ul.linhas-log`) ganha `overflow-y: auto; flex: 1` para
rolar dentro da gaveta sem ela crescer além de `100vh`.

### Backdrop

Novo `<div className="painel-backdrop" onClick={aoFechar} />` renderizado
**antes** da `<section>`, dentro do próprio `PainelContainer` (ou como
elemento irmão em `App.tsx` — decisão de implementação, tanto faz já que os
dois são `position: fixed`):

```css
.painel-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  z-index: 19; /* atrás da gaveta (20), na frente do resto (default) */
}
```

Clicar no backdrop chama `aoFechar` — mesmo callback do botão "fechar" já
existente. Clicar **dentro** da gaveta não deve propagar pro backdrop
(`onClick` na `<section>` com `e.stopPropagation()`, ou simplesmente o fato
de a `<section>` ser um elemento separado do backdrop já resolve, contanto
que não haja bubbling — como são irmãos, não há bubbling entre eles; só
cuidado é a gaveta não ficar *dentro* do backdrop no DOM).

### Fechar com Esc

Novo `useEffect` em `PainelContainer`, paralelo ao já existente:

```tsx
useEffect(() => {
  const aoTeclar = (e: KeyboardEvent) => {
    if (e.key === 'Escape') aoFechar()
  }
  window.addEventListener('keydown', aoTeclar)
  return () => window.removeEventListener('keydown', aoTeclar)
}, [aoFechar])
```

### O que NÃO muda

- Lógica de busca (`useEffect` de `buscarLinhas`), filtro de nível,
  `nivelInicial`, o `key` de remount em `App.tsx` — tudo igual.
- Posição do `FeedErros`, `TabelaContainers`, `ListaAlertas` na página —
  continuam no fluxo normal, só o `PainelContainer` sai dele.
- `App.tsx` continua condicionando a renderização a `selecionado !== null`;
  só que agora, visualmente, isso abre uma gaveta em vez de revelar uma
  seção no fim da página.

## Testes

- `PainelContainer.test.tsx`: tecla `Escape` chama `aoFechar`; clique no
  backdrop chama `aoFechar`; clique dentro da gaveta (ex.: no `<select>` ou
  numa linha de log) NÃO chama `aoFechar`.

## Fora de escopo

- Responsividade fina para telas muito estreitas (mobile) além do
  `max-width: 90vw` — se motivo aparecer numa iteração futura, o CSS já
  degrada razoavelmente (gaveta quase full-width), mas não foi um requisito
  explícito aqui.
- Mudar a lógica de dados do `PainelContainer` (drill-down continua
  idêntico) — só apresentação/posicionamento.
