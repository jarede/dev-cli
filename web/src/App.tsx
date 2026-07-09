// A raiz do portal: dona do estado global (containers, alertas, erro) e do
// POLLING — a cada 15s refaz as buscas, imitando o refresh do dashboard
// TUI. Os componentes de baixo só recebem props.

import { useCallback, useEffect, useState } from 'react'
import { buscarAlertas, buscarContainers } from './api'
import type { Alerta, ContainerResumo } from './tipos'
import { Cabecalho } from './componentes/Cabecalho'
import { ListaAlertas } from './componentes/ListaAlertas'
import { PainelContainer } from './componentes/PainelContainer'
import { TabelaContainers } from './componentes/TabelaContainers'

/// Intervalo do polling (ms). 15s: metade do intervalo default de coleta —
/// dados novos aparecem no máximo meio ciclo depois de gravados.
const INTERVALO_POLLING_MS = 15_000

function App() {
  const [containers, setContainers] = useState<ContainerResumo[]>([])
  const [alertas, setAlertas] = useState<Alerta[]>([])
  const [erro, setErro] = useState<string | null>(null)
  const [atualizadoEm, setAtualizadoEm] = useState<number | null>(null)
  const [selecionado, setSelecionado] = useState<string | null>(null)

  // `useCallback`: memoriza a função para o useEffect abaixo poder listá-la
  // como dependência sem recriar o intervalo a cada render.
  // docs: https://react.dev/reference/react/useCallback
  const carregar = useCallback(async () => {
    try {
      // As duas buscas em paralelo — Promise.all falha se qualquer uma
      // falhar, o que aqui é o comportamento certo (mesma API fora do ar).
      // docs: https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Promise/all
      const [novosContainers, novosAlertas] = await Promise.all([
        buscarContainers(),
        buscarAlertas(),
      ])
      setContainers(novosContainers)
      setAlertas(novosAlertas)
      setErro(null)
      setAtualizadoEm(Date.now())
    } catch (e: unknown) {
      // Mantém os últimos dados na tela (stale é melhor que vazio) e
      // liga o banner de erro.
      setErro(e instanceof Error ? e.message : String(e))
    }
  }, [])

  useEffect(() => {
    void carregar() // primeira busca imediata
    const timer = setInterval(() => void carregar(), INTERVALO_POLLING_MS)
    // Cleanup: sem isso o intervalo sobrevive ao componente (vazamento).
    // docs: https://react.dev/reference/react/useEffect#connecting-to-an-external-system
    return () => clearInterval(timer)
  }, [carregar])

  return (
    <>
      <Cabecalho containers={containers} erro={erro} atualizadoEm={atualizadoEm} />
      <ListaAlertas alertas={alertas} />
      <TabelaContainers
        containers={containers}
        selecionado={selecionado}
        aoSelecionar={(nome) => setSelecionado(nome === selecionado ? null : nome)}
      />
      {selecionado !== null && (
        <PainelContainer nome={selecionado} aoFechar={() => setSelecionado(null)} />
      )}
    </>
  )
}

export default App
