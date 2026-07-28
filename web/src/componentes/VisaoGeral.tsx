// Tela Visão geral: grid 1fr 380px com a tabela de containers à esquerda
// e a coluna lateral (alertas + feed de erros) à direita. Recebe os
// dados via props — quem busca e decide se há drawer aberto é o `App`.

import type { Alerta, ContainerResumo } from '../tipos'
import { formatarHaQuanto } from '../formato'
import { ListaAlertas } from './ListaAlertas'
import { FeedErros, type ErroLogComDestaque } from './FeedErros'
import { TabelaContainers } from './TabelaContainers'

interface Props {
  containers: ContainerResumo[]
  alertas: Alerta[]
  erros: ErroLogComDestaque[]
  /** Saúde do polling (`erro === null` no App) — repassada ao `FeedErros`
   *  como a prop `aoVivo`: verde = polling ok, neutra = parado. NÃO é sobre
   *  o feed ter itens (zero erros com polling ok ainda é "ao vivo"). */
  pollingOk: boolean
  /** Date.now() da última busca bem-sucedida (para "atualizado há Xs"). */
  atualizadoEm: number | null
  selecionado: string | null
  aoSelecionarContainer: (nome: string) => void
  aoClicarErro: (container: string, nivel: string) => void
}

export function VisaoGeral({
  containers,
  alertas,
  erros,
  pollingOk,
  atualizadoEm,
  selecionado,
  aoSelecionarContainer,
  aoClicarErro,
}: Props) {
  return (
    <main className="shell" data-screen-label="Visão geral">
      <header className="tela-header">
        <h1>Visão geral</h1>
        <span className="subtitulo">janela de 60 minutos · coleta contínua · docker local</span>
        <span className="atualizado">
          {atualizadoEm !== null
            ? `atualizado ${formatarHaQuanto(Math.floor(atualizadoEm / 1000))}`
            : 'aguardando primeira coleta…'}
        </span>
      </header>
      <div className="visao-grid">
        <section>
          <div className="visao-coluna-titulo">
            <h2 className="kicker">Containers</h2>
            <span className="subtitulo">piores primeiro</span>
          </div>
          <TabelaContainers
            containers={containers}
            selecionado={selecionado}
            aoSelecionar={aoSelecionarContainer}
          />
          <p className="visao-nota">Clique numa linha para abrir as linhas de log da janela.</p>
        </section>
        <aside className="visao-aside">
          <ListaAlertas alertas={alertas} />
          <FeedErros erros={erros} aoClicar={aoClicarErro} aoVivo={pollingOk} />
        </aside>
      </div>
    </main>
  )
}
