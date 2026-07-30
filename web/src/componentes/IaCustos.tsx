import { useEffect, useState } from 'react'
import { buscarCambio, buscarCustosIa } from '../api'
import type { Cambio, CustosIa, FonteIa } from '../tipos'
import {
  formatarHorasMinutos,
  formatarMoeda,
  formatarNumeroCompacto,
  mesAnterior,
  mesAtual,
  mesPorExtenso,
  mesSeguinte,
} from '../formato'
import { intensidadeParaCor } from '../formato'
import type { Moeda } from '../formato'

const ROTULOS_SEMANA = ['seg', '', 'qua', '', 'sex', '', 'dom']

const OPCOES_FONTE: FonteIa[] = ['opencode', 'claude', 'ambos']
const ROTULO_FONTE: Record<FonteIa, string> = {
  opencode: 'OpenCode',
  claude: 'Claude',
  ambos: 'Ambos',
}

export function IaCustos() {
  const [dados, setDados] = useState<CustosIa | null>(null)
  const [cambio, setCambio] = useState<Cambio | null>(null)
  const [erro, setErro] = useState<string | null>(null)
  const [moeda, setMoeda] = useState<Moeda>('R$')
  const [mes, setMes] = useState(mesAtual)
  const [fonte, setFonte] = useState<FonteIa>('ambos')

  useEffect(() => {
    let ativo = true
    Promise.all([buscarCustosIa(mes, fonte), buscarCambio()])
      .then(([c, cb]) => {
        if (ativo) {
          setDados(c)
          setCambio(cb)
          setErro(null)
        }
      })
      .catch((e: unknown) => {
        if (ativo) setErro(e instanceof Error ? e.message : String(e))
      })
    return () => {
      ativo = false
    }
  }, [mes, fonte])

  // Estado vazio conforme a fonte (spec §3).
  if (dados !== null && !dados.disponivel) {
    const mensagem =
      fonte === 'opencode'
        ? 'O banco do OpenCode não foi encontrado em <code>~/.local/share/opencode/opencode.db</code>. Rode o OpenCode pelo menos uma vez para popular os dados, ou aponte a env <code>DEV_CLI_OPENCODE_DB</code> para o caminho correto.'
        : fonte === 'claude'
          ? 'Nenhuma sessão do Claude Code encontrada no mês. Os transcritos ficam em <code>~/.claude/projects</code>.'
          : 'Nenhuma das fontes tem dados neste mês.'
    return (
      <main className="shell" data-screen-label="IA e custos">
        <header className="tela-header">
          <h1>IA · custos</h1>
          <SeletorMes mes={mes} setMes={setMes} />
          <FonteSegmented fonte={fonte} setFonte={setFonte} />
          <span className="atualizado" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => setMoeda(moeda === 'R$' ? 'US$' : 'R$')}
              title="Trocar moeda principal"
            >
              {moeda === 'R$' ? 'mostrar em US$' : 'mostrar em R$'}
            </button>
          </span>
        </header>
        <p className="vazio" dangerouslySetInnerHTML={{ __html: mensagem }} />
      </main>
    )
  }

  if (erro !== null && dados === null) {
    return (
      <main className="shell" data-screen-label="IA e custos">
        <header className="tela-header">
          <h1>IA · custos</h1>
          <span className="subtitulo">carregando…</span>
        </header>
        <div className="banner-api-fora">⚠ {erro}</div>
      </main>
    )
  }

  if (dados === null || cambio === null) {
    return (
      <main className="shell" data-screen-label="IA e custos">
        <header className="tela-header">
          <h1>IA · custos</h1>
          <span className="subtitulo">carregando…</span>
        </header>
        {erro !== null && <div className="banner-api-fora">⚠ {erro}</div>}
      </main>
    )
  }

  // Subtítulo conforme a fonte e dados disponíveis.
  const subtituloFonte =
    fonte === 'ambos'
      ? dados.claude_disponivel
        ? 'OpenCode + Claude'
        : 'OpenCode'
      : ROTULO_FONTE[fonte]

  const mostrarClaude = fonte !== 'opencode' && dados.claude_disponivel

  return (
    <main className="shell" data-screen-label="IA e custos">
      <header className="tela-header">
        <h1>IA · custos</h1>
        <SeletorMes mes={mes} setMes={setMes} />
        <FonteSegmented fonte={fonte} setFonte={setFonte} />
        <span className="subtitulo">
          {subtituloFonte} · câmbio {formatarMoeda(1, cambio.usd_brl, 'R$')}
        </span>
        <span className="atualizado" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => setMoeda(moeda === 'R$' ? 'US$' : 'R$')}
            title="Trocar moeda principal"
          >
            {moeda === 'R$' ? 'mostrar em US$' : 'mostrar em R$'}
          </button>
        </span>
      </header>

      <div className="kpis">
        <Kpi
          rotulo="Custo no mês"
          principal={formatarMoeda(dados.custo_usd, cambio.usd_brl, moeda)}
          secundario={
            moeda === 'R$'
              ? `≈ ${formatarMoeda(dados.custo_usd, cambio.usd_brl, 'US$')}`
              : `≈ ${formatarMoeda(dados.custo_usd, cambio.usd_brl, 'R$')}`
          }
        />
        <Kpi
          rotulo="Tokens"
          principal={formatarNumeroCompacto(dados.tokens)}
          secundario={dados.cache_pct > 0 ? `${Math.round(dados.cache_pct)}% em cache` : 'sem cache'}
        />
        {mostrarClaude ? (
          <Kpi
            rotulo="Horas com Claude"
            principal={formatarHorasMinutos(dados.claude_horas_mes * 60)}
            secundario={`média ${formatarHorasMinutos(dados.claude_media_horas_dia_ativo * 60)}/dia ativo`}
          />
        ) : (
          <Kpi
            rotulo="Horas com Claude"
            principal="—"
            secundario={fonte === 'opencode' ? 'apenas OpenCode' : 'sem sessões do Claude Code neste mês'}
          />
        )}
        <Kpi
          rotulo="Streak"
          principal={`${dados.streak_dias} ${dados.streak_dias === 1 ? 'dia' : 'dias'}`}
          secundario={`melhor: ${dados.melhor_streak_dias} ${dados.melhor_streak_dias === 1 ? 'dia' : 'dias'}`}
        />
      </div>

      <div className="ia-grid">
        <section>
          <h2 className="kicker">Atividade — {mesPorExtenso(dados.mes)}</h2>
          <Heatmap dados={dados.heatmap} offset={dados.offset_semana_dia1} mes={dados.mes} />

          <h2 className="kicker" style={{ margin: '36px 0 14px' }}>Horas por semana</h2>
          {!mostrarClaude || dados.claude_horas_por_semana.length === 0 ? (
            <p className="vazio">
              {fonte === 'opencode'
                ? 'apenas OpenCode — horas do Claude não fazem parte desta fonte.'
                : 'sem sessões do Claude Code neste mês.'}
            </p>
          ) : (
            <HorasPorSemana semanas={dados.claude_horas_por_semana} />
          )}
        </section>
        <section>
          <h2 className="kicker">Por modelo</h2>
          {dados.modelos.length === 0 ? (
            <p className="vazio">nenhum modelo usado no mês</p>
          ) : (
            <TabelaModelos modelos={dados.modelos} cambio={cambio.usd_brl} moeda={moeda} />
          )}
          <p className="ia-nota">
            Custos estimados pela tabela de preços por modelo; a conversão para reais usa
            a cotação ao vivo do dia.
          </p>
        </section>
      </div>
    </main>
  )
}

