// Núcleo puro compartilhado pelos dois provedores: heatmap, tabelas,
// barras coloridas, streaks. Nenhuma função aqui toca banco, disco ou rede
// — só transforma dados já carregados em texto pronto pra imprimir. Mesmo
// espírito de `contar()` em `src/logs.rs`.

// `BTreeMap`/`BTreeSet` (em vez de `HashMap`/`HashSet`): mantêm as chaves
// ordenadas automaticamente, o que é essencial aqui — o heatmap, as tabelas
// "por dia"/"por semana" e os streaks dependem de iterar datas em ordem
// crescente sem precisar chamar `.sort()` manualmente.
use std::collections::{BTreeMap, BTreeSet};

// `chrono`: crate de data/hora. `NaiveDate` é uma data sem fuso horário (só
// ano/mês/dia — o suficiente pro heatmap e pras sessões, que já chegam
// agregadas por dia). `DateTime<Utc>` guarda instante com fuso UTC explícito
// (usado em `duracao_sessao`, que precisa de hora/minuto/segundo). `Datelike`
// é a trait que dá os métodos `.weekday()`, `.month()`, `.month0()` a
// qualquer tipo de data do chrono. `Duration` representa um intervalo (ex:
// `Duration::days(1)`) que pode ser somado/subtraído de uma data.
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};
// Trait de extensão do `owo-colors`: importá-la dá a qualquer tipo `Display`
// métodos como `.truecolor(r,g,b)`, `.cyan()`, `.bold()`, que devolvem um
// wrapper que, ao ser formatado, embute os códigos de escape ANSI de cor.
use owo_colors::OwoColorize;

// Teto e piso de duração de uma sessão (ver `duracao_sessao`): sessões que
// ficam abertas a noite toda não devem contar como horas contínuas de
// trabalho, e sessões extremamente curtas ainda recebem um piso mínimo.
pub const TETO_HORAS: f64 = 4.0;
pub const MINIMO_HORAS: f64 = 1.0 / 60.0;

// Formata um número grande de forma compacta (1.5K, 2.3M, 1.2B), para caber
// nas colunas estreitas do dashboard sem estourar o alinhamento. Os `if/else`
// testam da maior escala para a menor, na ordem, e o primeiro que bater
// decide o divisor e o sufixo.
// `valor as i64` trunca a parte fracionária: como só entramos aqui abaixo
// de 1000, não perdemos nada que importe pro relatório.
pub fn numero_compacto(valor: f64) -> String {
    if valor >= 1_000_000_000.0 {
        format!("{:.1}B", valor / 1_000_000_000.0)
    } else if valor >= 1_000_000.0 {
        format!("{:.1}M", valor / 1_000_000.0)
    } else if valor >= 1_000.0 {
        format!("{:.1}K", valor / 1_000.0)
    } else {
        format!("{}", valor as i64)
    }
}

// Converte horas fracionárias (ex: 1.5) para o formato "1h30m" usado em todo
// o dashboard. Trabalha em minutos inteiros para não ter que lidar com
// arredondamento de segundos na formatação final.
pub fn formatar_horas(horas: f64) -> String {
    // `.round()` evita que erros de ponto flutuante (ex: 1.4999999999)
    // arredondem minutos pra baixo por acidente.
    let minutos_totais = (horas * 60.0).round() as i64;
    // `{:02}` no segundo argumento: preenche com zero à esquerda até 2
    // dígitos (ex: "05" em vez de "5"), para os minutos sempre ocuparem duas
    // colunas ("1h05m", não "1h5m").
    format!("{}h{:02}m", minutos_totais / 60, minutos_totais % 60)
}

// Paleta de 6 níveis (0 = sem atividade .. 5 = pico), espelha o `DAY_COLORS`
// do protótipo Python. Separado da cor em si pra ficar testável sem
// depender de código de terminal.
pub fn nivel_intensidade(valor: f64, maximo: f64) -> u8 {
    if maximo <= 0.0 {
        return 0;
    }
    let proporcao = valor / maximo;
    // `+ 0.5` antes de truncar arredonda pro nível mais próximo em vez de
    // sempre truncar pra baixo.
    let indice = (proporcao * 5.0 + 0.5) as i64;
    indice.clamp(0, 5) as u8
}

// `OwoColorize` (trait de extensão) dá o método `.truecolor(r, g, b)` (cor
// RGB arbitrária) e `.cyan()`/`.yellow()`/etc a qualquer `Display`. Cada
// braço do `match` devolve um tipo diferente por baixo do capô, por isso
// convertemos pra `String` já dentro do braço (mesmo truque de
// `colorir_nivel` em `src/logs.rs`).
fn aplicar_cor(nivel: u8, texto: &str) -> String {
    match nivel {
        0 => texto.truecolor(128, 128, 128).to_string(),
        1 => texto.cyan().to_string(),
        2 => texto.green().to_string(),
        3 => texto.yellow().to_string(),
        4 => texto.truecolor(255, 140, 0).to_string(),
        _ => texto.red().to_string(),
    }
}

// Desenha uma barra horizontal de blocos "█" proporcional a `valor/maximo`,
// usada nas seções "Por semana"/"Por dia" do dashboard. `largura_max` é o
// comprimento (em caracteres) que representa 100%. Com `cores = true`, a
// barra inteira ganha uma única cor conforme a intensidade do valor (ver
// `aplicar_cor`/`nivel_intensidade`); sem cor, é só texto puro.
pub fn renderizar_barra(valor: f64, maximo: f64, largura_max: usize, cores: bool) -> String {
    let comprimento = if maximo > 0.0 {
        ((valor / maximo) * largura_max as f64).round() as usize
    } else {
        0
    };
    // `.repeat(n)` cria uma nova `String` com o caractere repetido `n`
    // vezes; `.min(largura_max)` é uma proteção contra arredondamento que
    // ultrapasse o próprio máximo (ex: valor == maximo com erro de ponto
    // flutuante).
    let barra = "█".repeat(comprimento.min(largura_max));
    if cores {
        aplicar_cor(nivel_intensidade(valor, maximo), &barra)
    } else {
        barra
    }
}

// Paleta de 8 cores distintas para as fatias do gráfico de pizza —
// diferente da paleta sequencial do heatmap/barras (que expressa
// intensidade). Aqui as cores só precisam ser fáceis de diferenciar
// umas das outras, já que cada uma representa uma categoria (modelo),
// não um grau de intensidade.
const PALETA_PIZZA: [(u8, u8, u8); 8] = [
    (230, 111, 81),
    (69, 123, 157),
    (233, 196, 106),
    (42, 157, 143),
    (155, 93, 229),
    (244, 162, 97),
    (168, 218, 220),
    (231, 111, 148),
];

// Símbolos usados como fallback sem cor — cada fatia recebe um símbolo
// diferente pra continuar distinguível em `--no-color` (onde a cor não
// está disponível pra separar as fatias visualmente).
const SIMBOLOS_PIZZA: [char; 8] = ['█', '▓', '▒', '░', '◆', '▲', '●', '■'];

