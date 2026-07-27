# Feed de erros ao vivo no portal — design

Data: 2026-07-27

## Contexto e motivação

O portal web (`web/`) hoje mostra os containers numa tabela (`TabelaContainers`)
ordenada por severidade — boa visão de "qual container está pior", mas o
conteúdo real dos erros só aparece se o usuário clicar num container e abrir
o drill-down (`PainelContainer`), um de cada vez. Não existe hoje nenhuma
visão que junte os erros de **todos** os containers num só lugar: para saber
"o que está quebrando agora" é preciso ir container por container.

O usuário quer que os erros fiquem mais evidentes, independente de qual
container os gerou — um feed único, ao vivo, com as últimas linhas de
ERROR/CRIT de qualquer container, misturadas por ordem de chegada.

Decisões já validadas com o usuário:

- É um **feed único** (lista), não só destaque visual na tabela existente.
- Fica **acima** da tabela de containers — primeira coisa visível ao abrir a
  página, antes até da `ListaAlertas` atual.
- Atualiza via o **mesmo polling de 15s** que já existe em `App.tsx`; sem
  notificação do navegador (decisão de uma sessão anterior) — só atualização
  visual em tela, com destaque temporário nos itens novos.
- **Clicar num item do feed abre o drill-down** do container correspondente
  (o `PainelContainer` já existente), já filtrado no nível daquele erro.
- A tabela de containers e sua ordenação por severidade **continuam como
  estão** — não fazem parte deste trabalho.

## Arquitetura

### Backend: endpoint cursor-based `/api/erros`

Hoje a única forma de buscar linhas de log pela API é
`GET /api/containers/{nome}/linhas`, por container. Para o feed global
precisamos de uma consulta que junte todos os containers e que suporte
"buscar só o que é novo desde a última vez" sem reprocessar tudo a cada
poll.

Cursor por **`id`** de `log_lines` (chave primária autoincrement), não por
`collected_at`: timestamps podem colidir entre linhas gravadas na mesma
coleta (mesmo `collected_at` para várias linhas), então não servem como
cursor de "já vi isso". `id` é estritamente crescente e já existe na tabela.

Nova função em `crates/nucleo/src/db.rs`:

```rust
/// Uma linha de erro/crítico carregada para o feed global — item do
/// endpoint `/api/erros`. Diferente de `LinhaLog`, inclui `id` (o cursor)
/// e `container`, porque a consulta cruza containers.
pub struct ErroLog {
    pub id: i64,
    pub container: String,
    pub nivel: String,
    pub linha: String,
    pub collected_at: i64,
}

/// Erros/críticos com `id > desde_id`, de QUALQUER container, ordenados por
/// `id ASC` (mais antigo primeiro — o chamador decide como exibir).
/// `limite` protege a API de respostas gigantes se o cliente ficar muito
/// tempo sem buscar (aba aberta dias).
pub fn erros_desde(
    conn: &Connection,
    desde_id: i64,
    limite: usize,
) -> Result<Vec<ErroLog>, Box<dyn std::error::Error>>
```

SQL: `SELECT id, container_name, level, line, collected_at FROM log_lines
WHERE id > ?1 AND level IN ('ERROR','ERRO','CRIT','CRITICAL','FATAL')
ORDER BY id ASC LIMIT ?2`.

Nova rota em `crates/servidor/src/api.rs`:

```rust
.route("/api/erros", get(listar_erros))
```

```rust
#[derive(Deserialize)]
struct ParamsErros {
    desde_id: Option<i64>, // ausente/0 = desde o início
    limite: Option<usize>, // default 100
}

async fn listar_erros(
    State(estado): State<EstadoApi>,
    Query(params): Query<ParamsErros>,
) -> Result<Json<Vec<ErroLog>>, (StatusCode, String)>
```

Sem filtro de janela de tempo aqui — o cursor já resolve "o que é novo"; a
carga inicial do frontend (ver abaixo) pede um lote recente por `limite`,
não por `janela_min`.

### Frontend: `FeedErros` + hook de polling incremental

Novo arquivo `web/src/tipos.ts`: struct `ErroLog` espelhando o JSON acima
(mesma convenção do resto do arquivo — mudou a API, muda os dois).

Novo `web/src/api.ts`:

```ts
export function buscarErros(desdeId: number, limite = 100): Promise<ErroLog[]> {
  return buscarJson(`/api/erros?desde_id=${desdeId}&limite=${limite}`)
}
```

