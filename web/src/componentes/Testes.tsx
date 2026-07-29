// Tela Testes · e2e: lista de suítes (pipelines de comandos CLI encadeados)
// com expand/collapse, stepper horizontal, detalhes de cada etapa (com
// failure boxes para erros de execução e validação) e formulário de cadastro
// de novas suítes.
//
// A UI é puramente apresentacional — toda a parte de execução mora no
// dev-server: o `rodar` retorna um `id_execucao` e o componente FAZ
// POLLING de `/api/testes/execucoes/{id}` a cada 1s enquanto a execução
// está em `rodando`.
//
// Esta é a única tela do portal que ESCREVE (cria suítes e dispara
// execuções) — todas as outras são somente-leitura. A persistência é
// externa: as suítes vão para `/etc/dev-cli/testes/<id>.toml` no
// servidor, e as execuções em si vivem em memória.
//
// docs: docs/design_handoff_portal_dev_cli/README.md (seção 5)

import { useCallback, useEffect, useRef, useState } from 'react'
import {
  buscarExecucao,
  buscarHistoricoExecucoes,
  buscarSuites,
  cancelarExecucao,
  criarSuite,
  rodarSuite,
} from '../api'
import { formatarHaQuanto } from '../formato'
import type { ConclusaoExecucao, EstadoPasso, Execucao, Passo, Suite } from '../tipos'

/// Intervalo de polling do estado de uma execução em andamento. 1s:
/// granular para o "rodando…" parecer ao vivo, sem martelar o servidor.
const INTERVALO_POLLING_MS = 1_000

/// Limite de caracteres do "esperado" (coluna do form) — generoso porque
/// o usuário pode querer colar um trecho longo de log esperado.
const LIMITE_ESPERADO = 4_000

/// Estado de uma suíte na lista: o que veio da API + o estado da
/// última/presente execução (se houver) para renderizar a bolinha e o
/// stepper. Manter a execução junto da suíte simplifica o render — a
/// alternativa seria um `Map<id_suite, Execucao>` separado.
interface SuiteNaLista {
  suite: Suite
  execucao?: Execucao
  /// Aberto/fechado no header (toggle).
  aberto: boolean
}

