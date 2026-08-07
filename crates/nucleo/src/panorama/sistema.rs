// Coletor de saúde do host que roda os containers — memória, disco, carga e
// uptime. Serve de base para o cálculo de overcommit: a soma dos limites de
// memória dos containers contra a memória física do host.
//
// Duas metades com responsabilidades separadas (regra do workspace):
//   - `parsear` — NÚCLEO PURO: saída completa (`&str`) -> `ResultadoColeta<InfoHost>`.
//     Sem I/O, 100% testável com strings inline (vale como fixture).
//   - `coletar` — CASCA DE IO: dispara UMA ÚNICA invocação remota encadeando
//     todos os comandos com `;` — não pagar o custo de conexão 5 vezes — e
//     entrega a saída bruta ao parser.
//
// Os dados vêm de `/proc` (sistema virtual do kernel), `uname` e `df`; o
// shell encadeado funciona igual no modo Local e no modo SSH do `Executor`.

use crate::executor::Executor;
use crate::panorama::ResultadoColeta;
use crate::panorama::snapshot::{ErroColeta, InfoHost};

/// Marcador que separa as seções na saída do comando encadeado. O
/// `echo '---'` gera exatamente essa linha; o parser divide a saída por ela.
pub const MARCADOR: &str = "---";

/// Tudo em UMA linha: `uname -sr`, `/proc/uptime`, `/proc/meminfo`,
/// `/proc/loadavg` e `df -B1 /`, encadeados por `;` e separados por
/// `echo '---'` (o marcador). Um único `executar` = uma única conexão.
pub const COMANDO_SISTEMA: &str = "uname -sr; echo '---'; cat /proc/uptime; echo '---'; cat /proc/meminfo; echo '---'; cat /proc/loadavg; echo '---'; df -B1 /";

/// Rótulo de proveniência dos `ErroColeta` criados por este coletor.
fn rotulo_erro(nome_host: &str) -> String {
    format!("sistema:{nome_host}")
}

/// CASCA DE IO: dispara UMA invocação remota e entrega a saída ao parser.
///
/// O executor devolve `Err` quando o host está inacessível — isso vira
/// `ResultadoColeta::falha` (sem `dados`), em vez de um panic.
pub fn coletar(executor: &Executor, nome_host: &str) -> ResultadoColeta<InfoHost> {
    match executor.executar(&[COMANDO_SISTEMA]) {
        Ok(saida) => parsear(&saida, nome_host),
        Err(erro) => {
            ResultadoColeta::falha(format!("host inacessível: {erro}"), rotulo_erro(nome_host))
        }
    }
}

