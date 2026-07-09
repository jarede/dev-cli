// Drill-down de um container: as linhas de log cruas da janela, com filtro
// por nível — o equivalente web da tela de linhas da TUI.
// Este componente é "inteligente": busca os próprios dados via useEffect
// quando `nome` ou `nivel` mudam (as dependências do effect).
// docs: https://react.dev/reference/react/useEffect

import { useEffect, useState } from 'react'
import { buscarLinhas } from '../api'
import type { LinhaLog } from '../tipos'

/// Níveis oferecidos no filtro ('' = todos). Lista fixa: os níveis do
/// Loguru/parse do nucleo, não vale a pena descobrir dinamicamente.
const NIVEIS = ['', 'CRITICAL', 'ERROR', 'WARNING', 'INFO', 'DEBUG'] as const

interface Props {
  nome: string
  aoFechar: () => void
}

export function PainelContainer({ nome, aoFechar }: Props) {
  const [nivel, setNivel] = useState('')
  const [linhas, setLinhas] = useState<LinhaLog[]>([])
  const [erro, setErro] = useState<string | null>(null)

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

  return (
    <section className="painel-container">
      <div className="barra">
        <strong>{nome}</strong>
        <select value={nivel} onChange={(e) => setNivel(e.target.value)}>
          {NIVEIS.map((n) => (
            <option key={n} value={n}>
              {n === '' ? 'todos os níveis' : n}
            </option>
          ))}
        </select>
        <button onClick={aoFechar}>fechar</button>
      </div>
      {erro !== null && <p className="erro-conexao">⚠ {erro}</p>}
      {erro === null && linhas.length === 0 && (
        <p className="vazio">nenhuma linha na janela com esse filtro</p>
      )}
      <ul className="linhas-log">
        {linhas.map((l, i) => (
          <li key={i}>{l.linha}</li>
        ))}
      </ul>
    </section>
  )
}