export function Testes() {
  const [suites, setSuites] = useState<SuiteNaLista[] | null>(null)
  const [erro, setErro] = useState<string | null>(null)
  const [cadastro, setCadastro] = useState<DadosCadastro | null>(null)
  // `rodandoId`: ID da suíte que está rodando agora — usado pelo botão
  // "Rodando…" no card da suíte.
  const [rodandoId, setRodandoId] = useState<string | null>(null)
  // Histórico de execuções por suíte — carregado sob demanda (ao expandir
  // o card, ou depois que uma execução termina). `undefined` = ainda não
  // buscado (não confundir com `[]`, que é "buscado e vazio": a suíte
  // nunca rodou).
  const [historicos, setHistoricos] = useState<Record<string, Execucao[]>>({})

  const carregarHistorico = useCallback(async (id: string) => {
    try {
      const lista = await buscarHistoricoExecucoes(id)
      setHistoricos((atual) => ({ ...atual, [id]: lista }))
    } catch {
      // Histórico é um extra dentro do card já expandido — se falhar,
      // não vale a pena um banner de erro global por isso.
    }
  }, [])

  // Busca inicial + quando o cadastro é salvo (efeito simples, sem
  // polling: as suítes mudam só quando alguém cria).
  const carregar = useCallback(async () => {
    try {
      const lista = await buscarSuites()
      // Preserva o estado `aberto` e a `execucao` das suítes que já
      // estavam na lista (toggle não pode ser resetado a cada poll).
      setSuites((atual) => {
        const map = new Map((atual ?? []).map((s) => [s.suite.id, s]))
        return lista.map((suite) => {
          const anterior = map.get(suite.id)
          return anterior
            ? { ...anterior, suite }
            : { suite, aberto: false }
        })
      })
      setErro(null)
    } catch (e: unknown) {
      setErro(e instanceof Error ? e.message : String(e))
    }
  }, [])

  useEffect(() => {
    void carregar()
  }, [carregar])

  // Polling do estado da execução em andamento. Roda enquanto houver
  // uma `rodandoId` (suite.id) — busca a execucao pelo id_execucao da
  // suite. Quando termina, limpa o rodandoId.
  //
  // `idExecRef` mantém o id_execucao atual sem entrar como dep do
  // effect: se colocássemos `suites` como dep, qualquer atualização do
  // `setSuites` do PRÓPRIO callback do polling cancelaria e reiniciaria
  // o intervalo a cada poll — nunca passando de 1s e dando race no
  // cleanup. docs: https://react.dev/reference/react/useRef
  const idExecRef = useRef<string | null>(null)
  useEffect(() => {
    if (rodandoId === null) return
    const suite = suites?.find((s) => s.suite.id === rodandoId)
    const idExec = suite?.execucao?.id_execucao
    if (!idExec) return
    idExecRef.current = idExec
    let ativo = true
    const timer = setInterval(async () => {
      if (!ativo) return
      const id = idExecRef.current
      if (id === null) return
      try {
        const exec = await buscarExecucao(id)
        if (!ativo) return
        const suiteAlvo = rodandoId
        setSuites((atual) =>
          (atual ?? []).map((s) =>
            s.suite.id === suiteAlvo
              ? { ...s, execucao: exec, aberto: true }
              : s,
          ),
        )
        if (exec.conclusao !== 'rodando') {
          idExecRef.current = null
          setRodandoId(null)
          // A execução que acabou de terminar já entra no histórico —
          // recarrega para o card mostrá-la sem esperar um novo expand.
          void carregarHistorico(suiteAlvo)
        }
      } catch (e: unknown) {
        if (!ativo) return
        setErro(e instanceof Error ? e.message : String(e))
        idExecRef.current = null
        setRodandoId(null)
      }
    }, INTERVALO_POLLING_MS)
    return () => {
      ativo = false
      clearInterval(timer)
    }
  }, [rodandoId, suites, carregarHistorico])

  const aoAlternar = (id: string) => {
    const estaAberto = suites?.find((s) => s.suite.id === id)?.aberto ?? false
    setSuites((atual) =>
      (atual ?? []).map((s) => (s.suite.id === id ? { ...s, aberto: !s.aberto } : s)),
    )
    // Busca o histórico só ao ABRIR (não ao fechar) e só na primeira vez
    // (`historicos[id] === undefined`) — reabrir não repete a chamada;
    // quem quiser dados mais frescos usa o refresh automático pós-run.
    if (!estaAberto && historicos[id] === undefined) {
      void carregarHistorico(id)
    }
  }

  const aoRodar = async (id: string) => {
    if (rodandoId !== null) return
    try {
      const inicio = await rodarSuite(id)
      setRodandoId(id)
      // Cria um Execucao inicial "vazio" com conclusao=rodando
      // imediatamente — assim o polling já tem algo para atualizar.
      setSuites((atual) =>
        (atual ?? []).map((s) =>
          s.suite.id === id
            ? {
                ...s,
                aberto: true,
                execucao: {
                  id_execucao: inicio.id_execucao,
                  suite_id: id,
                  iniciada_em_unix: Math.floor(Date.now() / 1000),
                  estados: s.suite.passos.map(() => ({ status: 'pendente' })),
                  conclusao: 'rodando',
                },
              }
            : s,
        ),
      )
      setErro(null)
    } catch (e: unknown) {
      setErro(e instanceof Error ? e.message : String(e))
    }
  }

  const aoCancelar = async (idExecucao: string) => {
    try {
      await cancelarExecucao(idExecucao)
    } catch (e: unknown) {
      setErro(e instanceof Error ? e.message : String(e))
    }
  }

  const aoSalvarCadastro = async (suite: Suite) => {
    try {
      // POST é upsert (o servidor grava por cima do TOML existente se o
      // `id` já existir) — criação e edição usam o MESMO endpoint; a
      // diferença é só se o `id` veio de `idAPartirDoNome` (suíte nova)
      // ou preservado de `idOriginal` (edição, ver `aoIniciarEdicao`).
      await criarSuite(suite)
      setCadastro(null)
      await carregar()
      // Abre a suíte (nova ou editada) automaticamente.
      setSuites((atual) =>
        (atual ?? []).map((s) => (s.suite.id === suite.id ? { ...s, aberto: true } : s)),
      )
    } catch (e: unknown) {
      // 400 com mensagem do servidor (id inválido etc.) — o componente
      // do form já valida antes, mas defletimos o erro também aqui.
      throw e instanceof Error ? e : new Error(String(e))
    }
  }

  /// Abre o formulário pré-preenchido com os dados de uma suíte existente.
  /// `idOriginal` é o que diferencia de "Nova suíte": mantém o `id` (e,
  /// portanto, o arquivo `<id>.toml`) fixo mesmo que o usuário mude o nome.
  const aoIniciarEdicao = (suite: Suite) => {
    setCadastro({
      nome: suite.nome,
      timeout_etapa_seg: suite.timeout_etapa_seg,
      passos: suite.passos,
      idOriginal: suite.id,
    })
  }

  return (
    <main className="shell" data-screen-label="Testes e2e">
      <header className="tela-header">
        <h1>Testes · e2e</h1>
        <span className="subtitulo">
          pipelines de integração · <code className="caminho-mono">dev-cli testes rodar</code>
        </span>
        <span className="atualizado" style={{ marginLeft: 'auto', display: 'flex', gap: 14, alignItems: 'center' }}>
          <Resumo suites={suites} />
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => setCadastro({ nome: '', timeout_etapa_seg: 30, passos: [{ tipo: 'exec', cmd: '' }] })}
            style={{ padding: '5px 16px', fontSize: 12.5 }}
          >
            Nova suíte
          </button>
        </span>
      </header>

      <p className="testes-explicacao">
        Cada suíte é uma pipeline de comandos CLI encadeados: uma etapa de
        execução seguida de validações dos dados produzidos. As etapas rodam em
        ordem; se uma falha — por erro de execução (exit ≠ 0) ou por validação
        (exit 0 mas stdout ≠ esperado) — a pipeline para e as etapas seguintes
        não executam.
      </p>

      {erro !== null && <div className="banner-api-fora">⚠ {erro}</div>}

      {cadastro !== null && (
        <FormularioCadastro
          dados={cadastro}
          aoMudar={setCadastro}
          aoSalvar={aoSalvarCadastro}
          aoCancelar={() => setCadastro(null)}
        />
      )}

      {suites === null && erro === null && (
        <p className="vazio">carregando…</p>
      )}

      {suites !== null && suites.length === 0 && cadastro === null && (
        <p className="vazio">
          nenhuma suíte encontrada. Crie uma com <strong>Nova suíte</strong> ou
          adicione um arquivo <code className="caminho-mono">&lt;id&gt;.toml</code> em{' '}
          <code className="caminho-mono">/etc/dev-cli/testes/</code>.
        </p>
      )}

      {suites !== null && suites.length > 0 && (
        <div className="testes-lista">
          {suites.map((s) => (
            <CartaoSuite
              key={s.suite.id}
              item={s}
              rodando={rodandoId === s.suite.id}
              historico={historicos[s.suite.id]}
              aoAlternar={() => aoAlternar(s.suite.id)}
              aoRodar={() => void aoRodar(s.suite.id)}
              aoCancelar={() =>
                s.execucao !== undefined && void aoCancelar(s.execucao.id_execucao)
              }
              aoEditar={() => aoIniciarEdicao(s.suite)}
            />
          ))}
        </div>
      )}
    </main>
  )
}