function SeletorMes({
  mes,
  setMes,
}: {
  mes: string
  setMes: (m: string) => void
}) {
  return (
    <span className="seletor-mes">
      <button type="button" className="btn btn-ghost" aria-label="Mês anterior"
        onClick={() => setMes(mesAnterior(mes))}>‹</button>
      <span className="seletor-mes-rotulo">{mesPorExtenso(mes)}</span>
      <button type="button" className="btn btn-ghost" aria-label="Mês seguinte"
        disabled={mes === mesAtual()}
        onClick={() => setMes(mesSeguinte(mes))}>›</button>
    </span>
  )
}

function FonteSegmented({
  fonte,
  setFonte,
}: {
  fonte: FonteIa
  setFonte: (f: FonteIa) => void
}) {
  return (
    <div className="seg" role="radiogroup" aria-label="Fonte dos dados">
      {OPCOES_FONTE.map((op) => (
        <label key={op} className={`seg-opt ${fonte === op ? 'selected' : ''}`}>
          <input type="radio" name="fonte-ia" className="sr-only" value={op}
            checked={fonte === op} onChange={() => setFonte(op)}
            aria-label={ROTULO_FONTE[op]} />
          {ROTULO_FONTE[op]}
        </label>
      ))}
    </div>
  )
}

function Kpi({ rotulo, principal, secundario }: { rotulo: string; principal: string; secundario: string }) {
  return (
    <div className="kpi">
      <div className="kpi-kicker">{rotulo}</div>
      <div className="kpi-valor">{principal}</div>
      <div className="kpi-secundario">{secundario}</div>
    </div>
  )
}