/// Desenha um gráfico de pizza em ASCII/ANSI a partir de uma lista de
/// `(rótulo, valor)` já ordenada (a ordem de entrada é a ordem das
/// fatias, no sentido horário a partir do topo). Cada fatia recebe uma
/// cor de `PALETA_PIZZA` e um símbolo de `SIMBOLOS_PIZZA`, ciclando se
/// houver mais de 8 fatias. Valores zerados/negativos não geram fatia;
/// se a soma total for zero, devolve um vetor vazio (nada a desenhar).
///
/// `raio` controla o tamanho em linhas de terminal; a largura em
/// colunas é `4*raio+1` porque um caractere de terminal é ~2x mais alto
/// que largo — esticamos o eixo horizontal para o desenho não sair
/// achatado (ver o `x / 2.0` no teste de distância ao centro).
pub fn renderizar_pizza(fatias: &[(String, f64)], raio: i32, cores: bool) -> Vec<String> {
    let total: f64 = fatias.iter().map(|(_, v)| v.max(0.0)).sum();
    if total <= 0.0 || fatias.is_empty() {
        return Vec::new();
    }

    // Ângulo de início/fim de cada fatia, em radianos — soma cumulativa
    // das proporções ao redor do círculo completo (`TAU` = 2π).
    let mut angulos = Vec::with_capacity(fatias.len());
    let mut acumulado = 0.0;
    for (_, valor) in fatias {
        let inicio = acumulado * std::f64::consts::TAU;
        acumulado += valor.max(0.0) / total;
        angulos.push((inicio, acumulado * std::f64::consts::TAU));
    }
    // A última fatia pode ficar levemente abaixo de TAU por erro de
    // ponto flutuante — força o fim dela até TAU pra não sobrar um
    // fatiazinho sem dono na borda entre a última e a primeira.
    if let Some(ultima) = angulos.last_mut() {
        ultima.1 = std::f64::consts::TAU;
    }

    let mut linhas = Vec::with_capacity((raio * 2 + 1) as usize);
    for dy in -raio..=raio {
        let mut linha = String::new();
        for dx in -(raio * 2)..=(raio * 2) {
            // `dx / 2.0` compensa a razão altura:largura do caractere de
            // terminal (~2:1) — sem isso o círculo sai achatado.
            let x = dx as f64 / 2.0;
            let y = dy as f64;
            if x * x + y * y > (raio * raio) as f64 {
                linha.push(' ');
                continue;
            }
            // Ângulo medido a partir do topo (12h), no sentido horário —
            // por isso é `x.atan2(y_up)` e não `y.atan2(x)` (que mediria
            // a partir das 3h, sentido anti-horário, convenção
            // matemática padrão mas não a de um gráfico de pizza). `-y`
            // inverte o eixo vertical porque linhas de terminal crescem
            // pra baixo.
            let y_up = -y;
            let mut angulo = x.atan2(y_up);
            if angulo < 0.0 {
                angulo += std::f64::consts::TAU;
            }
            let indice = angulos
                .iter()
                .position(|(inicio, fim)| angulo >= *inicio && angulo < *fim)
                .unwrap_or(angulos.len() - 1);
            let simbolo = SIMBOLOS_PIZZA[indice % SIMBOLOS_PIZZA.len()];
            if cores {
                let (r, g, b) = PALETA_PIZZA[indice % PALETA_PIZZA.len()];
                linha.push_str(&simbolo.to_string().truecolor(r, g, b).to_string());
            } else {
                linha.push(simbolo);
            }
        }
        linhas.push(linha);
    }
    linhas
}

// `Copy`: a struct é só dois `u32`, mais barato copiar do que emprestar.
// `Default`: dá `Streaks::default()` com os dois campos zerados — usado
// tanto no teste "sem dias ativos" quanto como valor inicial do cálculo.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Streaks {
    pub atual: u32,
    pub recorde: u32,
}

// Calcula a sequência (streak) de dias consecutivos com atividade: o
// recorde histórico e a sequência atual (contando a partir de "hoje" pra
// trás). Recebe `hoje` como parâmetro (em vez de chamar `Local::now()`
// internamente) para a função ficar pura e testável com qualquer data fixa.
pub fn calcular_streaks(dias_ativos: &BTreeSet<NaiveDate>, hoje: NaiveDate) -> Streaks {
    let mut recorde = 0u32;
    let mut sequencia = 0u32;
    // `Option<NaiveDate>`: `None` antes da primeira iteração (não há "dia
    // anterior" ainda); vira `Some` a partir da primeira volta do loop.
    let mut anterior: Option<NaiveDate> = None;

    // `BTreeSet` já itera em ordem crescente — não precisamos ordenar.
    // `&dia` desestrutura a referência que o `for` dá sobre cada elemento do
    // set (iterar um `BTreeSet<NaiveDate>` empresta `&NaiveDate`; como
    // `NaiveDate` é `Copy`, `&dia` no padrão já copia o valor para `dia`).
    for &dia in dias_ativos {
        // `match` sobre o `Option`: só incrementa a sequência se existir dia
        // anterior E ele for exatamente um dia antes do atual; qualquer
        // outro caso (primeiro dia, ou um "buraco" no meio) reinicia em 1.
        match anterior {
            Some(dia_anterior) if dia == dia_anterior + Duration::days(1) => sequencia += 1,
            _ => sequencia = 1,
        }
        recorde = recorde.max(sequencia);
        anterior = Some(dia);
    }

    // Sequência atual: anda pra trás a partir de "hoje" enquanto o dia
    // estiver no conjunto.
    let mut atual = 0u32;
    let mut cursor = hoje;
    while dias_ativos.contains(&cursor) {
        atual += 1;
        cursor -= Duration::days(1);
    }

    Streaks { atual, recorde }
}

// Calcula os limiares de tokens (percentis 25/50/75) usados para decidir a
// cor de cada célula do heatmap (ver `nivel_atividade`). Devolve um array de
// tamanho fixo `[i64; 3]` (não um `Vec`) porque são sempre exatamente três
// valores — o tipo já documenta isso no retorno.
pub fn limiares_atividade(tokens_por_dia: &BTreeMap<NaiveDate, i64>) -> [i64; 3] {
    // `.values()` itera só os valores do mapa (ignora as chaves/datas);
    // `.copied()` transforma o iterador de `&i64` em `i64` (o tipo é `Copy`,
    // então copiar é barato e evita lidar com referências daqui em diante);
    // `.filter(...)` descarta dias sem atividade real; `.collect()` junta
    // tudo num `Vec` novo, já que precisamos ordenar (`BTreeMap` não permite
    // reordenar só os valores).
    let mut valores: Vec<i64> = tokens_por_dia
        .values()
        .copied()
        .filter(|&tokens| tokens > 0)
        .collect();
    // `sort_unstable`: mais rápido que `sort` porque não preserva a ordem
    // relativa de elementos iguais — irrelevante aqui, já que são só números.
    valores.sort_unstable();

    if valores.is_empty() {
        return [0, 0, 0];
    }

    let n = valores.len();
    // Mesmos percentis (25/50/75) do protótipo Python, aplicados sobre os
    // dias com atividade real. `indice_percentil` é uma closure que captura
    // `valores`/`n` por referência (só lê, não move) e devolve o valor no
    // índice correspondente ao percentil pedido.
    let indice_percentil = |p: f64| valores[(((n - 1) as f64) * p) as usize];
    [
        indice_percentil(0.25),
        indice_percentil(0.50),
        indice_percentil(0.75),
    ]
}

// Classifica um dia em um dos 5 níveis de intensidade (0 = sem dado, 1..4 =
// crescente) usados na cor da célula do heatmap. `tokens` é `Option`
// porque um dia pode simplesmente não ter entrada no mapa (nenhuma
// atividade registrada, diferente de "atividade zero").
pub fn nivel_atividade(tokens: Option<i64>, limiares: &[i64; 3]) -> u8 {
    // `match` com guardas (`if tokens <= ...`): cada braço testa uma faixa,
    // na ordem, até achar a primeira que bate.
    match tokens {
        None => 0,
        Some(tokens) if tokens <= 0 => 1,
        Some(tokens) if tokens <= limiares[0] => 1,
        Some(tokens) if tokens <= limiares[1] => 2,
        Some(tokens) if tokens <= limiares[2] => 3,
        Some(_) => 4,
    }
}

// Meses abreviados em pt-br, indexados por `NaiveDate::month0()` (0 = jan).
const MESES: [&str; 12] = [
    "Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez",
];