/// PURA: lê a saída bruta do comando e devolve `InfoHost`, tolerante a falha
/// parcial — cada seção que falha gera um `ErroColeta`; a coleta inteira vira
/// `falha` apenas quando nenhuma seção foi reconhecida.
pub fn parsear(saida: &str, nome_host: &str) -> ResultadoColeta<InfoHost> {
    // `split(MARCADOR)` gera as seções EM ORDEM (o comando encadeado obedece a
    // ordem: uname, uptime, meminfo, loadavg, disco); `trim` remove as quebras
    // de linha nas pontas de cada seção.
    // docs: https://doc.rust-lang.org/std/primitive.str.html#method.split
    let secoes: Vec<&str> = saida.split(MARCADOR).map(str::trim).collect();

    // Começamos no `Default` do contrato; cada seção reconhecida preenche seus
    // campos. `nome` vem da config, NÃO o hostname real.
    let mut info = InfoHost {
        nome: nome_host.to_string(),
        ..InfoHost::default()
    };
    let mut erros = Vec::new();
    let mut sucessos = 0usize;

    match parsear_sistema(secao(&secoes, 0)) {
        Ok((sistema, kernel)) => {
            info.sistema = sistema;
            info.kernel = kernel;
            sucessos += 1;
        }
        Err(motivo) => erros.push(erro_da_secao(nome_host, "sistema", &motivo)),
    }
    match parsear_uptime(secao(&secoes, 1)) {
        Ok(uptime) => {
            info.uptime_segundos = uptime;
            sucessos += 1;
        }
        Err(motivo) => erros.push(erro_da_secao(nome_host, "uptime", &motivo)),
    }
    match parsear_memoria(secao(&secoes, 2)) {
        Ok((total, usada)) => {
            info.memoria_total_bytes = total;
            info.memoria_usada_bytes = usada;
            sucessos += 1;
        }
        Err(motivo) => erros.push(erro_da_secao(nome_host, "meminfo", &motivo)),
    }
    match parsear_carga(secao(&secoes, 3)) {
        Ok((c_1m, c_5m, c_15m)) => {
            info.carga_1m = c_1m;
            info.carga_5m = c_5m;
            info.carga_15m = c_15m;
            sucessos += 1;
        }
        Err(motivo) => erros.push(erro_da_secao(nome_host, "loadavg", &motivo)),
    }
    match parsear_disco(secao(&secoes, 4)) {
        Ok((total, usado)) => {
            info.disco_total_bytes = total;
            info.disco_usado_bytes = usado;
            sucessos += 1;
        }
        Err(motivo) => erros.push(erro_da_secao(nome_host, "disco", &motivo)),
    }

    if sucessos == 0 {
        // Nada foi aproveitável: "não sei" é diferente de "zero", e o
        // consumidor precisa exibir ausência de dados (ver `ResultadoColeta`).
        return ResultadoColeta::falha(
            "saída sem nenhuma seção reconhecível",
            rotulo_erro(nome_host),
        );
    }
    ResultadoColeta::parcial(info, erros)
}

/// Recupera a seção `indice` da saída, ou `""` quando não existir — saída
/// truncada nunca é um crash, vira `ErroColeta` na seção correspondente.
fn secao<'a>(secoes: &[&'a str], indice: usize) -> &'a str {
    secoes.get(indice).copied().unwrap_or("")
}

/// Monta um `ErroColeta` apontando para o rótulo do coletor + qual seção.
fn erro_da_secao(nome_host: &str, secao: &str, motivo: &str) -> ErroColeta {
    ErroColeta {
        coletor: rotulo_erro(nome_host),
        mensagem: format!("{secao}: {motivo}"),
    }
}

/// `uname -sr` → ("Linux", "6.12.7"). A primeira linha da seção é tudo o que
/// importa — `uname` escreve uma linha só.
fn parsear_sistema(saida: &str) -> Result<(String, String), String> {
    let primeira_linha = saida.lines().next().ok_or_else(|| "vazio".to_string())?;
    // `splitn(2, ...)`: no máximo dois pedaços — sistema E KERNEL num só, para
    // um kernel como "6.8.2-1-generic" não quebrar em vários tokens.
    // docs: https://doc.rust-lang.org/std/primitive.str.html#method.splitn
    let mut campos = primeira_linha.splitn(2, char::is_whitespace);
    // let-chain: só seguimos com ambos os campos presentes.
    if let (Some(sistema), Some(kernel)) = (campos.next(), campos.next().map(str::trim)) {
        Ok((sistema.to_string(), kernel.to_string()))
    } else {
        Err("formato inesperado de uname".to_string())
    }
}

/// `uptime → "3800.123 2400.66"`: pega o primeiro campo e só a parte inteira.
fn parsear_uptime(saida: &str) -> Result<u64, String> {
    let primeiro = saida
        .split_whitespace()
        .next()
        .ok_or_else(|| "vazio".to_string())?;
    // "3800.123" → separando no "." fica só o inteiro que o contrato pede.
    let inteiro = primeiro
        .split('.')
        .next()
        .ok_or_else(|| "sem parte inteira".to_string())?;
    inteiro
        .parse()
        .map_err(|_| format!("não é número: {primeiro}"))
}

