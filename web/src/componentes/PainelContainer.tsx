import { useEffect, useState } from 'react'
import { buscarLinhas } from '../api'
import type { LinhaLog } from '../tipos'

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
