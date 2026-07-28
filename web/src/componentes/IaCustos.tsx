// Tela IA · custos: KPIs (custo/tokens/horas/streak), heatmap do mês,
// horas por semana e ranking por modelo. Os dados vêm de /api/ia/custos
// (carregado sob demanda) + /api/ia/cambio (taxa USD->BRL, buscada ao vivo
// pelo servidor). Moeda padrão é R$ (escolha do usuário ao revisar o
// protótipo) — o toggle no header troca o "principal" e o "secundário".

import { useEffect, useState } from 'react'
import { buscarCambio, buscarCustosIa } from '../api'
import type { Cambio, CustosIa } from '../tipos'
import { formatarHorasMinutos, formatarMoeda, formatarNumeroCompacto } from '../formato'
import type { Moeda } from '../formato'
import { intensidadeParaCor } from '../formato'

const ROTULOS_SEMANA = ['seg', '', 'qua', '', 'sex', '', 'dom']

export function IaCustos() {
  const [dados, setDados] = useState<CustosIa | null>(null)
  const [cambio, setCambio] = useState<Cambio | null>(null)
  const [erro, setErro] = useState<string | null>(null)
  const [moeda, setMoeda] = useState<Moeda>('R$')

  useEffect(() => {
    let ativo = true
    Promise.all([buscarCustosIa(), buscarCambio()])
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
  }, [])

  // Tudo zerado / banco ausente: mostra estado vazio, sem KPIs zerados
  // (que dariam a impressão de bug).
  if (dados !== null && !dados.disponivel) {
    return (
      <main className="shell" data-screen-label="IA e custos">
        <header className="tela-header">
          <h1>IA · custos</h1>
          <span className="subtitulo">dados não disponíveis</span>
        </header>
        <p className="vazio">
          O banco do OpenCode não foi encontrado em <code>~/.local/share/opencode/opencode.db</code>.
          Rode o OpenCode pelo menos uma vez para popular os dados, ou aponte a env
          <code> DEV_CLI_OPENCODE_DB</code> para o caminho correto.
        </p>
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

  return (
    <main className="shell" data-screen-label="IA e custos">
      <header className="tela-header">
        <h1>IA · custos</h1>
        <span className="subtitulo">
          {dados.mes} · OpenCode · câmbio {formatarMoeda(1, cambio.usd_brl, 'R$')}
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

      {/* Faixa de KPIs: 4 células com divisórias internas por hairline.
          O `border-right` de cada KPI vira o divisor; o último não tem
          (a borda externa da faixa toda é do container). */}
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
        {dados.claude_disponivel ? (
          <Kpi
            rotulo="Horas com Claude"
            principal={formatarHorasMinutos(dados.claude_horas_mes * 60)}
            secundario={`média ${formatarHorasMinutos(dados.claude_media_horas_dia_ativo * 60)}/dia ativo`}
          />
        ) : (
          <Kpi
            rotulo="Horas com Claude"
            principal="—"
            secundario="sem sessões do Claude Code neste mês"
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
          <h2 className="kicker">Atividade — {mesLegivel(dados.mes)}</h2>
          <Heatmap dados={dados.heatmap} offset={dados.offset_semana_dia1} mes={dados.mes} />

          <h2 className="kicker" style={{ margin: '36px 0 14px' }}>Horas por semana</h2>
          {!dados.claude_disponivel || dados.claude_horas_por_semana.length === 0 ? (
            <p className="vazio">
              sem sessões do Claude Code neste mês — fonte: transcritos locais em{' '}
              <code>~/.claude/projects</code>.
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
            a cotação ao vivo do dia. Sessões abertas a noite inteira contam no máximo 4h,
            espelhando o dashboard do terminal.
          </p>
        </section>
      </div>
    </main>
  )
}

/// Um KPI: kicker + valor grande serif tnum + linha secundária.
/// Componente local: pequeno o suficiente para não merecer arquivo próprio.
function Kpi({ rotulo, principal, secundario }: { rotulo: string; principal: string; secundario: string }) {
  return (
    <div className="kpi">
      <div className="kpi-kicker">{rotulo}</div>
      <div className="kpi-valor">{principal}</div>
      <div className="kpi-secundario">{secundario}</div>
    </div>
  )
}

/// Heatmap mensal: grid 7 linhas x N colunas, com offset inicial para
/// alinhar o dia 1° com o dia da semana correto. `offset` (0 = segunda ...
/// 6 = domingo) vem do SERVIDOR (`offset_semana_dia1`, calculado com
/// `chrono` a partir do `mes` real) — antes era um valor fixo em 0 no
/// cliente, o que fazia o dia 1 sempre cair na linha "seg" mesmo quando o
/// mês começava em outro dia da semana.
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
  // Dias FUTUROS do mês corrente ficam transparentes (não faz sentido
  // pintar "sem atividade" num dia que ainda não aconteceu). Só se aplica
  // quando `mes` é o mês corrente — meses passados não têm "futuro".
  const hoje = new Date()
  const mesAtual = `${hoje.getFullYear()}-${String(hoje.getMonth() + 1).padStart(2, '0')}`
  const diaHoje = mesAtual === mes ? hoje.getDate() : Number.POSITIVE_INFINITY

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

/// Seção "Horas por semana": uma linha por semana (`.semanas-linha`, já
/// definida no CSS mas sem consumidor até então) — [rótulo 90px | barra
/// 10px | valor "18h18m"]. Dados vêm de `claude_horas_por_semana`
/// (`crates/servidor/src/ia.rs::calcular_horas_claude`).
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

/// Tabela de modelos: ranking por tokens DESC, com barra colorida por
/// modelo (sonnet=acento, opus=vermelho, haiku=verde, outros=neutro).
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

function mesLegivel(mes: string): string {
  // "2026-07" -> "julho de 2026" (pt-br best-effort, sem chrono JS).
  const [ano, m] = mes.split('-')
  const nomes = ['janeiro', 'fevereiro', 'março', 'abril', 'maio', 'junho',
    'julho', 'agosto', 'setembro', 'outubro', 'novembro', 'dezembro']
  const idx = Number(m) - 1
  if (idx < 0 || idx > 11) return mes
  return `${nomes[idx]} de ${ano}`
}

/// Abreviação do mês para o tooltip "D/mês" — a partir do `mes` da
/// RESPOSTA (`"YYYY-MM"`), não do relógio local: antes usava
/// `new Date().getMonth()`, o que rotulava errado qualquer mês que não
/// fosse o corrente (ex.: olhando julho em agosto, mostrava "ago").
function mesCurto(mes: string): string {
  const nomes = ['jan', 'fev', 'mar', 'abr', 'mai', 'jun',
    'jul', 'ago', 'set', 'out', 'nov', 'dez']
  const idx = Number(mes.split('-')[1]) - 1
  return nomes[idx] ?? ''
}