/// Extrai "16000000" de "MemTotal:       16000000 kB" — o valor é o primeiro
/// campo da linha; `split_whitespace` colapsa os espaços múltiplos.
fn valor_em_kb(linha: &str) -> Option<u64> {
    linha.split_whitespace().next()?.parse().ok()
}

/// `/proc/meminfo` está em kB → (total em bytes, usada em bytes).
fn parsear_memoria(saida: &str) -> Result<(u64, u64), String> {
    let mut total_kb = None;
    let mut disponivel_kb = None;
    for linha in saida.lines() {
        // let-chain: `strip_prefix` devolve `&str` apenas se o prefixo casa —
        // junta "linha é um MemTotal?" com "qual o valor?" num `if` só.
        // docs: https://doc.rust-lang.org/edition-guide/rust-2024/let-chains.html
        if let Some(resto) = linha.trim().strip_prefix("MemTotal:") {
            total_kb = valor_em_kb(resto);
        } else if let Some(resto) = linha.trim().strip_prefix("MemAvailable:") {
            disponivel_kb = valor_em_kb(resto);
        }
    }
    let total_kb = total_kb.ok_or_else(|| "sem MemTotal".to_string())?;
    let disponivel_kb = disponivel_kb.ok_or_else(|| "sem MemAvailable".to_string())?;

    // IMPORTANTE: `MemAvailable`, NÃO `MemFree`. Cache e buffers CONTAM como
    // disponíveis; MemFree mostraria um servidor saudável à beira do OOM.
    // Quem trocar para MemFree aqui, os testes de memória deixam de bater.
    let usada_kb = total_kb.saturating_sub(disponivel_kb);
    // /proc/meminfo reporta kB; bytes = kB × 1024.
    Ok((total_kb * 1024, usada_kb * 1024))
}

/// `/proc/loadavg` → as três médias de carga (1, 5 e 15 minutos).
fn parsear_carga(saida: &str) -> Result<(f64, f64, f64), String> {
    let mut campos = saida.split_whitespace();
    let c_1m: f64 = campos
        .next()
        .ok_or_else(|| "vazio".to_string())?
        .parse()
        .map_err(|_| "média de carga inválida".to_string())?;
    let c_5m: f64 = campos
        .next()
        .ok_or_else(|| "vazio".to_string())?
        .parse()
        .map_err(|_| "média de carga inválida".to_string())?;
    let c_15m: f64 = campos
        .next()
        .ok_or_else(|| "vazio".to_string())?
        .parse()
        .map_err(|_| "média de carga inválida".to_string())?;
    Ok((c_1m, c_5m, c_15m))
}