// ─── Resumo no header ────────────────────────────────────────────────

function Resumo({ suites }: { suites: SuiteNaLista[] | null }) {
  if (suites === null) return null
  const total = suites.length
  const falhando = suites.filter((s) => s.execucao?.conclusao === 'falha').length
  // "última execução" — para simplificar, omitimos o "há X" e mostramos
  // só o total + quantas falhando; o portal tem outras telas com
  // "atualizado há Xs" e repetir o timestamp aqui é ruído.
  return (
    <span className="testes-resumo">
      {total} suíte{total === 1 ? '' : 's'} · {falhando} falhando
    </span>
  )
}

// ─── Cartão da suíte ─────────────────────────────────────────────────

function CartaoSuite({
  item,
  rodando,
  historico,
  aoAlternar,
  aoRodar,
  aoCancelar,
  aoEditar,
}: {
  item: SuiteNaLista
  rodando: boolean
  /// `undefined` = ainda não buscado (a UI não mostra a seção enquanto
  /// isso); `[]` = buscado e a suíte nunca rodou.
  historico: Execucao[] | undefined
  aoAlternar: () => void
  aoRodar: () => void
  aoCancelar: () => void
  aoEditar: () => void
}) {
  const { suite, execucao, aberto } = item
  // Bolinha da suíte: verde = última execução sucesso / nunca rodada,
  // vermelho = última execução falhou, acento = rodando agora.
  let corSuite = 'var(--sev-verde)'
  let resumoSuite = `${suite.passos.length} etapa${suite.passos.length === 1 ? '' : 's'} · todas passaram`
  let corResumo = 'var(--color-neutral-500)'
  if (rodando) {
    corSuite = 'var(--color-accent)'
    const atual = execucao?.estados.findIndex((e) => e.status === 'rodando') ?? 0
    resumoSuite = `rodando etapa ${atual + 1} de ${suite.passos.length}…`
  } else if (execucao !== undefined && execucao.conclusao === 'falha') {
    corSuite = 'var(--sev-vermelho)'
    const idxFalha = execucao.estados.findIndex(
      (e) => e.status === 'falha_exec' || e.status === 'falha_valida',
    )
    const passaram = execucao.estados.filter((e) => e.status === 'ok').length
    resumoSuite = `${passaram} de ${suite.passos.length} etapas · parou na etapa ${idxFalha + 1}`
    corResumo = 'var(--sev-vermelho)'
  }
  // "duração" da suíte — uma string derivada da execucao.iniciada_em_unix
  // (o servidor não calcula nem devolve, mas dá para mostrar "Xs" no
  // header quando terminada).
  let duracao = '—'
  if (execucao && execucao.conclusao !== 'rodando') {
    const seg = Math.max(0, Math.floor(Date.now() / 1000) - execucao.iniciada_em_unix)
    duracao = seg < 60 ? `${seg.toFixed(1)}s` : `${Math.floor(seg / 60)}m${String(seg % 60).padStart(2, '0')}s`
  }

  return (
    <section className="card card-testes">
      <div className="testes-cabecalho" onClick={aoAlternar} role="button" tabIndex={0}
        onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); aoAlternar() } }}>
        <span className="bolinha" style={{ background: corSuite }} title={corSuite.includes('verde') ? 'passou' : corSuite.includes('vermelho') ? 'falhou' : 'rodando'} />
        <span className="testes-nome">{suite.nome}</span>
        <span className="testes-resumo-card" style={{ color: corResumo }}>{resumoSuite}</span>
        <span className="testes-duracao" style={{ marginLeft: 'auto' }}>{duracao}</span>
        {!rodando && (
          <button
            type="button"
            className="btn btn-ghost"
            onClick={(e) => { e.stopPropagation(); aoEditar() }}
            style={{ padding: '4px 14px', fontSize: 12.5 }}
            title="Editar nome, timeout e etapas desta suíte"
          >
            Editar
          </button>
        )}
        {rodando ? (
          <button
            type="button"
            className="btn btn-ghost"
            onClick={(e) => { e.stopPropagation(); aoCancelar() }}
            style={{ padding: '4px 14px', fontSize: 12.5 }}
            title="Cancelar a execução em andamento"
          >
            Cancelar
          </button>
        ) : (
          <button
            type="button"
            className="btn btn-ghost"
            onClick={(e) => { e.stopPropagation(); aoRodar() }}
            disabled={rodando}
            style={{ padding: '4px 14px', fontSize: 12.5 }}
          >
            Rodar
          </button>
        )}
      </div>
      {aberto && <CorpoSuite suite={suite} execucao={execucao} historico={historico} />}
    </section>
  )
}