fn domingo_da_semana(dia: NaiveDate) -> NaiveDate {
    // `num_days_from_sunday`: domingo=0 .. sábado=6 — exatamente quanto
    // subtrair pra voltar ao domingo daquela semana.
    dia - Duration::days(dia.weekday().num_days_from_sunday() as i64)
}

// Monta a linha de cabeçalho do heatmap com as abreviações de mês (ex:
// "Jun    Jul") alinhadas sobre a coluna da semana em que o mês começa.
// `primeiro_domingo` é o domingo da semana mais à esquerda do heatmap;
// `semanas` é o número de colunas (uma por semana).
fn linha_dos_meses(primeiro_domingo: NaiveDate, semanas: u32) -> String {
    // Um `Vec<char>` de espaços, uma célula por semana — vamos sobrescrever
    // só as posições onde um rótulo de mês começa.
    let mut celulas = vec![' '; semanas as usize];
    // Rastreia o mês da última semana processada; `None` no início força a
    // primeira semana a sempre escrever seu rótulo.
    let mut mes_anterior: Option<u32> = None;

    for semana in 0..semanas {
        let dia = primeiro_domingo + Duration::weeks(semana as i64);
        // Só escreve o rótulo quando o mês muda em relação à semana
        // anterior — senão "Jun" apareceria repetido em toda coluna daquele
        // mês.
        if Some(dia.month()) != mes_anterior {
            let rotulo = MESES[dia.month0() as usize];
            // `.chars().enumerate()`: percorre cada letra do rótulo junto
            // com seu deslocamento (0, 1, 2...), para espalhar "Jun" pelas
            // colunas seguintes à da mudança de mês.
            for (deslocamento, letra) in rotulo.chars().enumerate() {
                let coluna = semana as usize + deslocamento;
                // Protege contra o rótulo estourar a última coluna do
                // heatmap (ex: mês muda na penúltima semana).
                if coluna < semanas as usize {
                    celulas[coluna] = letra;
                }
            }
            mes_anterior = Some(dia.month());
        }
    }

    // `into_iter().collect()`: reconstrói uma `String` a partir do
    // `Vec<char>`, consumindo o vetor (não precisamos mais dele).
    celulas.into_iter().collect()
}

// Devolve o caractere (colorido ou não) que representa um nível de
// atividade (0..4) numa célula do heatmap. Índice no array é o próprio
// `nivel` — por isso os arrays de paleta/símbolos têm exatamente 5 posições.
fn celula_heatmap(nivel: u8, cores: bool) -> String {
    if cores {
        // Paleta verde crescente (5 tons), independente da paleta de barras
        // de `nivel_intensidade` — heatmap e barras representam conceitos
        // diferentes (atividade por dia vs. horas por dia).
        const PALETA: [(u8, u8, u8); 5] = [
            (88, 88, 88),
            (30, 90, 40),
            (40, 120, 60),
            (50, 160, 80),
            (60, 210, 110),
        ];
        let (r, g, b) = PALETA[nivel as usize];
        "■".truecolor(r, g, b).to_string()
    } else {
        ["□", "░", "▒", "▓", "█"][nivel as usize].to_string()
    }
}

// Monta o heatmap completo (estilo GitHub contributions): uma grade de
// `semanas` colunas por 7 linhas (domingo a sábado), mais a linha de meses
// no topo e a legenda "Menos ... Mais" embaixo. Devolve um `Vec<String>`
// (uma entrada por linha de terminal) para quem chama decidir como juntar
// (aqui, `renderizar_dashboard` concatena com `\n`).
pub fn renderizar_heatmap(
    tokens_por_dia: &BTreeMap<NaiveDate, i64>,
    semanas: u32,
    hoje: NaiveDate,
    cores: bool,
) -> Vec<String> {
    let domingo_atual = domingo_da_semana(hoje);
    // A primeira coluna do heatmap é `semanas - 1` semanas antes da atual
    // (ex: com `semanas = 12`, mostramos a semana atual mais as 11
    // anteriores).
    let primeiro_domingo = domingo_atual - Duration::weeks((semanas - 1) as i64);
    let limiares = limiares_atividade(tokens_por_dia);

    // `vec![...]` com um único elemento: a linha de meses já entra como
    // primeira linha do resultado; as linhas de dias da semana são
    // adicionadas (`push`) depois, uma por vez.
    let mut linhas = vec![format!(
        "      {}",
        linha_dos_meses(primeiro_domingo, semanas)
    )];

    // Domingo=0 .. sábado=6, igual ao `weekday()` do chrono com
    // `num_days_from_sunday`. Só rotulamos Seg/Qua/Sex (dias alternados)
    // para não poluir a lateral esquerda do heatmap.
    let rotulos_dias = ["   ", "Seg", "   ", "Qua", "   ", "Sex", "   "];
    for deslocamento_dia in 0..7u32 {
        let mut linha = format!("  {} ", rotulos_dias[deslocamento_dia as usize]);
        for semana in 0..semanas {
            let dia = primeiro_domingo
                + Duration::weeks(semana as i64)
                + Duration::days(deslocamento_dia as i64);
            // Dias no futuro (além de "hoje") não têm como ter atividade —
            // deixamos a célula em branco em vez de mostrar nível 0 (que
            // representaria "sem atividade" num dia que já passou).
            if dia > hoje {
                linha.push(' ');
                continue;
            }
            // `.get(&dia).copied()`: `get` devolve `Option<&i64>`; `.copied()`
            // converte para `Option<i64>` (o valor é `Copy`), que é o que
            // `nivel_atividade` espera — `None` natural quando o dia não
            // está no mapa (sem atividade registrada).
            let nivel = nivel_atividade(tokens_por_dia.get(&dia).copied(), &limiares);
            linha.push_str(&celula_heatmap(nivel, cores));
        }
        linhas.push(linha);
    }

    // Legenda: uma célula de cada nível (0 a 4), do "menos" ao "mais"
    // intenso. `(0..5).map(...)` gera os cinco níveis e `.collect()` junta
    // as células (cada uma já é uma `String`, possivelmente colorida) numa
    // única `String`.
    let legenda: String = (0..5).map(|nivel| celula_heatmap(nivel, cores)).collect();
    linhas.push(format!("      Menos {legenda} Mais"));
    linhas
}

// Sessão de trabalho com data e duração em horas.
// Tipo contrato que `ai/claude.rs` (Task 7) vai produzir — o nome e os campos
// são parte da interface pública entre módulos.
#[derive(Debug, Clone, PartialEq)]
pub struct Sessao {
    pub dia: NaiveDate,
    pub duracao_horas: f64,
}

// Duração de uma sessão a partir dos horários de suas mensagens (já
// ordenados). Sessão de 1 mensagem só vira um valor fixo de 5 minutos —
// não há intervalo pra medir. Com 2+ horários, é a diferença entre o
// primeiro e o último, limitada entre `MINIMO_HORAS` e `TETO_HORAS` (uma
// sessão que ficou aberta a noite toda não deve contar como 8h de
// trabalho contínuo). `?` sobre `.first()`/`.last()` devolve `None` pra
// um slice vazio em vez de exigir `.expect()` numa invariante que quem
// chama já garante (sempre monta o vetor com pelo menos 1 elemento).
pub fn duracao_sessao(horarios: &[DateTime<Utc>]) -> Option<f64> {
    let inicio = *horarios.first()?;
    if horarios.len() < 2 {
        return Some(5.0 / 60.0);
    }
    let fim = *horarios.last()?;
    let horas_brutas = (fim - inicio).num_seconds() as f64 / 3600.0;
    Some(horas_brutas.clamp(MINIMO_HORAS, TETO_HORAS))
}