Novo componente `web/src/componentes/FeedErros.tsx`, "burro" como
`TabelaContainers` — recebe a lista de erros e um callback de clique via
props, não busca nada sozinho:

```tsx
interface Props {
  erros: ErroLogComDestaque[] // ErroLog + `novo: boolean` calculado no pai
  aoClicar: (container: string, nivel: string) => void
}
```

A busca e o cursor vivem em `App.tsx`, junto do `carregar()` existente —
não um `useEffect` isolado dentro do componente, para ficar no mesmo ciclo
de 15s que já orquestra containers/alertas (evita um segundo timer
independente e possíveis races entre polls):

```ts
const [erros, setErros] = useState<ErroLog[]>([])
const cursorRef = useRef(0) // maior id já visto; useRef porque não deve
                             // disparar re-render sozinho ao mudar

// dentro de carregar():
const novosErros = await buscarErros(cursorRef.current)
if (novosErros.length > 0) {
  cursorRef.current = novosErros[novosErros.length - 1].id
  setErros((atual) => [...novosErros].reverse().concat(atual).slice(0, 50))
}
```

Carga inicial (no primeiro `carregar()`, `cursorRef.current` começa em 0):
busca os últimos ~50 erros existentes para já mostrar contexto ao abrir a
página — diferente da decisão anterior sobre notificação (que não queria
histórico), aqui faz sentido porque é uma lista, não um alerta intrusivo.

Destaque de item novo: `FeedErros` recebe os erros já com a informação de
"é novo" (calculada comparando com o cursor anterior, não com estado interno
do componente) e aplica uma classe CSS que soma no mount de cada `<li>` novo
e é removida via `setTimeout(..., 2000)` num `useEffect` local do item —
isolado num subcomponente `ItemErro` para o timeout não re-renderizar a
lista inteira a cada expiração.

Teto de **50 itens**: lista mais que isso é descartada do estado (mantém só
os mais recentes); o cursor sempre avança, então nada se perde no banco —
só não fica tudo pendurado na memória do navegador numa aba esquecida aberta
por dias.

### Posição na página e integração com o drill-down

`App.tsx` passa a renderizar, nesta ordem:

```tsx
<Cabecalho ... />
<FeedErros erros={...} aoClicar={(container, nivel) => {
  setSelecionado(container)
  setNivelInicial(nivel)
}} />
<ListaAlertas ... />
<TabelaContainers ... />
{selecionado !== null && (
  <PainelContainer nome={selecionado} nivelInicial={nivelInicial} aoFechar={...} />
)}
```

`PainelContainer` ganha uma prop nova opcional `nivelInicial?: string`, usada
só para inicializar o `useState('')` do filtro de nível (`useState(nivelInicial ?? '')`).
Sem essa prop, comportamento idêntico ao de hoje (abre em "todos os
níveis").

### Cada linha do feed

Container (bolinha de cor reaproveitando `COR_SEVERIDADE`/nível → cor via a
mesma tabela de `colorir_nivel` do lado Rust, replicada em CSS), nome do
container, nível, texto da linha truncado (CSS `text-overflow: ellipsis`,
sem quebra — clicar abre o drill-down completo pra ver inteiro), horário
relativo (reaproveita o que `Cabecalho` já usa pra "atualizado há Xs", se
existir um helper, senão novo helper em `formato.ts`).

## Testes

- `crates/nucleo/src/db.rs`: teste unitário de `erros_desde` — filtra por
  nível, respeita `desde_id` (não repete o que já passou do cursor) e
  `limite`, mistura containers diferentes na mesma resposta.
- `crates/servidor/src/api.rs`: teste do endpoint `/api/erros` — JSON tem os
  campos certos, `desde_id` ausente traz tudo, `desde_id` alto traz vazio.
- `web/src/api.test.ts`: `buscarErros` monta a URL certa com os query params.
- `web/src/componentes/FeedErros.test.tsx`: renderiza níveis com a cor
  certa, clique dispara `aoClicar` com container+nível certos, item marcado
  `novo` tem a classe de destaque.
- `web/src/App.test.tsx`: avança o cursor entre polls (mock de duas
  respostas de `buscarErros`, confirma que a segunda chamada usa o
  `desde_id` certo).

## Fora de escopo

- Notificação do navegador (`Notification` API) — decisão explícita de
  sessão anterior: só atualização em tela.
- Filtro de container ou nível no feed (mostra tudo) — pode virar uma
  iteração futura se o volume de erros tornar o feed poluído.
- Mudanças na tabela de containers existente ou sua ordenação — já validada
  como boa pelo usuário, fica como está.
