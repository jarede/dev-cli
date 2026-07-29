// A raiz do portal: dona do estado global (containers, alertas, erros, erro)
// e do POLLING — a cada 15s refaz as buscas, imitando o refresh do dashboard
// TUI. As 4 telas vêm do react-router; o drawer de drill-down é estado
// (não rota) porque é modal — não tem URL "compartilhável".

import type { ReactNode } from 'react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { Route, Routes } from 'react-router-dom'
import { buscarAlertas, buscarContainers, buscarErros } from './api'
import type { Alerta, ContainerResumo } from './tipos'
import { Cabecalho } from './componentes/Cabecalho'
import { Configuracao } from './componentes/Configuracao'
import type { ErroLogComDestaque } from './componentes/FeedErros'
import { Historico } from './componentes/Historico'
import { IaCustos } from './componentes/IaCustos'
import { PainelContainer } from './componentes/PainelContainer'
import { Testes } from './componentes/Testes'
import { VisaoGeral } from './componentes/VisaoGeral'
import { ROTAS } from './rotas'

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
  // Trava contra corrida: sem ela, duas chamadas de `carregar()` sobrepostas
  // (o StrictMode do React chama os efeitos duas vezes em dev; em produção
  // pode acontecer se uma resposta demorar mais que os 15s do próximo poll)
  // leriam o MESMO `cursorRef.current` antes de qualquer uma avançá-lo,
  // buscando e duplicando o mesmo lote de erros no feed.
  const buscandoRef = useRef(false)

  // `useCallback`: memoriza a função para o useEffect abaixo poder listá-la
  // como dependência sem recriar o intervalo a cada render.
  // docs: https://react.dev/reference/react/useCallback
  const carregar = useCallback(async () => {
    if (buscandoRef.current) return
    buscandoRef.current = true
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
    } finally {
      buscandoRef.current = false
    }
  }, [])

  useEffect(() => {
    void carregar() // primeira busca imediata
    const timer = setInterval(() => void carregar(), INTERVALO_POLLING_MS)
    // Cleanup: sem isso o intervalo sobrevive ao componente (vazamento).
    // docs: https://react.dev/reference/react/useEffect#connecting-to-an-external-system
    return () => clearInterval(timer)
  }, [carregar])

  // Um elemento por caminho de `ROTAS` (a fonte única dos 4 caminhos —
  // ver `rotas.ts`): a nav (`Cabecalho`) itera `ROTAS` para os links, e
  // aqui iteramos a MESMA lista para gerar as `<Route>`, em vez de repetir
  // os 4 caminhos como literais nos dois lugares.
  const elementoPorCaminho: Record<string, ReactNode> = {
    '/': (
      <VisaoGeral
        containers={containers}
        alertas={alertas}
        erros={erros}
        // A bolinha "ao vivo" do feed reflete a SAÚDE do polling (erro ===
        // null), não se há erros no feed — ver `FeedErros.tsx`. Um sistema
        // saudável (zero erros) continua "ao vivo" enquanto o polling
        // funcionar; só fica "parado" quando a API para de responder.
        pollingOk={erro === null}
        atualizadoEm={atualizadoEm}
        selecionado={selecionado}
        aoSelecionarContainer={(nome) => {
          setSelecionado(nome === selecionado ? null : nome)
          setNivelInicial(undefined)
        }}
        aoClicarErro={(container, nivel) => {
          setSelecionado(container)
          setNivelInicial(nivel)
        }}
      />
    ),
    '/historico': <Historico />,
    '/testes': <Testes />,
    '/ia': <IaCustos />,
    '/configuracao': <Configuracao />,
  }

  return (
    <>
      <Cabecalho containers={containers} erro={erro} />
      <Routes>
        {ROTAS.map((r) => (
          <Route key={r.caminho} path={r.caminho} element={elementoPorCaminho[r.caminho]} />
        ))}
      </Routes>
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
