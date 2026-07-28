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
  HistoricoContainer,
  LinhaLog,
} from './tipos'

/// GET + parse de JSON com erro para status não-2xx.
/// Genérica em T: o chamador diz o tipo esperado do corpo.
/// docs: https://developer.mozilla.org/docs/Web/API/Fetch_API
async function buscarJson<T>(caminho: string): Promise<T> {
  const resposta = await fetch(caminho)
  if (!resposta.ok) {
    throw new Error(`API respondeu ${resposta.status} em ${caminho}`)
  }
  return resposta.json() as Promise<T>
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