// Mapa dia -> (soma de horas, quantidade de sessões).
// `entry(...).or_insert((0.0, 0))`: pega a entrada existente ou cria zerada,
// sem precisar de `if contains_key` — evita uma segunda busca na árvore.
pub fn agregar_por_dia(sessoes: &[Sessao]) -> BTreeMap<NaiveDate, (f64, u32)> {
    let mut mapa: BTreeMap<NaiveDate, (f64, u32)> = BTreeMap::new();
    for sessao in sessoes {
        let entrada = mapa.entry(sessao.dia).or_insert((0.0, 0));
        entrada.0 += sessao.duracao_horas;
        entrada.1 += 1;
    }
    mapa
}

// Mapa segunda-feira-da-semana -> (soma de horas, quantidade de sessões,
// conjunto de dias distintos com atividade naquela semana).
// `num_days_from_monday()`: segunda=0 .. domingo=6 — exatamente quanto
// subtrair da data para voltar à segunda daquela semana.
pub fn agregar_por_semana(
    sessoes: &[Sessao],
) -> BTreeMap<NaiveDate, (f64, u32, BTreeSet<NaiveDate>)> {
    let mut mapa: BTreeMap<NaiveDate, (f64, u32, BTreeSet<NaiveDate>)> = BTreeMap::new();
    for sessao in sessoes {
        let segunda =
            sessao.dia - Duration::days(sessao.dia.weekday().num_days_from_monday() as i64);
        let entrada = mapa.entry(segunda).or_insert((0.0, 0, BTreeSet::new()));
        entrada.0 += sessao.duracao_horas;
        entrada.1 += 1;
        entrada.2.insert(sessao.dia);
    }
    mapa
}

/// Agregação de uso por modelo — tipo contrato compartilhado entre os dois
/// provedores (`opencode` e `claude`). Ambos os bancos (SQLite e JSONL)
/// expõem os mesmos conceitos (nome do modelo, provedor, sessões, tokens,
/// custo), então um único struct evita duplicação de tipos e de lógica de
/// renderização. A trait `serde::Serialize` é necessária para o `--json`.
///
/// Tokens e custo já vêm separados pelos quatro tipos de cobrança que a
/// Anthropic distingue: entrada "fresca", cache write, cache read e saída
/// — ver `precos::CustoDetalhado`. Não há outros tipos de cobrança de
/// token; o relatório mostra exatamente essas quatro parcelas.
#[derive(Debug, serde::Serialize)]
pub struct ModeloUso {
    pub modelo: String,
    pub provedor: String,
    pub sessoes: i64,
    pub tokens_entrada: i64,
    pub tokens_cache_escrita: i64,
    pub tokens_cache_leitura: i64,
    pub tokens_saida: i64,
    pub custo_entrada: f64,
    pub custo_cache_escrita: f64,
    pub custo_cache_leitura: f64,
    pub custo_saida: f64,
}

impl ModeloUso {
    // Soma os quatro tipos de token num único total — usado na tabela de
    // modelos e no gráfico de pizza (cada fatia é o total de um modelo).
    pub fn tokens_totais(&self) -> i64 {
        self.tokens_entrada
            + self.tokens_cache_escrita
            + self.tokens_cache_leitura
            + self.tokens_saida
    }

    // Análogo a `tokens_totais`, mas somando as quatro parcelas de custo em
    // dólar.
    pub fn custo_total(&self) -> f64 {
        self.custo_entrada + self.custo_cache_escrita + self.custo_cache_leitura + self.custo_saida
    }
}

/// Pacote de dados já carregados e agregados de um provedor (OpenCode ou
/// Claude), pronto para virar dashboard ou JSON. Existe para que `ai
/// stats` sem subcomando (`stats.rs`) possa carregar os dois provedores e
/// mesclá-los com `mesclar_dados`, sem duplicar a lógica de agregação que
/// já vive em `claude::carregar_dados` e `opencode::carregar_dados`.
#[derive(Debug)]
pub struct DadosProvedor {
    pub sessoes: Vec<Sessao>,
    pub modelos: Vec<ModeloUso>,
    pub tokens_por_dia: BTreeMap<NaiveDate, i64>,
    pub custo_total: f64,
    pub sem_preco: Vec<String>,
}

/// Mescla dois `DadosProvedor` num só: tokens por dia somados por chave
/// (`entry().or_insert(0) +=`, mesmo idiom usado no resto do arquivo),
/// sessões e modelos concatenados (a coluna `provedor` de `ModeloUso` já
/// distingue as linhas na tabela renderizada, então não precisa reagrupar)
/// e custo somado. `sem_preco` passa por um `BTreeSet` só para ordenar e
/// remover duplicatas entre os dois provedores.
pub fn mesclar_dados(mut a: DadosProvedor, b: DadosProvedor) -> DadosProvedor {
    for (dia, tokens) in b.tokens_por_dia {
        *a.tokens_por_dia.entry(dia).or_insert(0) += tokens;
    }
    a.sessoes.extend(b.sessoes);
    a.modelos.extend(b.modelos);
    a.custo_total += b.custo_total;

    let sem_preco: BTreeSet<String> = a.sem_preco.into_iter().chain(b.sem_preco).collect();
    a.sem_preco = sem_preco.into_iter().collect();

    a
}