// ─── Corpo expandido ─────────────────────────────────────────────────

function CorpoSuite({
  suite,
  execucao,
  historico,
}: {
  suite: Suite
  execucao?: Execucao
  historico: Execucao[] | undefined
}) {
  // Calcula o estado "efetivo" de cada passo: a execucao atual sobrescreve
  // o que está na suite (a suite tem só o "resultado" esperado pra
  // simulação — em produção os estados vêm todos do servidor).
  const estados: EstadoPasso[] = suite.passos.map((_, i) => {
    if (execucao?.estados[i]) return execucao.estados[i]
    return { status: 'pendente' }
  })
  return (
    <div className="testes-corpo">
      {/* Stepper horizontal: chips "cli 1 → cli 2 → …" */}
      <div className="testes-stepper">
        {suite.passos.map((_, i) => (
          <StepperChip key={i} estado={estados[i]} idx={i} temSeta={i < suite.passos.length - 1} />
        ))}
      </div>
      <div className="testes-passos">
        {suite.passos.map((passo, i) => (
          <LinhaPasso
            key={i}
            num={i + 1}
            passo={passo}
            estado={estados[i]}
          />
        ))}
      </div>
      <HistoricoExecucoes suite={suite} historico={historico} />
    </div>
  )
}

// ─── Histórico de execuções ──────────────────────────────────────────

