// Feed único de erros/críticos de QUALQUER container — apresentacional:
// a App busca e mantém o cursor; este componente só lista e notifica clique.
// Item novo ganha destaque temporário (2s) isolado em `ItemErro` para o
// timeout não re-renderizar a lista inteira a cada expiração.

import { useEffect, useState } from 'react'
import type { ErroLog } from '../tipos'
import { formatarHaQuanto } from '../formato'

/// ErroLog + flag de "acabou de chegar" (calculada no pai, não aqui).
export type ErroLogComDestaque = ErroLog & { novo: boolean }

interface Props {
  erros: ErroLogComDestaque[]
  aoClicar: (container: string, nivel: string) => void
}

/// Mapa nível → cor CSS, espelhando `colorir_nivel` do CLI (ERROR/CRIT = vermelho).
function corNivel(nivel: string): string {
  const n = nivel.toUpperCase()
  if (n === 'ERROR' || n === 'ERRO' || n === 'CRIT' || n === 'CRITICAL' || n === 'FATAL') {
    return 'var(--vermelho)'
  }
  if (n === 'WARNING' || n === 'WARN') return 'var(--amarelo)'
  return 'var(--texto-fraco)'
}

/// Um item do feed. O destaque `novo` some sozinho após 2s via estado local
/// — assim o setTimeout não força re-render da lista pai.
function ItemErro({
  erro,
  aoClicar,
}: {
  erro: ErroLogComDestaque
  aoClicar: (container: string, nivel: string) => void
}) {
  const [destacar, setDestacar] = useState(erro.novo)

  useEffect(() => {
    if (!erro.novo) return
    const timer = setTimeout(() => setDestacar(false), 2000)
    return () => clearTimeout(timer)
  }, [erro.novo, erro.id])

  return (
    <li
      className={destacar ? 'novo' : undefined}
      onClick={() => aoClicar(erro.container, erro.nivel)}
      title={erro.linha}
    >
      <span className="bolinha" style={{ background: corNivel(erro.nivel) }} />
      <span className="feed-container">{erro.container}</span>
      <span className="feed-nivel" style={{ color: corNivel(erro.nivel) }}>
        {erro.nivel}
      </span>
      <span className="feed-linha">{erro.linha}</span>
      <span className="feed-quando">{formatarHaQuanto(erro.collected_at)}</span>
    </li>
  )
}

export function FeedErros({ erros, aoClicar }: Props) {
  if (erros.length === 0) return null

  return (
    <section className="feed-erros">
      <h2>erros ao vivo</h2>
      <ul>
        {erros.map((e) => (
          <ItemErro key={e.id} erro={e} aoClicar={aoClicar} />
        ))}
      </ul>
    </section>
  )
}