function Heatmap({
  dados,
  offset,
  mes,
}: {
  dados: import('../tipos').CelulaHeatmap[]
  offset: number
  mes: string
}) {
  if (dados.length === 0) {
    return <p className="vazio">sem dados de atividade no mês</p>
  }
  const hoje = new Date()
  const mesCorrente = `${hoje.getFullYear()}-${String(hoje.getMonth() + 1).padStart(2, '0')}`
  const diaHoje = mesCorrente === mes ? hoje.getDate() : Number.POSITIVE_INFINITY

  return (
    <div className="heatmap-linha">
      <div className="heatmap-rotulos">
        {ROTULOS_SEMANA.map((r, i) => (
          <span key={i}>{r}</span>
        ))}
      </div>
      <div className="heatmap">
        {Array.from({ length: offset }).map((_, i) => (
          <span key={`offset-${i}`} className="heatmap-celula futuro" />
        ))}
        {dados.map((c) => {
          const futuro = c.dia > diaHoje
          return (
            <span
              key={c.dia}
              className={`heatmap-celula${futuro ? ' futuro' : ''}`}
              style={futuro ? undefined : { background: intensidadeParaCor(c.intensidade) }}
              title={futuro ? undefined : `${c.dia}/${mesCurto(mes)} — nível ${c.intensidade}`}
            />
          )
        })}
      </div>
    </div>
  )
}

function HorasPorSemana({ semanas }: { semanas: import('../tipos').SemanaHoras[] }) {
  const maxHoras = semanas.reduce((m, s) => Math.max(m, s.horas), 0)
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {semanas.map((s) => {
        const larg = maxHoras > 0 ? (s.horas / maxHoras) * 100 : 0
        return (
          <div className="semanas-linha" key={s.rotulo}>
            <span className="semanas-rotulo">{s.rotulo}</span>
            <div className="barra-trilho">
              <div className="barra-preenchimento" style={{ width: `${larg}%` }} />
            </div>
            <span className="semanas-valor">{formatarHorasMinutos(s.horas * 60)}</span>
          </div>
        )
      })}
    </div>
  )
}

function TabelaModelos({
  modelos,
  cambio,
  moeda,
}: {
  modelos: import('../tipos').ModeloCusto[]
  cambio: number
  moeda: Moeda
}) {
  const maxCusto = modelos.reduce((m, x) => Math.max(m, x.custo_usd), 0)
  return (
    <table className="table">
      <thead>
        <tr>
          <th>modelo</th>
          <th className="num">tokens</th>
          <th className="num">custo</th>
          <th style={{ width: '34%' }}></th>
        </tr>
      </thead>
      <tbody>
        {modelos.map((m) => {
          const cor = corModelo(m.modelo)
          const larg = maxCusto > 0 ? (m.custo_usd / maxCusto) * 100 : 0
          return (
            <tr key={m.modelo + m.provedor}>
              <td style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}>{m.modelo}</td>
              <td className="num">{formatarNumeroCompacto(m.tokens)}</td>
              <td className="num">{formatarMoeda(m.custo_usd, cambio, moeda)}</td>
              <td>
                <div className="barra-trilho">
                  <div
                    className="barra-preenchimento"
                    style={{ width: `${larg}%`, background: cor }}
                  />
                </div>
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}

function corModelo(nome: string): string {
  const n = nome.toLowerCase()
  if (n.includes('opus')) return 'var(--sev-vermelho)'
  if (n.includes('sonnet')) return 'var(--color-accent)'
  if (n.includes('haiku')) return 'var(--sev-verde)'
  return 'var(--color-neutral-500)'
}

function mesCurto(mes: string): string {
  const nomes = ['jan', 'fev', 'mar', 'abr', 'mai', 'jun',
    'jul', 'ago', 'set', 'out', 'nov', 'dez']
  const idx = Number(mes.split('-')[1]) - 1
  return nomes[idx] ?? ''
}