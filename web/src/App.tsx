import { useCallback, useEffect, useState } from 'react'
import { buscarAlertas, buscarContainers } from './api'
import type { Alerta, ContainerResumo } from './tipos'
import { Cabecalho } from './componentes/Cabecalho'
import { ListaAlertas } from './componentes/ListaAlertas'
import { PainelContainer } from './componentes/PainelContainer'
import { TabelaContainers } from './componentes/TabelaContainers'

const INTERVALO_POLLING_MS = 15_000

function App() {
  const [containers, setContainers] = useState<ContainerResumo[]>([])
  const [alertas, setAlertas] = useState<Alerta[]>([])
  const [erro, setErro] = useState<string | null>(null)
  const [atualizadoEm, setAtualizadoEm] = useState<number | null>(null)
  const [selecionado, setSelecionado] = useState<string | null>(null)

  const carregar = useCallback(async () => {
    try {
      const [novosContainers, novosAlertas] = await Promise.all([
        buscarContainers(),
        buscarAlertas(),
      ])
      setContainers(novosContainers)
      setAlertas(novosAlertas)
      setErro(null)
      setAtualizadoEm(Date.now())
    } catch (e: unknown) {
      setErro(e instanceof Error ? e.message : String(e))
    }
  }, [])

  useEffect(() => {
    void carregar()
    const timer = setInterval(() => void carregar(), INTERVALO_POLLING_MS)
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
