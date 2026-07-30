// Tipos espelhando o JSON da API do dev-server. Os nomes dos campos são
// EXATAMENTE as chaves do JSON (vêm dos structs Rust com Serialize) —
// não renomear.
// docs: https://www.typescriptlang.org/docs/handbook/2/objects.html

/// A severidade calculada pelo servidor (o portal não reclassifica).
export type Severidade = 'Verde' | 'Amarelo' | 'Vermelho' | 'Parado'

/// Um container em /api/containers — resumo da janela + severidade.
export interface ContainerResumo {
  nome: string
  status: string
  uptime: string
  erros: number
  crits: number
  c5xx: number
  c4xx: number
  reqs: number
  /** p95 do tempo de resposta em segundos; null sem requests na janela. */
  p95_seg: number | null
  max_seg: number | null
  total_linhas: number
  /** Timestamp Unix (segundos) da última coleta deste container. */
  ultima_coleta: number
  severidade: Severidade
}

/// Uma linha de log em /api/containers/{nome}/linhas.
export interface LinhaLog {
  nivel: string
  linha: string
  collected_at: number
}

/// Um alerta (container parou/reiniciou) em /api/alertas.
export interface Alerta {
  container: string
  tipo: string
  mensagem: string
  criado_em: number
}

/// Uma linha de erro/crítico no feed global (`/api/erros`).
/// Inclui `id` (cursor) e `container` porque a consulta cruza containers.
export interface ErroLog {
  id: number
  container: string
  nivel: string
  linha: string
  collected_at: number
}

/// Uma célula do histórico em /api/containers/historico: a contagem de
/// erros+críticos naquela hora. `hora` é o timestamp Unix (segundos) do
/// INÍCIO da hora (múltiplo de 3600); o portal usa para tooltip.
export interface CelulaHistorico {
  hora: number
  quantidade: number
  /** Intensidade 0..=5 já calculada no servidor (escala log, relativa ao
   *  maior `quantidade` da resposta inteira) — mesma fonte de verdade do
   *  heatmap de custos de IA (`nucleo::metricas::intensidade_log`). O
   *  portal só pinta com `intensidadeParaCor`, não recalcula. */
  intensidade: number
}

/// Strip de histórico de um container: nome + 24 células (mais recente
/// primeiro) + total. Vem em /api/containers/historico.
export interface HistoricoContainer {
  nome: string
  horas: CelulaHistorico[]
  total: number
}

/// Resposta de /api/ia/custos: pacote completo para a tela IA · custos.
/// `disponivel: false` significa que o banco do OpenCode não foi lido
/// (arquivo ausente / corrompido); o portal mostra estado "sem dados".
export interface CustosIa {
  mes: string
  disponivel: boolean
  tokens: i64Like
  custo_usd: number
  cache_pct: number
  streak_dias: number
  melhor_streak_dias: number
  heatmap: CelulaHeatmap[]
  /** Dia da semana do dia 1º do mês (0 = segunda ... 6 = domingo) — offset
   *  de células transparentes que o heatmap desenha ANTES do dia 1 para
   *  alinhar com os rótulos seg/qua/sex/dom. */
  offset_semana_dia1: number
  modelos: ModeloCusto[]
  /** True se havia sessão do Claude Code no mês (fonte INDEPENDENTE do
   *  OpenCode — transcritos JSONL locais, não o SQLite do OpenCode). */
  claude_disponivel: boolean
  claude_horas_mes: number
  claude_media_horas_dia_ativo: number
  claude_horas_por_semana: SemanaHoras[]
}

/// Uma semana no gráfico "Horas por semana" (`claude_horas_por_semana`):
/// rótulo (segunda-feira da semana, "dd/mm") + total de horas.
export interface SemanaHoras {
  rotulo: string
  horas: number
}

/// Alias para `number | bigint` (alguns campos vindos do JSON podem ser
/// inteiros grandes que excedem `Number.MAX_SAFE_INTEGER`).
type i64Like = number