/// `df -B1 /` já vem em bytes; pula o cabeçalho e lê total + usado.
fn parsear_disco(saida: &str) -> Result<(u64, u64), String> {
    // A primeira linha é o cabeçalho ("Filesystem 1B-blocks ..."). Buscamos a
    // primeira linha que NÃO seja cabeçalho nem vazia.
    let linha_dados = saida
        .lines()
        .map(str::trim)
        .find(|linha| !linha.is_empty() && !linha.starts_with("Filesystem"))
        .ok_or_else(|| "sem linha de dados".to_string())?;
    let mut campos = linha_dados.split_whitespace();
    // Coluna 0 = dispositivo ("/dev/sda1"); 1 = total; 2 = usado.
    let total: u64 = campos
        .nth(1)
        .ok_or_else(|| "sem coluna de total".to_string())?
        .parse()
        .map_err(|_| "tamanho inválido".to_string())?;
    let usado: u64 = campos
        .next()
        .ok_or_else(|| "sem coluna de usado".to_string())?
        .parse()
        .map_err(|_| "tamanho inválido".to_string())?;
    Ok((total, usado))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture completa e inline — o que `coletar` receberia de um host real
    /// depois da única invocação remota. MemFree é pequeno e MemAvailable é
    /// grande, de propósito: ver `memoria_usada_usar_mem_available`.
    const FIXTURE_COMPLETA: &str = "\
Linux 6.8.2-1-generic
---
3800.123 2400.66
---
MemTotal:       16000000 kB
MemFree:           600000 kB
MemAvailable:    12000000 kB
Cached:          3211000 kB
---
0.35 0.20 0.12 1/300 123
---
Filesystem         1B-blocks        Used Available Use% Mounted on
/dev/sda1          500000000000 220000000000 340000000000  50% /
";

    #[test]
    fn saida_completa_e_parseada() {
        let res = parsear(FIXTURE_COMPLETA, "app-01");
        assert!(res.erros.is_empty());
        let info = res.dados.expect("fixture completa é bem formada");
        assert_eq!(info.nome, "app-01");
        assert_eq!(info.sistema, "Linux");
        assert_eq!(info.kernel, "6.8.2-1-generic");
        assert_eq!(info.uptime_segundos, 3800);
        assert_eq!(info.memoria_total_bytes, 16_384_000_000);
        assert_eq!(info.memoria_usada_bytes, 4_096_000_000);
        assert_eq!(info.carga_1m, 0.35);
        assert_eq!(info.carga_5m, 0.20);
        assert_eq!(info.carga_15m, 0.12);
        assert_eq!(info.disco_total_bytes, 500_000_000_000);
        assert_eq!(info.disco_usado_bytes, 220_000_000_000);
    }

    /// Criterio 2: a memória usada SEGUE o MemAvailable. Se a implementação
    /// usar MemFree (600000 kB), o esperado aqui deixa de bater — MemAvailable
    /// (12000000 kB) é muito maior.
    #[test]
    fn memoria_em_kb_para_bytes() {
        let (total, usada) = parsear_memoria("MemTotal: 16000000 kB\nMemAvailable: 12000000 kB\n")
            .expect("meminfo válido para o teste");
        // 16000000 kB → 16_384_000_000 bytes (× 1024, não × 1000).
        assert_eq!(total, 16_384_000_000);
        assert_eq!(usada, 4_096_000_000);
    }

    #[test]
    fn memoria_usada_considera_mem_available() {
        let res = parsear(FIXTURE_COMPLETA, "app-01");
        let info = res.dados.expect("memória deve ser parseada");
        // (16000000 − 12000000) kB × 1024 = 4_096_000_000 bytes.
        // Com MemFree seria (16000000 − 600000) × 1024 — valor muito maior.
        assert_eq!(info.memoria_usada_bytes, 4_096_000_000);
    }

    #[test]
    fn saida_truncada_sem_disco_deixa_parcial_com_erro() {
        // Falta a seção do df: uptime e memória ainda são aproveitados e o
        // disco fica no padrão — NADA de panic.
        let truncada = "\
Linux 6.2.0
---
456.5 200.2
---
MemTotal: 16000000 kB
MemAvailable: 14000000 kB
---
1.50 0.00 0.00
";
        let res = parsear(truncada, "app-01");
        let info = res.dados.expect("parcial ainda tem dados");
        assert_eq!(info.uptime_segundos, 456);
        assert_eq!(info.memoria_total_bytes, 16_384_000_000);
        assert_eq!(info.disco_total_bytes, 0);
        assert_eq!(info.disco_usado_bytes, 0);
        assert!(!res.erros.is_empty());
    }

    #[test]
    fn saida_irreconhecivel_devolve_none() {
        // Linha única sem a forma de uname ("<sistema> <kernel>"): nenhuma das
        // 5 seções aproveita — sobra só o erro descritivo.
        let res = parsear("gibberish", "app-01");
        assert!(res.dados.is_none());
        assert!(!res.erros.is_empty());
        assert_eq!(res.erros[0].coletor, "sistema:app-01");
    }

    #[test]
    fn comando_encadeia_cinco_secoes() {
        // Já que tudo corre numa única invocação, o comando deve conter 4
        // marcadores — 5 seções — para não pagar 5 conexões.
        assert_eq!(COMANDO_SISTEMA.matches(MARCADOR).count(), 4);
    }
}
