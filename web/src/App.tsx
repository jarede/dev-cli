// A raiz do portal: dona do estado global (containers, alertas, erros, erro)
// e do POLLING — a cada 15s refaz as buscas, imitando o refresh do dashboard
// TUI. Os componentes de baixo só recebem props.

import { useCallback, useEffect, useRef, useState } from 'react'
import { buscarAlertas, buscarContainers, buscarErros } from './api'
import type { Alerta, ContainerResumo } from './tipos'
import { Cabecalho } from './componentes/Cabecalho'
import { FeedErros, type ErroLogComDestaque } from './componentes/FeedErros'
import { ListaAlertas } from './componentes/ListaAlertas'
import { PainelContainer } from './componentes/PainelContainer'
import { TabelaContainers } from './componentes/TabelaContainers'

/// Intervalo do polling (ms). 15s: metade do intervalo default de coleta —
/// dados novos aparecem no máximo meio ciclo depois de gravados.
const INTERVALO_POLLING_MS = 15_000

/// Teto de itens do feed na memória do navegador (aba esquecida dias).
const TETO_FEED = 50

function App() {
  const [containers, setContainers] = useState<ContainerResumo[]>([])
  const [alertas, setAlertas] = useState<Alerta[]>([])
  const [erros, setErros] = useState<ErroLogComDestaque[]>([])
  const [erro, setErro] = useState<string | null>(null)
  const [atualizadoEm, setAtualizadoEm] = useState<number | null>(null)
  const [selecionado, setSelecionado] = useState<string | null>(null)
  const [nivelInicial, setNivelInicial] = useState<string | undefined>(undefined)
  // Maior id já visto. useRef: mudar o cursor NÃO deve disparar re-render.
  // docs: https://react.dev/reference/react/useRef
  const cursorRef = useRef(0)
  // Primeira carga mostra contexto sem piscar o destaque de "novo".
  const cargaInicialRef = useRef(true)

  // `useCallback`: memoriza a função para o useEffect abaixo poder listá-la
  // como dependência sem recriar o intervalo a cada render.
  // docs: https://react.dev/reference/react/useCallback
  const carregar = useCallback(async () => {
    try {
      // As três buscas em paralelo — Promise.all falha se qualquer uma
      // falhar, o que aqui é o comportamento certo (mesma API fora do ar).
      // docs: https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Promise/all
      const [novosContainers, novosAlertas, loteErros] = await Promise.all([
        buscarContainers(),
        buscarAlertas(),
        buscarErros(cursorRef.current),
      ])
      setContainers(novosContainers)
      setAlertas(novosAlertas)

      if (loteErros.length > 0) {
        // Avança o cursor ANTES do setState: o próximo poll já pede só o novo.
        cursorRef.current = loteErros[loteErros.length - 1].id
        const marcarNovo = !cargaInicialRef.current
        // API devolve id ASC; o feed mostra mais recente primeiro.
        setErros((atual) => {
          const comDestaque: ErroLogComDestaque[] = [...loteErros]
            .reverse()
            .map((e) => ({ ...e, novo: marcarNovo }))
          return comDestaque.concat(atual).slice(0, TETO_FEED)
        })
      }
      cargaInicialRef.current = false

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
      <FeedErros
        erros={erros}
        aoClicar={(container, nivel) => {
          setSelecionado(container)
          setNivelInicial(nivel)
        }}
      />
      <ListaAlertas alertas={alertas} />
      <TabelaContainers
        containers={containers}
        selecionado={selecionado}
        aoSelecionar={(nome) => {
          setSelecionado(nome === selecionado ? null : nome)
          setNivelInicial(undefined)
        }}
      />
      {selecionado !== null && (
        <PainelContainer
          key={`${selecionado}-${nivelInicial ?? ''}`}
          nome={selecionado}
          nivelInicial={nivelInicial}
          aoFechar={() => {
            setSelecionado(null)
            setNivelInicial(undefined)
          }}
        />
      )}
    </>
  )
}

export default App
