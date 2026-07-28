// As 4 rotas do portal — declaradas uma vez para o `Cabecalho` (nav) e o
// `App` (que gera as `<Route>`) sem duplicar a lista de caminhos/rótulos.
// Em módulo PRÓPRIO (não dentro de Cabecalho.tsx) porque o Vite/oxlint só
// aplica fast-refresh em arquivos que exportam SÓ componentes — um arquivo
// que mistura `export function Componente` com `export const ROTAS` faz o
// fast-refresh desistir e recarregar a página inteira a cada edição.
// docs: https://vite.dev/guide/backend-integration.html#fast-refresh

/// Uma rota do portal: caminho da URL, rótulo visível na nav e (opcional)
/// `fim: true` para o `<NavLink end>` do react-router — sem isso, "/"
/// ficaria "ativo" em QUALQUER rota (todo caminho começa com "/").
/// docs: https://reactrouter.com/api/components/NavLink#end
export interface Rota {
  caminho: string
  rotulo: string
  fim?: boolean
}

export const ROTAS: Rota[] = [
  { caminho: '/', rotulo: 'Visão geral', fim: true },
  { caminho: '/historico', rotulo: 'Histórico' },
  { caminho: '/ia', rotulo: 'IA · custos' },
  { caminho: '/configuracao', rotulo: 'Configuração' },
]