/// Uma célula do heatmap mensal (1..=31). `intensidade` é 0..=5, na
/// escala do Classical (0 = sem atividade, 5 = máximo do mês).
export interface CelulaHeatmap {
  dia: number
  intensidade: number
}

/// Fonte dos dados da tela IA · custos — espelha o query param `fonte`
/// de /api/ia/custos ('ambos' é o default do servidor E da UI).
export type FonteIa = 'opencode' | 'claude' | 'ambos'

/// Modelo no ranking mensal (por tokens DESC).
export interface ModeloCusto {
  modelo: string
  provedor: string
  sessoes: number
  tokens: number
  custo_usd: number
  tokens_cache: number
  tokens_sem_cache: number
}

/// Câmbio USD → BRL exposto em /api/ia/cambio.
export interface Cambio {
  usd_brl: number
}

/// Resposta de GET /api/config: a `Config` EFETIVA do dev-server (depois de
/// flags > env > arquivo > defaults) — espelha `nucleo::config::Config` 1:1,
/// os três `#[serde(default)]` do Rust viram três sub-objetos aqui. Tela
/// Configuração é somente-leitura: não existe endpoint de escrita.
export interface ConfigEfetiva {
  coleta: ConfigColeta
  limiares: ConfigLimiares
  servidor: ConfigServidor
}

// ─── Testes · e2e ─────────────────────────────────────────────────────
// Espelha `nucleo::testes` (Rust): mesmas chaves JSON, mesma semântica.
// Ver `docs/design_handoff_portal_dev_cli/README.md` (seção 5).

/** Tipo de uma etapa da pipeline. */
export type TipoPasso = 'exec' | 'valida'

/** Uma etapa da pipeline (um comando + tipo + saída esperada p/ validação). */
export interface Passo {
  tipo: TipoPasso
  cmd: string
  /** Só faz sentido para `valida`: o stdout (trimmed) tem que bater com isso. */
  esperado?: string
}

/** Uma suíte de testes — o mesmo arquivo `<id>.toml` em /etc/dev-cli/testes. */
export interface Suite {
  id: string
  nome: string
  timeout_etapa_seg: number
  passos: Passo[]
}

/** Estado de uma etapa dentro de uma execução. O discriminante é `status`. */
export type EstadoPasso =
  | { status: 'pendente' }
  | { status: 'rodando' }
  | { status: 'ok'; saida?: string }
  | { status: 'falha_exec'; exit_code: number; stderr: string }
  | { status: 'falha_valida'; esperado: string; obtido: string }
  | { status: 'pulado' }

/** Conclusão da pipeline como um todo. */
export type ConclusaoExecucao = 'rodando' | 'sucesso' | 'falha'

/** Uma execução — uma "rodada" de uma suíte. Tem um ID único (criado pelo
 *  servidor) e o estado atual de cada passo, atualizado pelo runner. */
export interface Execucao {
  id_execucao: string
  suite_id: string
  iniciada_em_unix: number
  estados: EstadoPasso[]
  conclusao: ConclusaoExecucao
}

/** Resposta de `POST /api/testes/suites/{id}/rodar` — o cliente usa
 *  `id_execucao` para fazer polling em `/api/testes/execucoes/{id}`. */
export interface InicioExecucao {
  id_execucao: string
}

export interface ConfigColeta {
  intervalo_seg: number
  janela_min: number
  retencao_horas: number
  tail_inicial: number
  /** Caminho do SQLite; vazio = default do sistema. */
  db: string
  /** Host SSH ("user@host"); vazio = docker local. */
  ssh: string
}

export interface ConfigLimiares {
  p95_lento_seg: number
  taxa_erro_pct: number
}

export interface ConfigServidor {
  /** "host:porta", ex. "127.0.0.1:8787". */
  bind: string
  portal_dir: string
}