/// Monta o dashboard unificado — heatmap + horas + modelos + custo — usado
/// tanto pelo `opencode` quanto pelo `claude`. Cada comando só precisa
/// carregar os dados da sua fonte (`carregar_*` específico) e chamar esta
/// função, eliminando a duplicação de rendering que existia antes.
///
/// # Parâmetros
/// - `nome`: rótulo do provedor (ex: "OpenCode atividade").
/// - `mes_ou_resumo`: subtítulo — mês (YYYY-MM) ou resumo geral de tokens.
/// - `tokens_por_dia`: mapa dia → tokens, para o heatmap e streaks.
/// - `sessoes`: vetor de sessões com data e duração, para horas/semanas/dias.
/// - `modelos`: agregação por modelo, para a tabela de modelos.
/// - `custo_usd`: custo total em dólar.
/// - `modelos_sem_preco`: modelos que não têm preço na tabela (só claude).
/// - `semanas_heatmap`: quantas semanas o heatmap mostra (opencode=52, claude=12).
/// - `cores`: se true, usa ANSI truecolor; se false, texto puro.
/// - `top_dias`: se `Some(N)`, inclui ranking dos N dias com mais horas.
///
/// O clippy `too_many_arguments` é suprimido porque cada parâmetro representa
/// uma seção do dashboard; agrupar num struct obscurificaria a interface.
#[allow(clippy::too_many_arguments)]
pub fn renderizar_dashboard(
    nome: &str,
    mes_ou_resumo: &str,
    tokens_por_dia: &BTreeMap<NaiveDate, i64>,
    sessoes: &[Sessao],
    modelos: &[ModeloUso],
    custo_usd: f64,
    modelos_sem_preco: &[String],
    semanas_heatmap: u32,
    cores: bool,
    top_dias: Option<usize>,
) -> String {
    // `Local::now()` pega o instante atual no fuso horário local da
    // máquina; `.date_naive()` descarta a hora, ficando só com a data —
    // é o "hoje" usado tanto no heatmap quanto no cálculo de streak.
    let hoje = Local::now().date_naive();
    // `.keys()` itera só as datas do mapa (ignora os valores de tokens);
    // `.copied()` tira a referência (`NaiveDate` é `Copy`); `.collect()`
    // monta um `BTreeSet` novo — perdemos a contagem de tokens de propósito,
    // aqui só interessa "quais dias tiveram alguma atividade".
    let dias_ativos: BTreeSet<NaiveDate> = tokens_por_dia.keys().copied().collect();
    let streaks = calcular_streaks(&dias_ativos, hoje);

    let por_dia = agregar_por_dia(sessoes);
    let por_semana = agregar_por_semana(sessoes);

    // `.values()` itera as tuplas `(horas, sessões)`/`(horas, sessões, dias)`
    // agregadas; `.map(|(h, _)| h)` extrai só o campo de horas, descartando
    // o resto da tupla; `.sum()` soma tudo num único `f64`.
    let total_horas: f64 = por_dia.values().map(|(h, _)| h).sum();
    // `.fold(0.0, f64::max)`: percorre os valores acumulando o maior já
    // visto, começando de `0.0`. Preferimos `fold` a `.max()` do iterador
    // porque `f64` não implementa `Ord` (por causa de `NaN`), então o
    // `Iterator::max()` padrão não compila para `f64` sem um comparador
    // explícito — `f64::max` já resolve isso tratando `NaN` de forma
    // definida.
    let max_dia = por_dia.values().map(|(h, _)| *h).fold(0.0, f64::max);
    let max_semana = por_semana.values().map(|(h, _, _)| *h).fold(0.0, f64::max);

    // `.ok()`: converte o `Result<f64, _>` da busca de câmbio num
    // `Option<f64>`, descartando o erro específico — aqui só nos importa se
    // deu certo ou não (a seção de custo total decide o que exibir com
    // `match taxa_brl`, mais abaixo).
    let taxa_brl = crate::ai::cambio::buscar_taxa_usd_brl().ok();

    // `.bold()` sempre emite o escape ANSI, mesmo sem cor — por isso o
    // helper checa `cores` antes de aplicar. Usado no cabeçalho e no nome
    // de cada modelo na tabela.
    let negrito = |texto: &str| -> String {
        if cores {
            texto.bold().to_string()
        } else {
            texto.to_string()
        }
    };
    // Rótulo alinhado (padding aplicado ANTES de colorir — se colorirmos
    // primeiro, o `format!` conta os bytes do escape ANSI como parte da
    // largura e o alinhamento quebra). Em cinza quando `cores` está
    // ativo, pra ficar visualmente subordinado ao texto ao redor. Usado
    // no detalhamento do heatmap, nos blocos de modelo e no custo total.
    let rotulo = |texto: &str| -> String {
        let alinhado = format!("{texto:<15}");
        if cores {
            alinhado.truecolor(150, 150, 150).to_string()
        } else {
            alinhado
        }
    };

    // Cabeçalho: nome do provedor em negrito (quando `cores` está ativo).
    let mut saida = format!("\n  {} — {}\n\n", negrito(nome), mes_ou_resumo);

    // ── Heatmap ──────────────────────────────────────────────────────
    // O heatmap mostra a intensidade de uso (em tokens) por dia da
    // semana, com `semanas_heatmap` colunas (cada uma é uma semana).
    // Células mais claras/verdes = mais tokens naquele dia.
    for linha in renderizar_heatmap(tokens_por_dia, semanas_heatmap, hoje, cores) {
        saida.push_str(&linha);
        saida.push('\n');
    }
    // Abaixo do grid: total de dias únicos, tokens totais no período e
    // streaks (sequência de dias consecutivos com tokens). O total é a
    // soma de TODO tipo de token (entrada + cache write + cache read +
    // saída) — a linha seguinte detalha essa soma pelos quatro tipos,
    // usando os mesmos totais agregados por modelo mostrados mais abaixo.
    let total_tokens: i64 = tokens_por_dia.values().sum();
    // Mesmo padrão `iter().map(campo).sum()` repetido quatro vezes: para
    // cada tipo de cobrança, percorre todos os modelos (`.iter()` empresta
    // cada `ModeloUso`, sem consumir o slice) e soma aquele campo isolado.
    let tokens_entrada_total: i64 = modelos.iter().map(|m| m.tokens_entrada).sum();
    let tokens_cache_escrita_total: i64 = modelos.iter().map(|m| m.tokens_cache_escrita).sum();
    let tokens_cache_leitura_total: i64 = modelos.iter().map(|m| m.tokens_cache_leitura).sum();
    let tokens_saida_total: i64 = modelos.iter().map(|m| m.tokens_saida).sum();
    saida.push_str(&format!(
        "  {} dias ativos  |  {} tokens no total (entrada + cache + saída)  |  streak atual: {}  |  recorde: {}\n",
        dias_ativos.len(),
        numero_compacto(total_tokens as f64),
        streaks.atual,
        streaks.recorde
    ));
    // Duas colunas alinhadas (mesmo rótulo com padding fixo usado nos
    // blocos de modelo mais abaixo), em vez de tudo espremido numa
    // linha só separada por "·".
    saida.push_str(&format!(
        "      {} {:>8} tok      {} {:>8} tok\n",
        rotulo("entrada:"),
        numero_compacto(tokens_entrada_total as f64),
        rotulo("cache-escrita:"),
        numero_compacto(tokens_cache_escrita_total as f64)
    ));
    saida.push_str(&format!(
        "      {} {:>8} tok      {} {:>8} tok\n\n",
        rotulo("cache-leitura:"),
        numero_compacto(tokens_cache_leitura_total as f64),
        rotulo("saída:"),
        numero_compacto(tokens_saida_total as f64)
    ));

    // ── Total de horas trabalhadas ───────────────────────────────────
    // Soma de todas as durações de sessão, formatada como XhYm.
    saida.push_str(&format!(
        "  Total: {}  ({} sessões)\n\n",
        formatar_horas(total_horas),
        sessoes.len()
    ));

    // Contagem de tokens por dia/semana, a partir do mesmo `tokens_por_dia`
    // usado no heatmap — assim "por semana", "por dia" e "por modelo"
    // sempre mostram tokens, não só horas/sessões.
    let tokens_do_dia = |dia: NaiveDate| -> i64 { tokens_por_dia.get(&dia).copied().unwrap_or(0) };
    let tokens_da_semana = |segunda: NaiveDate| -> i64 {
        let domingo = segunda + Duration::days(6);
        tokens_por_dia
            .range(segunda..=domingo)
            .map(|(_, t)| *t)
            .sum()
    };

    // ── Por semana ───────────────────────────────────────────────────
    // Cada linha = uma semana (segunda como identificador). A barra
    // colorida escala com `max_semana` para dar contexto visual.
    saida.push_str("  [Por semana]\n");
    for (segunda, (horas, sessoes_semana, dias)) in &por_semana {
        let barra = renderizar_barra(*horas, max_semana, 20, cores);
        saida.push_str(&format!(
            "    semana de {segunda}   {:>3} dias   {:>3} sessões   {:>8}   {:>8} tokens   {barra}\n",
            dias.len(),
            sessoes_semana,
            formatar_horas(*horas),
            numero_compacto(tokens_da_semana(*segunda) as f64)
        ));
    }

    // ── Por dia ──────────────────────────────────────────────────────
    // Cada linha = um dia com sessões. A barra escala com `max_dia`.
    // `largura_max = 25` é maior que a semanal porque a granularidade
    // é mais fina (um dia vs. uma semana).
    saida.push_str("\n  [Por dia]\n");
    for (dia, (horas, sessoes_dia)) in &por_dia {
        let barra = renderizar_barra(*horas, max_dia, 25, cores);
        saida.push_str(&format!(
            "    {dia}   {:>3} sessões   {:>8}   {:>8} tokens   {barra}\n",
            sessoes_dia,
            formatar_horas(*horas),
            numero_compacto(tokens_do_dia(*dia) as f64)
        ));
    }

    // ── Top dias (opcional) ──────────────────────────────────────────
    // Se o provedor pedir (`top_dias = Some(N)`), ordena os dias por
    // horas (decrescente) e mostra os N primeiros. Só o claude usa;
    // opencode passa `None`.
    if let Some(top) = top_dias {
        saida.push_str(&format!("\n  [Top {} dias mais intensos]\n", top));
        let mut dias_ordenados: Vec<_> = por_dia.iter().collect();
        dias_ordenados.sort_by(|a, b| {
            b.1.0
                .partial_cmp(&a.1.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (dia, (horas, sessoes_dia)) in dias_ordenados.into_iter().take(top) {
            saida.push_str(&format!(
                "    {dia}   {}   {} sessões   {} tokens\n",
                formatar_horas(*horas),
                sessoes_dia,
                numero_compacto(tokens_do_dia(*dia) as f64)
            ));
        }
    }

    // ── Modelos usados ───────────────────────────────────────────────
    // Gráfico de pizza mostrando a composição de tokens entre os
    // modelos (fatia maior primeiro), com legenda ao lado — substitui as
    // barras individuais por um único gráfico comparando a fatia de
    // cada modelo no total do período. Depois, um bloco por modelo:
    // cabeçalho com nome/provedor/sessões (em negrito), uma linha por
    // tipo de cobrança (entrada, cache write, cache read, saída) com
    // rótulo, tokens e custo em colunas alinhadas, e a linha de total.
    // Cache write custa mais que entrada fresca (1.25x — o modelo
    // processa e grava); cache read custa bem menos (0.1x — só
    // reaproveita o que já foi processado).
    saida.push_str("\n  [Modelos usados]\n\n");

    // Cadeia `filter` → `map` → `collect`: primeiro descarta modelos sem
    // nenhum token (não fazem fatia), depois transforma cada `ModeloUso`
    // sobrevivente em uma tupla `(nome, total_de_tokens)` — o formato que
    // `renderizar_pizza` espera. `m.modelo.clone()`: `m` é só uma referência
    // emprestada do slice `modelos` (o dashboard não é dono dos dados), então
    // para colocar o nome dentro do novo `Vec<(String, f64)>` (que precisa
    // ser dono do próprio conteúdo) é preciso copiar a `String`, não apenas
    // emprestá-la.
    let mut fatias_pizza: Vec<(String, f64)> = modelos
        .iter()
        .filter(|m| m.tokens_totais() > 0)
        .map(|m| (m.modelo.clone(), m.tokens_totais() as f64))
        .collect();
    // Ordena as fatias da maior pra menor (`b` antes de `a` no
    // `partial_cmp` inverte a ordem natural crescente). `f64` não tem `Ord`
    // total (por causa de `NaN`), só `PartialOrd` — por isso `partial_cmp`
    // devolve `Option<Ordering>`, e o `unwrap_or(Equal)` trata o caso
    // (que não deveria ocorrer aqui, já que tokens nunca são `NaN`) tratando
    // como empate em vez de arriscar um panic.
    fatias_pizza.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    const RAIO_PIZZA: i32 = 6;
    let pizza = renderizar_pizza(&fatias_pizza, RAIO_PIZZA, cores);
    let total_pizza: f64 = fatias_pizza.iter().map(|(_, v)| *v).sum();
    // `.enumerate()` empareia cada item com seu índice (0, 1, 2...) — aqui
    // precisamos do índice para escolher o mesmo símbolo/cor usados na
    // fatia correspondente do desenho (`renderizar_pizza` cicla os mesmos
    // arrays na mesma ordem).
    let legenda_pizza: Vec<String> = fatias_pizza
        .iter()
        .enumerate()
        .map(|(indice, (nome, valor))| {
            let simbolo = SIMBOLOS_PIZZA[indice % SIMBOLOS_PIZZA.len()];
            let marcador = if cores {
                let (r, g, b) = PALETA_PIZZA[indice % PALETA_PIZZA.len()];
                format!("{s}{s}", s = simbolo.to_string().truecolor(r, g, b))
            } else {
                format!("{simbolo}{simbolo}")
            };
            let percentual = if total_pizza > 0.0 {
                valor / total_pizza * 100.0
            } else {
                0.0
            };
            format!(
                "{marcador} {:<28} {:>5.1}%  ({} tok)",
                nome,
                percentual,
                numero_compacto(*valor)
            )
        })
        .collect();

    // Linha em branco (mesma largura visível do desenho) pra alinhar a
    // legenda quando ela tem mais linhas que a pizza, ou vice-versa.
    let linha_vazia_pizza = " ".repeat((RAIO_PIZZA * 4 + 1) as usize);
    // O desenho da pizza (altura fixa `2*raio+1`) e a legenda (uma linha por
    // modelo) quase nunca têm o mesmo número de linhas — usamos o maior dos
    // dois como número de iterações, para nenhuma linha "sobrando" de
    // qualquer um dos lados ficar de fora.
    let linhas_pizza = pizza.len().max(legenda_pizza.len());
    for i in 0..linhas_pizza {
        // `.get(i)` devolve `Option<&String>` (evita panic se um dos lados
        // já acabou); `.map(String::as_str)` converte para `Option<&str>`
        // (a assinatura de `unwrap_or` pede o mesmo tipo dos dois lados);
        // `.unwrap_or(&linha_vazia_pizza)` preenche com espaços em branco do
        // tamanho certo quando a pizza já terminou mas a legenda continua.
        let esquerda = pizza
            .get(i)
            .map(String::as_str)
            .unwrap_or(&linha_vazia_pizza);
        let direita = legenda_pizza.get(i).map(String::as_str).unwrap_or("");
        saida.push_str(&format!("    {esquerda}   {direita}\n"));
    }
    saida.push('\n');

    // Uma linha de detalhe: rótulo alinhado à esquerda, tokens e custo
    // alinhados à direita em colunas fixas — mesma largura pras quatro
    // linhas do bloco, o que faz elas ficarem visualmente em coluna.
    let linha_detalhe = |rotulo_texto: &str, tokens: i64, custo: f64| -> String {
        format!(
            "        {} {:>8} tok   US$ {:>9.4}\n",
            rotulo(rotulo_texto),
            numero_compacto(tokens as f64),
            custo
        )
    };

    for modelo in modelos {
        saida.push_str(&format!(
            "    {}  ({}, {} sessões)\n",
            negrito(&modelo.modelo),
            modelo.provedor,
            modelo.sessoes
        ));
        saida.push_str(&linha_detalhe(
            "entrada:",
            modelo.tokens_entrada,
            modelo.custo_entrada,
        ));
        saida.push_str(&linha_detalhe(
            "cache-escrita:",
            modelo.tokens_cache_escrita,
            modelo.custo_cache_escrita,
        ));
        saida.push_str(&linha_detalhe(
            "cache-leitura:",
            modelo.tokens_cache_leitura,
            modelo.custo_cache_leitura,
        ));
        saida.push_str(&linha_detalhe(
            "saída:",
            modelo.tokens_saida,
            modelo.custo_saida,
        ));
        // Custo médio por sessão desse modelo, entre parênteses — dá uma
        // noção de quanto cada sessão típica custou, sem precisar dividir
        // o total pelas sessões de cabeça.
        let media_sessao = if modelo.sessoes > 0 {
            modelo.custo_total() / modelo.sessoes as f64
        } else {
            0.0
        };
        saida.push_str(&format!(
            "        {} {:>8} tok   US$ {:>9.4} (US$ {:.4})\n\n",
            rotulo("total:"),
            numero_compacto(modelo.tokens_totais() as f64),
            modelo.custo_total(),
            media_sessao
        ));
    }

    // ── Custo total ──────────────────────────────────────────────────
    // Soma cada uma das quatro parcelas (entrada, cache write, cache read,
    // saída) através de todos os modelos, mostrando cada uma em sua
    // própria linha (mesmo layout alinhado dos blocos de modelo acima) e,
    // entre parênteses, o custo médio por sessão daquela parcela — dá uma
    // ideia de quanto cada sessão custa em cada tipo de cobrança. O
    // câmbio BRL é buscado via API (best-effort); se a requisição falhar,
    // mostramos só o valor em dólar com um aviso.
    let custo_entrada_total: f64 = modelos.iter().map(|m| m.custo_entrada).sum();
    let custo_cache_escrita_total: f64 = modelos.iter().map(|m| m.custo_cache_escrita).sum();
    let custo_cache_leitura_total: f64 = modelos.iter().map(|m| m.custo_cache_leitura).sum();
    let custo_saida_total: f64 = modelos.iter().map(|m| m.custo_saida).sum();
    let sessoes_total: i64 = modelos.iter().map(|m| m.sessoes).sum();
    let media_por_sessao = |custo: f64| -> f64 {
        if sessoes_total > 0 {
            custo / sessoes_total as f64
        } else {
            0.0
        }
    };

    // `match` sobre o `Option<f64>` calculado lá em cima: com cotação
    // disponível, mostra os dois valores (USD e BRL já convertido); sem
    // cotação (a chamada de rede falhou), mostra só o dólar com um aviso —
    // o relatório nunca falha por causa da API de câmbio estar fora.
    match taxa_brl {
        Some(taxa) => saida.push_str(&format!(
            "\n  Custo total: US$ {:.2}  (R$ {:.2})\n",
            custo_usd,
            crate::ai::cambio::converter_para_brl(custo_usd, taxa)
        )),
        None => saida.push_str(&format!(
            "\n  Custo total: US$ {:.2}  (cotação indisponível, R$ não calculado)\n",
            custo_usd
        )),
    }
    saida.push_str(&format!(
        "      {} US$ {:>9.4} (US$ {:.4})\n",
        rotulo("entrada:"),
        custo_entrada_total,
        media_por_sessao(custo_entrada_total)
    ));
    saida.push_str(&format!(
        "      {} US$ {:>9.4} (US$ {:.4})\n",
        rotulo("cache-escrita:"),
        custo_cache_escrita_total,
        media_por_sessao(custo_cache_escrita_total)
    ));
    saida.push_str(&format!(
        "      {} US$ {:>9.4} (US$ {:.4})\n",
        rotulo("cache-leitura:"),
        custo_cache_leitura_total,
        media_por_sessao(custo_cache_leitura_total)
    ));
    saida.push_str(&format!(
        "      {} US$ {:>9.4} (US$ {:.4})\n",
        rotulo("saída:"),
        custo_saida_total,
        media_por_sessao(custo_saida_total)
    ));

    // Modelos cujo custo não pôde ser estimado (só relevante pro claude,
    // que depende de uma tabela de preços local; o opencode já armazena
    // o custo no banco).
    if !modelos_sem_preco.is_empty() {
        saida.push_str(&format!(
            "    modelos sem preço na tabela (não estimados): {}\n",
            modelos_sem_preco.join(", ")
        ));
    }

    // `trim_end`: remove as quebras de linha finais acumuladas pelos vários
    // `push_str`/`push('\n')` ao longo da função — quem imprime (`ai/*.rs`)
    // decide o espaçamento final, então devolvemos o texto "limpo" na ponta.
    saida.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numero_compacto_formata_por_ordem_de_grandeza() {
        assert_eq!(numero_compacto(999.0), "999");
        assert_eq!(numero_compacto(1_500.0), "1.5K");
        assert_eq!(numero_compacto(2_300_000.0), "2.3M");
        assert_eq!(numero_compacto(1_200_000_000.0), "1.2B");
    }

    #[test]
    fn formatar_horas_converte_fracao_em_horas_e_minutos() {
        assert_eq!(formatar_horas(1.5), "1h30m");
        assert_eq!(formatar_horas(5.0 / 60.0), "0h05m");
        assert_eq!(formatar_horas(4.0), "4h00m");
    }

    #[test]
    fn renderizar_pizza_sem_fatias_devolve_vazio() {
        assert!(renderizar_pizza(&[], 4, false).is_empty());
    }

    #[test]
    fn renderizar_pizza_com_total_zero_devolve_vazio() {
        let fatias = vec![("a".to_string(), 0.0), ("b".to_string(), 0.0)];
        assert!(renderizar_pizza(&fatias, 4, false).is_empty());
    }

    #[test]
    fn renderizar_pizza_tem_altura_igual_a_duas_vezes_o_raio_mais_um() {
        let fatias = vec![("a".to_string(), 1.0)];
        let linhas = renderizar_pizza(&fatias, 5, false);
        assert_eq!(linhas.len(), 11); // 2*5 + 1

        // Sem cor, cada linha tem exatamente 4*raio+1 caracteres — a
        // largura é esticada horizontalmente pra compensar a proporção
        // do caractere de terminal, então toda linha tem o mesmo
        // comprimento (preenchida com espaço fora do círculo).
        for linha in &linhas {
            assert_eq!(linha.chars().count(), 21); // 4*5 + 1
        }
    }

    #[test]
    fn renderizar_pizza_fatia_unica_usa_um_simbolo_so() {
        let fatias = vec![("único".to_string(), 42.0)];
        let linhas = renderizar_pizza(&fatias, 4, false);
        let simbolos_usados: std::collections::HashSet<char> = linhas
            .iter()
            .flat_map(|l| l.chars())
            .filter(|c| *c != ' ')
            .collect();
        assert_eq!(simbolos_usados, std::collections::HashSet::from(['█']));
    }

    #[test]
    fn renderizar_pizza_duas_fatias_iguais_usa_dois_simbolos() {
        let fatias = vec![("a".to_string(), 1.0), ("b".to_string(), 1.0)];
        let linhas = renderizar_pizza(&fatias, 6, false);
        let simbolos_usados: std::collections::HashSet<char> = linhas
            .iter()
            .flat_map(|l| l.chars())
            .filter(|c| *c != ' ')
            .collect();
        assert_eq!(simbolos_usados, std::collections::HashSet::from(['█', '▓']));
    }

    #[test]
    fn nivel_intensidade_distribui_em_seis_niveis() {
        assert_eq!(nivel_intensidade(0.0, 100.0), 0);
        assert_eq!(nivel_intensidade(20.0, 100.0), 1);
        assert_eq!(nivel_intensidade(50.0, 100.0), 3);
        assert_eq!(nivel_intensidade(80.0, 100.0), 4);
        assert_eq!(nivel_intensidade(100.0, 100.0), 5);
    }

    #[test]
    fn nivel_intensidade_sem_maximo_e_sempre_zero() {
        assert_eq!(nivel_intensidade(10.0, 0.0), 0);
    }

    #[test]
    fn renderizar_barra_sem_cor_devolve_so_os_blocos() {
        assert_eq!(renderizar_barra(50.0, 100.0, 20, false), "██████████");
        assert_eq!(renderizar_barra(0.0, 100.0, 20, false), "");
    }

    #[test]
    fn renderizar_barra_com_cor_envolve_em_escape_ansi() {
        let barra = renderizar_barra(50.0, 100.0, 20, true);
        assert!(
            barra.starts_with("\u{1b}["),
            "esperava escape ANSI, veio: {barra:?}"
        );
        assert!(barra.contains('█'));
    }

    fn data(ano: i32, mes: u32, dia: u32) -> NaiveDate {
        // `expect` aqui é seguro: as datas nos testes são sempre válidas,
        // escritas à mão — não é entrada externa que possa falhar.
        NaiveDate::from_ymd_opt(ano, mes, dia).expect("data de teste válida")
    }

    #[test]
    fn calcular_streaks_conta_sequencia_atual_e_recorde() {
        let dias: BTreeSet<NaiveDate> = [
            data(2026, 6, 28),
            data(2026, 6, 29),
            data(2026, 6, 30),
            data(2026, 7, 2), // quebra a sequência (pulou dia 1º)
        ]
        .into_iter()
        .collect();

        let streaks = calcular_streaks(&dias, data(2026, 7, 2));
        assert_eq!(streaks.recorde, 3); // 28, 29, 30
        assert_eq!(streaks.atual, 1); // só o dia 2 é contíguo até "hoje"
    }

    #[test]
    fn calcular_streaks_sem_dias_ativos_e_zero() {
        let dias: BTreeSet<NaiveDate> = BTreeSet::new();
        let streaks = calcular_streaks(&dias, data(2026, 7, 2));
        assert_eq!(streaks, Streaks::default());
    }

    #[test]
    fn limiares_atividade_ignora_dias_zerados() {
        let tokens: BTreeMap<NaiveDate, i64> = [
            (data(2026, 6, 1), 0),
            (data(2026, 6, 2), 10),
            (data(2026, 6, 3), 20),
            (data(2026, 6, 4), 30),
            (data(2026, 6, 5), 40),
        ]
        .into_iter()
        .collect();

        let limiares = limiares_atividade(&tokens);
        assert_eq!(limiares, [10, 20, 30]);
    }

    #[test]
    fn nivel_atividade_classifica_pelos_limiares() {
        let limiares = [10, 20, 30];
        assert_eq!(nivel_atividade(None, &limiares), 0);
        assert_eq!(nivel_atividade(Some(0), &limiares), 1);
        assert_eq!(nivel_atividade(Some(10), &limiares), 1);
        assert_eq!(nivel_atividade(Some(20), &limiares), 2);
        assert_eq!(nivel_atividade(Some(30), &limiares), 3);
        assert_eq!(nivel_atividade(Some(31), &limiares), 4);
    }

    #[test]
    fn renderizar_heatmap_tem_uma_linha_por_dia_da_semana_mais_cabecalho_e_legenda() {
        let tokens: BTreeMap<NaiveDate, i64> = [(data(2026, 7, 1), 100)].into_iter().collect();
        let linhas = renderizar_heatmap(&tokens, 4, data(2026, 7, 1), false);
        // 1 linha de meses + 7 linhas de dias da semana + 1 linha de legenda.
        assert_eq!(linhas.len(), 9);
        assert!(
            linhas
                .last()
                .expect("legenda sempre é a última linha")
                .contains("Menos")
        );
    }

    #[test]
    fn agregar_por_dia_soma_horas_e_conta_sessoes() {
        let sessoes = vec![
            Sessao {
                dia: data(2026, 6, 1),
                duracao_horas: 1.0,
            },
            Sessao {
                dia: data(2026, 6, 1),
                duracao_horas: 2.5,
            },
            Sessao {
                dia: data(2026, 6, 2),
                duracao_horas: 0.5,
            },
        ];

        let por_dia = agregar_por_dia(&sessoes);
        assert_eq!(por_dia.get(&data(2026, 6, 1)), Some(&(3.5, 2)));
        assert_eq!(por_dia.get(&data(2026, 6, 2)), Some(&(0.5, 1)));
        assert_eq!(por_dia.len(), 2);
    }

    #[test]
    fn agregar_por_semana_agrupa_pela_segunda_feira() {
        // 2026-06-01 é segunda; 2026-06-05 é sexta da mesma semana.
        let sessoes = vec![
            Sessao {
                dia: data(2026, 6, 1),
                duracao_horas: 1.0,
            },
            Sessao {
                dia: data(2026, 6, 5),
                duracao_horas: 2.0,
            },
            Sessao {
                dia: data(2026, 6, 8),
                duracao_horas: 4.0,
            }, // semana seguinte
        ];

        let por_semana = agregar_por_semana(&sessoes);
        assert_eq!(por_semana.len(), 2);

        let semana1 = &por_semana[&data(2026, 6, 1)];
        assert_eq!(semana1.0, 3.0); // horas
        assert_eq!(semana1.1, 2); // sessões
        assert_eq!(semana1.2.len(), 2); // dias distintos

        let semana2 = &por_semana[&data(2026, 6, 8)];
        assert_eq!(semana2.0, 4.0);
    }

    #[test]
    fn duracao_sessao_com_uma_mensagem_e_cinco_minutos_fixos() {
        let horarios = vec!["2026-06-01T10:00:00Z".parse().expect("data válida")];
        assert_eq!(duracao_sessao(&horarios), Some(5.0 / 60.0));
    }

    #[test]
    fn duracao_sessao_curta_e_limitada_ao_minimo() {
        let horarios = vec![
            "2026-06-01T10:00:00Z".parse().expect("data válida"),
            "2026-06-01T10:00:10Z".parse().expect("data válida"), // 10s de intervalo
        ];
        assert_eq!(duracao_sessao(&horarios), Some(MINIMO_HORAS));
    }

    #[test]
    fn duracao_sessao_longa_e_limitada_ao_teto() {
        let horarios = vec![
            "2026-06-01T08:00:00Z".parse().expect("data válida"),
            "2026-06-01T20:00:00Z".parse().expect("data válida"), // 12h de intervalo
        ];
        assert_eq!(duracao_sessao(&horarios), Some(TETO_HORAS));
    }

    #[test]
    fn duracao_sessao_normal_usa_o_intervalo_real() {
        let horarios = vec![
            "2026-06-01T10:00:00Z".parse().expect("data válida"),
            "2026-06-01T11:30:00Z".parse().expect("data válida"), // 1h30 de intervalo
        ];
        assert_eq!(duracao_sessao(&horarios), Some(1.5));
    }

    #[test]
    fn duracao_sessao_sem_horarios_e_none() {
        let horarios: Vec<chrono::DateTime<chrono::Utc>> = vec![];
        assert_eq!(duracao_sessao(&horarios), None);
    }

    #[test]
    fn mesclar_dados_soma_tokens_e_custo_e_concatena_modelos_e_sessoes() {
        let dia1 = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let dia2 = NaiveDate::from_ymd_opt(2026, 7, 2).unwrap();

        let a = DadosProvedor {
            sessoes: vec![Sessao {
                dia: dia1,
                duracao_horas: 1.0,
            }],
            modelos: vec![ModeloUso {
                modelo: "claude-x".to_string(),
                provedor: "anthropic".to_string(),
                sessoes: 1,
                tokens_entrada: 10,
                tokens_cache_escrita: 0,
                tokens_cache_leitura: 0,
                tokens_saida: 5,
                custo_entrada: 0.1,
                custo_cache_escrita: 0.0,
                custo_cache_leitura: 0.0,
                custo_saida: 0.05,
            }],
            tokens_por_dia: BTreeMap::from([(dia1, 100)]),
            custo_total: 0.15,
            sem_preco: vec!["modelo-a".to_string()],
        };
        let b = DadosProvedor {
            sessoes: vec![Sessao {
                dia: dia2,
                duracao_horas: 2.0,
            }],
            modelos: vec![ModeloUso {
                modelo: "grok-y".to_string(),
                provedor: "opencode".to_string(),
                sessoes: 1,
                tokens_entrada: 20,
                tokens_cache_escrita: 0,
                tokens_cache_leitura: 0,
                tokens_saida: 8,
                custo_entrada: 0.2,
                custo_cache_escrita: 0.0,
                custo_cache_leitura: 0.0,
                custo_saida: 0.08,
            }],
            tokens_por_dia: BTreeMap::from([(dia1, 50), (dia2, 200)]),
            custo_total: 0.28,
            sem_preco: vec!["modelo-a".to_string(), "modelo-b".to_string()],
        };

        let mesclado = mesclar_dados(a, b);

        assert_eq!(mesclado.tokens_por_dia.get(&dia1), Some(&150));
        assert_eq!(mesclado.tokens_por_dia.get(&dia2), Some(&200));
        assert_eq!(mesclado.sessoes.len(), 2);
        assert_eq!(mesclado.modelos.len(), 2);
        assert!((mesclado.custo_total - 0.43).abs() < 1e-9);
        assert_eq!(
            mesclado.sem_preco,
            vec!["modelo-a".to_string(), "modelo-b".to_string()]
        );
    }
}
