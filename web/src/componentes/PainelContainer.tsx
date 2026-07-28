// Drill-down de um container: as linhas de log cruas da janela, com filtro
// por nível — o equivalente web da tela de linhas da TUI.
// Este componente é "inteligente": busca os próprios dados via useEffect
// quando `nome` ou `nivel` mudam (as dependências do effect).
// docs: https://react.dev/reference/react/useEffect

import { useEffect, useState } from 'react'
import { buscarLinhas } from '../api'
import type { LinhaLog } from '../tipos'
import { corNivel } from '../formato'

/// Níveis oferecidos no filtro ('' = todos). Lista fixa: os níveis do
/// Loguru/parse do nucleo, não vale a pena descobrir dinamicamente.
/// Inclui as abreviações de supervisord (CRIT, ERRO, FATAL) além das formas
/// completas — são literais reais gravados pelo `nucleo::core` (ver
/// `NIVEIS_DOCKER`), e chegam aqui via `nivelInicial` quando o clique vem do
/// feed de erros. Sem elas, o `<select>` mostraria "todos os níveis" mesmo
/// com a busca já filtrada por um desses níveis.
const NIVEIS = [
  '',
  'CRITICAL',
  'CRIT',
  'FATAL',
  'ERROR',
  'ERRO',
  'WARNING',
  'INFO',
  'DEBUG',
] as const

interface Props {
  nome: string
  aoFechar: () => void
  /** Filtro de nível ao abrir (ex.: veio do clique num item do feed). */
  nivelInicial?: string
}

export function PainelContainer({ nome, aoFechar, nivelInicial }: Props) {
  // Só a montagem usa nivelInicial — se o pai trocar o filtro, remonta via key.
  const [nivel, setNivel] = useState(nivelInicial ?? '')
  const [linhas, setLinhas] = useState<LinhaLog[]>([])
  const [erro, setErro] = useState<string | null>(null)

  // Desliza da direita ao montar: nasce fora da tela (sem a classe `aberto`,
  // que o CSS traduz em `translateX(100%)`) e, no frame seguinte, ganha a
  // classe que dispara a transição — precisa de dois renders porque CSS não
  // anima uma mudança que já chega pronta no primeiro paint.
  // docs: https://developer.mozilla.org/docs/Web/API/window/requestAnimationFrame
  const [aberto, setAberto] = useState(false)
  useEffect(() => {
    const id = requestAnimationFrame(() => setAberto(true))
    return () => cancelAnimationFrame(id)
  }, [])

  useEffect(() => {
    // Flag de "ainda interessa": se o componente desmontar (ou nome/nivel
    // mudarem) antes da resposta chegar, o cleanup vira a flag e o
    // setState atrasado é ignorado — evita atualizar componente morto.
    // docs: https://react.dev/reference/react/useEffect#fetching-data-with-effects
    let ativo = true
    buscarLinhas(nome, nivel || undefined)
      .then((novas) => {
        if (ativo) {
          setLinhas(novas)
          setErro(null)
        }
      })
      .catch((e: unknown) => {
        if (ativo) setErro(e instanceof Error ? e.message : String(e))
      })
    return () => {
      ativo = false
    }
  }, [nome, nivel])

  // Fechar a gaveta com a tecla Escape — padrão de acessibilidade.
  // docs: https://developer.mozilla.org/docs/Web/API/KeyboardEvent/key
  useEffect(() => {
    const aoTeclar = (e: KeyboardEvent) => {
      if (e.key === 'Escape') aoFechar()
    }
    window.addEventListener('keydown', aoTeclar)
    return () => window.removeEventListener('keydown', aoTeclar)
  }, [aoFechar])

  return (
    <>
      <div className="drawer-backdrop" onClick={aoFechar} />
      <section className={`drawer${aberto ? ' aberto' : ''}`} role="dialog" aria-label={`Linhas de ${nome}`}>
        <div className="drawer-barra">
          <h2>{nome}</h2>
          <select
            className="input"
            style={{ width: 'auto', padding: '4px 8px', fontSize: 12 }}
            value={nivel}
            onChange={(e) => setNivel(e.target.value)}
            aria-label="Filtrar por nível"
          >
            {NIVEIS.map((n) => (
              <option key={n} value={n}>
                {n === '' ? 'todos os níveis' : n}
              </option>
            ))}
          </select>
          <button type="button" className="btn btn-ghost" onClick={aoFechar} style={{ marginLeft: 'auto' }}>
            Fechar
          </button>
        </div>
        <div className="drawer-kicker">
          Linhas da janela · {linhas.length} {linhas.length === 1 ? 'linha' : 'linhas'}
        </div>
        {erro !== null && <div className="banner-api-fora">⚠ {erro}</div>}
        <div className="drawer-conteudo">
          {erro === null && linhas.length === 0 && (
            <p className="drawer-vazio">Nenhuma linha na janela com esse filtro.</p>
          )}
          {linhas.map((l, i) => (
            <div className="drawer-linha" key={i}>
              <span className="nivel" style={{ color: corNivel(l.nivel) }}>{l.nivel}</span>
              <span className="texto">{l.linha}</span>
            </div>
          ))}
        </div>
      </section>
    </>
  )
}
