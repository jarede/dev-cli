// Cliente da API do dev-server. Só fetch nativo — sem axios/react-query:
// para este portal, o fetch + polling da App resolve (YAGNI).
// As URLs são RELATIVAS (/api/...): em dev o proxy do Vite repassa ao
// dev-server; em produção portal e API saem da mesma origem (ServeDir).

import type {
  Alerta,
  Cambio,
  ConfigEfetiva,
  ContainerResumo,
  CustosIa,
  ErroLog,
  Execucao,
  HistoricoContainer,
  InicioExecucao,
  LinhaLog,
  Suite,
} from './tipos'

/// GET + parse de JSON com erro para status não-2xx.
/// Genérica em T: o chamador diz o tipo esperado do corpo.
/// Aceita um segundo argumento opcional com opções extras do `fetch`
/// (method, body...) — usado pelos POSTs de criar/rodar/cancelar.
/// Seta `content-type: application/json` SÓ quando há body, e NUNCA
/// passa um segundo argumento vazio pro fetch (pra preservar a
/// assinatura `fetch(caminho)` nos testes do GET puro).
/// docs: https://developer.mozilla.org/docs/Web/API/Fetch_API
async function buscarJson<T>(caminho: string, init?: RequestInit): Promise<T> {
  const options: RequestInit | undefined =
    init === undefined
      ? undefined
      : init.body !== undefined
        ? { ...init, headers: { 'content-type': 'application/json', ...init.headers } }
        : init
  const resposta = options === undefined ? await fetch(caminho) : await fetch(caminho, options)
  if (!resposta.ok) {
    throw new Error(`API respondeu ${resposta.status} em ${caminho}`)
  }
  return resposta.json() as Promise<T>
}

/// Variante para endpoints sem corpo de resposta (ex.: 204 No Content do
/// cancelamento). Mesma lógica de erro de status, mas não tenta parsear
/// JSON.
async function buscarVoid(caminho: string, init?: RequestInit): Promise<void> {
  const resposta =
    init === undefined ? await fetch(caminho) : await fetch(caminho, init)
  if (!resposta.ok) {
    throw new Error(`API respondeu ${resposta.status} em ${caminho}`)
  }
}

/// Containers ranqueados por severidade (a ORDENAÇÃO vem do servidor).
export function buscarContainers(janelaMin?: number): Promise<ContainerResumo[]> {
  const query = janelaMin !== undefined ? `?janela_min=${janelaMin}` : ''
  return buscarJson(`/api/containers${query}`)
}

/// Linhas de log de um container, opcionalmente filtradas por nível.
/// `encodeURIComponent`: nomes de container entram no PATH da URL.
export function buscarLinhas(
  nome: string,
  nivel?: string,
  limite: number = 100,
): Promise<LinhaLog[]> {
  const params = new URLSearchParams({ limite: String(limite) })
  if (nivel) params.set('nivel', nivel)
  return buscarJson(`/api/containers/${encodeURIComponent(nome)}/linhas?${params}`)
}

/// Strip de histórico: contagem de erros+críticos por hora nas últimas
/// `horas` (default 24) para todos os containers. `horas` é HORAS, não
/// minutos como `janela_min` — alinha com a UI (1 strip = N horas).
export function buscarHistorico(horas: number = 24): Promise<HistoricoContainer[]> {
  return buscarJson(`/api/containers/historico?horas=${horas}`)
}

/// Alertas recentes (containers que pararam/reiniciaram).
export function buscarAlertas(limite: number = 100): Promise<Alerta[]> {
  return buscarJson(`/api/alertas?limite=${limite}`)
}

/// Erros/críticos globais com `id > desdeId` (cursor incremental do feed).
export function buscarErros(desdeId: number, limite = 100): Promise<ErroLog[]> {
  return buscarJson(`/api/erros?desde_id=${desdeId}&limite=${limite}`)
}

