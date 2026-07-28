// Feed único de erros/críticos de QUALQUER container — apresentacional:
// a App busca e mantém o cursor; este componente só lista e notifica clique.
// Item novo ganha destaque temporário (~2s) isolado em `ItemErro` para o
// timeout não re-renderizar a lista inteira a cada expiração.

import { useEffect, useState } from 'react'
import type { ErroLog } from '../tipos'
import { corNivel, formatarHaQuanto } from '../formato'

/// ErroLog + flag de "acabou de chegar" (calculada no pai, não aqui).
export type ErroLogComDestaque = ErroLog & { novo: boolean }

interface Props {
  erros: ErroLogComDestaque[]
  aoClicar: (container: string, nivel: string) => void
  /** Bolinha de "polling ok" no canto do título — verde quando o polling de
   *  15s está funcionando (`erro === null` no App), neutra quando a API
   *  parou de responder. Deliberadamente INDEPENDENTE de `erros.length`:
   *  um sistema saudável com zero erros continua "ao vivo" (a bolinha não
   *  mede "há erro novo", mede "estamos coletando"). */
  aoVivo: boolean
}

/// Um item do feed. O destaque `novo` some sozinho após ~2s via estado
/// local — assim o setTimeout não força re-render da lista pai.
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
    const timer = setTimeout(() => setDestacar(false), 2200)
    return () => clearTimeout(timer)
  }, [erro.novo, erro.id])

  return (
    <div
      className={destacar ? 'feed-item novo' : 'feed-item'}
      onClick={() => aoClicar(erro.container, erro.nivel)}
      title={erro.linha}
    >
      <div className="feed-item-linha1">
        <span
          className="feed-item-nivel"
          style={{ color: corNivel(erro.nivel) }}
        >
          {erro.nivel}
        </span>
        <span className="feed-item-container">{erro.container}</span>
        <span className="feed-item-quando">{formatarHaQuanto(erro.collected_at)}</span>
      </div>
      <div className="feed-item-linha2">{erro.linha}</div>
    </div>
  )
}

export function FeedErros({ erros, aoClicar, aoVivo }: Props) {
  return (
    <section>
      <div className="feed-titulo">
        <h2 className="kicker">Erros ao vivo</h2>
        <span
          className={`bolinha-aovivo ${aoVivo ? '' : 'parado'}`}
          title={aoVivo ? 'polling ok' : 'sem novos itens'}
        />
      </div>
      {erros.length === 0 ? (
        <p className="vazio">nenhum erro/crítico no feed ainda</p>
      ) : (
        <div className="feed-lista">
          {erros.map((e) => (
            <ItemErro key={e.id} erro={e} aoClicar={aoClicar} />
          ))}
        </div>
      )}
    </section>
  )
}