/// Cor + rótulo pela conclusão da execução (não pelo passo individual —
/// aqui é sempre a suíte inteira, já terminada ou ainda rodando).
function corConclusao(conclusao: ConclusaoExecucao): string {
  switch (conclusao) {
    case 'sucesso': return 'var(--sev-verde)'
    case 'falha': return 'var(--sev-vermelho)'
    case 'rodando': return 'var(--color-accent-700)'
  }
}

function rotuloConclusao(conclusao: ConclusaoExecucao): string {
  switch (conclusao) {
    case 'sucesso': return 'passou'
    case 'falha': return 'falhou'
    case 'rodando': return 'rodando…'
  }
}

/// Lista de execuções passadas da suíte, mais recente primeiro. Cada linha
/// é clicável e expande inline as etapas DAQUELA execução (reaproveitando
/// `LinhaPasso`) — útil para investigar uma falha antiga sem rodar de novo.
///
/// Não mostra "duração": o servidor não grava um timestamp de término, só
/// o de início (`iniciada_em_unix`) — calcular "agora - início" para uma
/// execução JÁ TERMINADA no passado daria um número que cresce a cada
/// re-render, não a duração real. Mostramos só "há Xmin" (quando começou).
function HistoricoExecucoes({
  suite,
  historico,
}: {
  suite: Suite
  historico: Execucao[] | undefined
}) {
  const [abertaId, setAbertaId] = useState<string | null>(null)

  if (historico === undefined) {
    return <p className="testes-historico-vazio">carregando histórico…</p>
  }
  if (historico.length === 0) {
    return <p className="testes-historico-vazio">nenhuma execução anterior registrada.</p>
  }

  return (
    <div className="testes-historico">
      <div className="kicker">Histórico · últimas execuções</div>
      <div className="testes-historico-lista">
        {historico.map((exec) => {
          const aberta = abertaId === exec.id_execucao
          return (
            <div key={exec.id_execucao}>
              <button
                type="button"
                className="testes-historico-linha"
                onClick={() => setAbertaId(aberta ? null : exec.id_execucao)}
              >
                <span className="bolinha" style={{ background: corConclusao(exec.conclusao) }} />
                <span className="testes-historico-quando">
                  {formatarHaQuanto(exec.iniciada_em_unix)}
                </span>
                <span
                  className="testes-historico-conclusao"
                  style={{ color: corConclusao(exec.conclusao) }}
                >
                  {rotuloConclusao(exec.conclusao)}
                </span>
              </button>
              {aberta && (
                <div className="testes-passos testes-historico-passos">
                  {suite.passos.map((passo, i) => (
                    <LinhaPasso
                      key={i}
                      num={i + 1}
                      passo={passo}
                      estado={exec.estados[i] ?? { status: 'pendente' }}
                    />
                  ))}
                </div>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )
}

function StepperChip({
  estado,
  idx,
  temSeta,
}: {
  estado: EstadoPasso
  idx: number
  temSeta: boolean
}) {
  const { borda, cor, fundo } = estiloChip(estado)
  return (
    <>
      <span
        className="testes-chip"
        style={{ borderColor: borda, color: cor, background: fundo }}
      >
        cli {idx + 1}
      </span>
      {temSeta && <span className="testes-chip-seta">→</span>}
    </>
  )
}

function estiloChip(estado: EstadoPasso): { borda: string; cor: string; fundo: string } {
  if (estado.status === 'ok') {
    return { borda: 'var(--sev-verde)', cor: 'var(--sev-verde)', fundo: 'transparent' }
  }
  if (estado.status === 'falha_exec' || estado.status === 'falha_valida') {
    return { borda: 'var(--sev-vermelho)', cor: 'var(--sev-vermelho)', fundo: 'transparent' }
  }
  if (estado.status === 'rodando') {
    return { borda: 'var(--color-accent)', cor: 'var(--color-accent-700)', fundo: 'color-mix(in srgb, var(--color-accent) 8%, transparent)' }
  }
  return { borda: 'var(--color-divider)', cor: 'var(--color-neutral-400)', fundo: 'transparent' }
}

function LinhaPasso({
  num,
  passo,
  estado,
}: {
  num: number
  passo: Passo
  estado: EstadoPasso
}) {
  const opacidade = estado.status === 'pulado' || estado.status === 'pendente' ? 0.5 : 1
  return (
    <div className="testes-linha" style={{ opacity: opacidade }}>
      <span className="testes-linha-num">{num}.</span>
      <span className={`tag ${passo.tipo === 'exec' ? 'tag-accent' : 'tag-neutral'}`} style={{ alignSelf: 'start', justifySelf: 'start' }}>
        {passo.tipo === 'exec' ? 'execução' : 'validação'}
      </span>
      <div style={{ minWidth: 0 }}>
        <div className="testes-rotulo">{rotuloPasso(passo, estado)}</div>
        <div className="testes-comando">$ {passo.cmd}</div>
        {estado.status === 'falha_valida' && (
          <FalhaValida esperado={estado.esperado} obtido={estado.obtido} />
        )}
        {estado.status === 'falha_exec' && (
          <FalhaExec exit={estado.exit_code} stderr={estado.stderr} />
        )}
        {estado.status === 'ok' && estado.saida !== undefined && estado.saida !== '' && (
          <div className="testes-saida">↳ {estado.saida}</div>
        )}
      </div>
      <span className="testes-status" style={{ color: corStatus(estado) }}>{rotuloStatus(estado)}</span>
    </div>
  )
}

function rotuloPasso(passo: Passo, estado: EstadoPasso): string {
  if (passo.tipo === 'exec') return 'Executa o comando'
  if (estado.status === 'falha_valida') {
    return `Valida: stdout = "${estado.esperado}"`
  }
  return `Valida: stdout = "${passo.esperado ?? ''}"`
}

function rotuloStatus(estado: EstadoPasso): string {
  switch (estado.status) {
    case 'pendente': return 'aguardando'
    case 'rodando': return 'rodando…'
    case 'ok': return 'passou'
    case 'falha_exec': return 'falhou · execução'
    case 'falha_valida': return 'falhou · validação'
    case 'pulado': return 'não executado'
  }
}

function corStatus(estado: EstadoPasso): string {
  switch (estado.status) {
    case 'ok': return 'var(--sev-verde)'
    case 'falha_exec':
    case 'falha_valida': return 'var(--sev-vermelho)'
    case 'rodando': return 'var(--color-accent-700)'
    case 'pendente':
    case 'pulado': return 'var(--color-neutral-400)'
  }
}

function FalhaValida({ esperado, obtido }: { esperado: string; obtido: string }) {
  return (
    <div className="testes-falha">
      <div className="testes-falha-cabecalho">Comando executou sem erro (exit 0), mas o dado não bate:</div>
      <div className="testes-falha-grid">
        <span className="testes-falha-rotulo">esperado</span>
        <span style={{ color: 'var(--sev-verde)' }}>{esperado}</span>
        <span className="testes-falha-rotulo">obtido</span>
        <span style={{ color: 'var(--sev-vermelho)' }}>{obtido}</span>
      </div>
    </div>
  )
}

function FalhaExec({ exit, stderr }: { exit: number; stderr: string }) {
  return (
    <div className="testes-falha">
      <div className="testes-falha-cabecalho">Falha de execução — exit {exit}:</div>
      <pre className="testes-falha-stderr">{stderr}</pre>
    </div>
  )
}

// ─── Formulário de cadastro ──────────────────────────────────────────

interface DadosCadastro {
  nome: string
  timeout_etapa_seg: number
  passos: Passo[]
  /// Presente SÓ quando o formulário está editando uma suíte existente —
  /// o `id` (e portanto o arquivo `<id>.toml`) fica fixo nesse caso, para
  /// editar o nome não criar um TOML novo e deixar o antigo órfão.
  idOriginal?: string
}

/// Valida o formulário e devolve uma mensagem de erro (ou null se ok).
function validar(d: DadosCadastro): string | null {
  if (d.nome.trim() === '') return 'Dê um nome à suíte.'
  if (d.passos.length === 0) return 'A suíte precisa ter pelo menos uma etapa.'
  if (d.passos.some((p) => p.cmd.trim() === '')) {
    return 'Toda etapa precisa de um comando CLI.'
  }
  if (d.passos.some((p) => p.tipo === 'valida' && (p.esperado ?? '').trim() === '')) {
    return 'Etapas de validação precisam da saída esperada.'
  }
  return null
}

function idAPartirDoNome(nome: string): string {
  // Slug simples: minúsculas, sem acentos (best-effort), troca espaço
  // por "-", tira caracteres não seguros. O usuário pode editar depois
  // se quiser; o servidor revalida.
  return nome
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9 _-]/g, '')
    .trim()
    .replace(/\s+/g, '-')
    .slice(0, 64)
}

function FormularioCadastro({
  dados,
  aoMudar,
  aoSalvar,
  aoCancelar,
}: {
  dados: DadosCadastro
  aoMudar: (d: DadosCadastro) => void
  aoSalvar: (suite: Suite) => Promise<void>
  aoCancelar: () => void
}) {
  const [erro, setErro] = useState<string | null>(null)
  const erroRef = useRef<HTMLDivElement | null>(null)

  const aoMudarPasso = (i: number, patch: Partial<Passo>) => {
    aoMudar({
      ...dados,
      passos: dados.passos.map((p, j) => (i === j ? { ...p, ...patch } : p)),
    })
  }
  const aoAdicionarPasso = () => {
    aoMudar({
      ...dados,
      passos: [...dados.passos, { tipo: 'valida', cmd: '' }],
    })
  }
  const aoRemoverPasso = (i: number) => {
    if (dados.passos.length <= 1) return
    aoMudar({
      ...dados,
      passos: dados.passos.filter((_, j) => j !== i),
    })
  }
  const aoSubmeter = async (e: React.FormEvent) => {
    e.preventDefault()
    const msg = validar(dados)
    if (msg !== null) {
      setErro(msg)
      return
    }
    // Na edição, o `id` fica FIXO (o mesmo arquivo é sobrescrito) mesmo
    // que o nome mude; só suíte nova deriva o id do nome digitado.
    const id = dados.idOriginal ?? idAPartirDoNome(dados.nome)
    if (id === '') {
      setErro('O nome precisa ter pelo menos uma letra ou número.')
      return
    }
    const suite: Suite = {
      id,
      nome: dados.nome.trim(),
      timeout_etapa_seg: dados.timeout_etapa_seg,
      passos: dados.passos.map((p) => ({
        tipo: p.tipo,
        cmd: p.cmd.trim(),
        ...(p.tipo === 'valida' ? { esperado: (p.esperado ?? '').trim() } : {}),
      })),
    }
    try {
      await aoSalvar(suite)
    } catch (err) {
      setErro(err instanceof Error ? err.message : String(err))
      erroRef.current?.scrollIntoView({ behavior: 'smooth', block: 'center' })
    }
  }

  return (
    <section className="card card-testes-cadastro">
      <div className="testes-cadastro-titulo">
        <h2>{dados.idOriginal !== undefined ? 'Editar suíte' : 'Nova suíte'}</h2>
        <span className="subtitulo">
          grava em{' '}
          <code className="caminho-mono">
            /etc/dev-cli/testes/{dados.idOriginal ?? '<id>'}.toml
          </code>
        </span>
      </div>
      <form onSubmit={aoSubmeter}>
        <div className="testes-cadastro-grid">
          <div className="field">
            <label htmlFor="suite-nome">Nome da suíte</label>
            <input
              id="suite-nome"
              className="input"
              type="text"
              placeholder="prezzo · exportar planilha"
              value={dados.nome}
              onChange={(e) => aoMudar({ ...dados, nome: e.target.value })}
            />
          </div>
          <div className="field">
            <label htmlFor="suite-timeout">Timeout por etapa (s)</label>
            <input
              id="suite-timeout"
              className="input"
              type="number"
              min={1}
              value={dados.timeout_etapa_seg}
              onChange={(e) =>
                aoMudar({ ...dados, timeout_etapa_seg: Number(e.target.value) || 1 })
              }
            />
          </div>
        </div>
        <div className="kicker" style={{ marginBottom: 10 }}>Etapas · executam em ordem, param na primeira falha</div>
        <div className="testes-cadastro-passos">
          {dados.passos.map((p, i) => (
            <div className="testes-cadastro-linha" key={i}>
              <span className="testes-cadastro-num">{i + 1}.</span>
              <div className="field" style={{ margin: 0 }}>
                <label htmlFor={`suite-tipo-${i}`}>Tipo</label>
                <select
                  id={`suite-tipo-${i}`}
                  className="input"
                  value={p.tipo}
                  onChange={(e) => aoMudarPasso(i, { tipo: e.target.value as 'exec' | 'valida' })}
                  style={{ padding: '6px 8px', fontSize: 12.5 }}
                >
                  <option value="exec">execução</option>
                  <option value="valida">validação</option>
                </select>
              </div>
              <div className="field" style={{ margin: 0 }}>
                <label htmlFor={`suite-cmd-${i}`}>Comando CLI</label>
                <input
                  id={`suite-cmd-${i}`}
                  className="input"
                  type="text"
                  placeholder="echo oi > oi.txt"
                  value={p.cmd}
                  onChange={(e) => aoMudarPasso(i, { cmd: e.target.value })}
                  style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}
                />
              </div>
              <div className="field" style={{ margin: 0, opacity: p.tipo === 'exec' ? 0.45 : 1 }}>
                <label htmlFor={`suite-esperado-${i}`}>Saída esperada (stdout)</label>
                <input
                  id={`suite-esperado-${i}`}
                  className="input"
                  type="text"
                  placeholder="existe"
                  maxLength={LIMITE_ESPERADO}
                  value={p.esperado ?? ''}
                  disabled={p.tipo === 'exec'}
                  onChange={(e) => aoMudarPasso(i, { esperado: e.target.value })}
                  style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}
                />
              </div>
              <button
                type="button"
                className="btn btn-ghost"
                onClick={() => aoRemoverPasso(i)}
                disabled={dados.passos.length <= 1}
                style={{ padding: '4px 10px', fontSize: 12, marginTop: 22 }}
              >
                Remover
              </button>
            </div>
          ))}
        </div>
        <button
          type="button"
          className="btn btn-ghost"
          onClick={aoAdicionarPasso}
          style={{ padding: '4px 14px', fontSize: 12.5, marginBottom: 20 }}
        >
          + Adicionar etapa
        </button>
        <p className="testes-cadastro-nota">
          Etapas de <em>execução</em> falham por exit code ≠ 0. Etapas de <em>validação</em> falham também
          quando o comando roda sem erro mas o stdout não bate com a saída esperada.
        </p>
        {erro !== null && (
          <div ref={erroRef} className="testes-cadastro-erro" role="alert">{erro}</div>
        )}
        <div className="testes-cadastro-acoes">
          <button type="submit" className="btn btn-primary">
            {dados.idOriginal !== undefined ? 'Salvar alterações' : 'Salvar suíte'}
          </button>
          <button type="button" className="btn btn-ghost" onClick={aoCancelar}>Cancelar</button>
        </div>
      </form>
    </section>
  )
}
