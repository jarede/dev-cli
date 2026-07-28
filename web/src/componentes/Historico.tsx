// Tela Histórico: um strip de 24 células por container, com cor pela
// intensidade (0..=5) da contagem de erros+críticos naquela hora. Os
// dados vêm de /api/containers/historico (carregado sob demanda — esta
// tela não entra no polling de 15s para não martelar o servidor).

import { useEffect, useState } from 'react'
import { buscarHistorico } from '../api'
import type { HistoricoContainer as THistorico } from '../tipos'
import { formatarHora, formatarNumero, intensidadeParaCor } from '../formato'

const LEGENDA_NIVEIS = [0, 1, 2, 3, 4, 5]

export function Historico() {
  const [dados, setDados] = useState<THistorico[] | null>(null)
  const [erro, setErro] = useState<string | null>(null)

  useEffect(() => {
    let ativo = true
    buscarHistorico(24)
      .then((d) => {
        if (ativo) {
          setDados(d)
          setErro(null)
        }
      })
      .catch((e: unknown) => {
        if (ativo) setErro(e instanceof Error ? e.message : String(e))
      })
    return () => {
      ativo = false
    }
  }, [])

  return (
    <main className="shell" data-screen-label="Histórico">
      <header className="tela-header">
        <h1>Histórico</h1>
        <span className="subtitulo">erros e críticos por hora · últimas 24 horas</span>
      </header>

      {erro !== null && <div className="banner-api-fora">⚠ {erro}</div>}

      {dados === null && erro === null && (
        <p className="vazio">carregando…</p>
      )}

      {dados !== null && dados.length === 0 && (
        <p className="vazio">nenhum container conhecido — aguarde a primeira coleta</p>
      )}

      {dados !== null && dados.length > 0 && (
        <>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
            {dados.map((h) => (
              <div className="historico-linha" key={h.nome}>
                <span className="historico-nome">{h.nome}</span>
                <div className="historico-strip">
                  {h.horas.map((c) => (
                    // `intensidade` vem PRONTA do servidor
                    // (`nucleo::metricas::intensidade_log`, a MESMA função
                    // usada pelo heatmap mensal de custos de IA) — antes
                    // esta tela recalculava com faixas fixas chumbadas
                    // aqui, uma segunda escala que podia divergir da do
                    // heatmap. Ver `crates/nucleo/src/db.rs::historico_por_hora`.
                    <span
                      key={c.hora}
                      className="historico-celula"
                      style={{ background: intensidadeParaCor(c.intensidade) }}
                      title={`${formatarHora(c.hora)} · ${c.quantidade} erros/críticos`}
                    />
                  ))}
                </div>
                <span className="historico-total">{formatarNumero(h.total)}</span>
              </div>
            ))}
          </div>
          <div className="historico-rodape">
            <span>00h</span>
            <span style={{ flex: 1, borderTop: '1px solid var(--color-divider)' }} />
            <span>agora</span>
            <span className="historico-legenda">
              menos
              {LEGENDA_NIVEIS.map((n) => (
                <span
                  key={n}
                  className="quad"
                  style={{ background: intensidadeParaCor(n) }}
                />
              ))}
              mais
            </span>
          </div>
        </>
      )}
    </main>
  )
}