/// Pacote completo da tela IA · custos: tokens/custo no mês, heatmap,
/// streak e ranking de modelos. `mes` = "YYYY-MM"; default = mês atual
/// (resolvido no servidor, via Local::now()).
export function buscarCustosIa(mes?: string): Promise<CustosIa> {
  const query = mes !== undefined ? `?mes=${encodeURIComponent(mes)}` : ''
  return buscarJson(`/api/ia/custos${query}`)
}

/// Câmbio USD → BRL usado pela tela IA · custos — o servidor busca ao vivo
/// (mesma fonte do `dev-cli ai stats`) e cai num fallback se a rede falhar.
export function buscarCambio(): Promise<Cambio> {
  return buscarJson('/api/ia/cambio')
}

/// Config efetiva do dev-server (tela Configuração, somente-leitura).
export function buscarConfig(): Promise<ConfigEfetiva> {
  return buscarJson('/api/config')
}

// ─── Testes · e2e ─────────────────────────────────────────────────────
//
// Estas rotas ESCREVEM no disco e EXECUTAM comandos arbitrários no
// dev-server — por isso `main.rs` não libera CORS permissivo nelas, e
// exige este cabeçalho custom em toda chamada (`CABECALHO_ANTICSRF` em
// `testes_api.rs`, mesmo valor aqui). Sem ele, o middleware do servidor
// responde 403 antes de chegar em qualquer handler. Ver o comentário da
// constante no Rust para o raciocínio completo (CSRF via "requisição
// simples" não é bloqueado só por CORS).
const CABECALHO_ANTICSRF = 'x-dev-cli-portal'

/// Lista todas as suítes (de `/etc/dev-cli/testes/*.toml`).
export function buscarSuites(): Promise<Suite[]> {
  return buscarJson('/api/testes/suites', { headers: { [CABECALHO_ANTICSRF]: '1' } })
}

/// Cria (ou sobrescreve) uma suíte — POST com o `Suite` em JSON. O
/// servidor revalida o `id` e escreve o TOML no disco.
export function criarSuite(suite: Suite): Promise<Suite> {
  return buscarJson('/api/testes/suites', {
    method: 'POST',
    headers: { [CABECALHO_ANTICSRF]: '1' },
    body: JSON.stringify(suite),
  })
}

/// Inicia a execução de uma suíte — recebe o `id_execucao` para
/// polling. O runner roda em background no servidor.
export function rodarSuite(id: string): Promise<InicioExecucao> {
  return buscarJson(`/api/testes/suites/${encodeURIComponent(id)}/rodar`, {
    method: 'POST',
    headers: { [CABECALHO_ANTICSRF]: '1' },
  })
}

/// Snapshot de uma execução em andamento ou terminada. O portal polla
/// esta rota a cada 1s enquanto `conclusao === 'rodando'`.
export function buscarExecucao(idExecucao: string): Promise<Execucao> {
  return buscarJson(`/api/testes/execucoes/${encodeURIComponent(idExecucao)}`, {
    headers: { [CABECALHO_ANTICSRF]: '1' },
  })
}

/// Histórico de execuções de uma suíte, mais recente primeiro (default:
/// últimas 20). Usado pela seção "Histórico · últimas execuções" no card
/// expandido de cada suíte.
export function buscarHistoricoExecucoes(idSuite: string, limite?: number): Promise<Execucao[]> {
  const query = limite !== undefined ? `?limite=${limite}` : ''
  return buscarJson(`/api/testes/suites/${encodeURIComponent(idSuite)}/execucoes${query}`, {
    headers: { [CABECALHO_ANTICSRF]: '1' },
  })
}

/// Pede o cancelamento de uma execução em andamento. O runner checa o
/// flag entre etapas e marca o resto como `pulado`. Retorna `void` (o
/// endpoint responde 204 No Content).
export function cancelarExecucao(idExecucao: string): Promise<void> {
  return buscarVoid(`/api/testes/execucoes/${encodeURIComponent(idExecucao)}/cancelar`, {
    method: 'POST',
    headers: { [CABECALHO_ANTICSRF]: '1' },
  })
}
