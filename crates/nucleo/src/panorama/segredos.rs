// Redação de variáveis de ambiente SENSÍVEIS antes de gravar o snapshot.
//
// Esta é a parte mais crítica do módulo `panorama`: `docker inspect` devolve
// o ambiente completo de cada container — senhas, tokens, chaves — e o
// snapshot vai para disco e é lido por outra aplicação. As regras de ouro:
//   - a redação acontece ANTES de gravar, nunca depois;
//   - na dúvida, redigir: um falso positivo redige uma variável qualquer,
//     um falso negativo VASA um segredo para o arquivo.
//
// Módulo puro — texto na entrada, mapa na saída, sem um pingo de I/O —
// 100% testável com strings inline.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;

/// Valor que substitui o segredo. Uma sentinela (e não vazio) de propósito:
/// o consumidor do snapshot distingue "variável não foi coletada" (ausente)
/// de "existe, mas é segredo" (redigida).
pub const REDIGIDO: &str = "***REDIGIDO***";

/// Padrão de nomes de chave que sugerem segredo, case-insensitive.
///
/// `(?i)` liga o modo case-insensitive e as alternativas casam em QUALQUER
/// posição do nome (sem amarração `^`/`$`) — então `DB_PASSWORD`,
/// `MEU_TOKEN_AQUI`, `x-api-key` e `APP_SECRET_V2` são todos flagrados.
const PADRAO_SEGREDO_TEXTO: &str =
    r"(?i)SENHA|PASSWORD|PASSWD|SECRET|TOKEN|KEY|CREDENTIAL|AUTH|PRIVADA|PRIVATE";

/// Compila o padrão UMA única vez, na primeira chamada, e o guarda numa
/// estática para a vida do processo. O `OnceLock` (std, sem dep extra) é o
/// jeito moderno de "inicializar um global caro uma vez": cada célula só pode
/// ser preenchida uma vez e, depois, lida sem lock.
/// docs: https://doc.rust-lang.org/std/sync/struct.OnceLock.html
static PADRAO_SEGREDO: OnceLock<Regex> = OnceLock::new();

/// `true` quando o nome da chave sugere segredo.
///
/// O `expect` aqui inicializa o `OnceLock`. `Regex::new` não é `const`, então
/// a compilação acontece na primeira execução; a literal acima é sempre uma
/// expressão regular válida, então falhar é impossível. Num código com tipo de
/// erro, o resultado seria transformado em erro — para uma entrada que não
/// pode falhar, paniquear com uma mensagem descritiva é o diagnóstico honesto.
fn chave_e_sensivel(chave: &str) -> bool {
    let padrao = PADRAO_SEGREDO
        .get_or_init(|| Regex::new(PADRAO_SEGREDO_TEXTO).expect("padrão literal sempre válido"));
    padrao.is_match(chave)
}

/// Substitui o valor de toda chave cujo nome sugere segredo. O restante é
/// preservado byte a byte — valor vira `***REDIGIDO***`, nunca parcialmente.
pub fn redigir_variaveis(variaveis: BTreeMap<String, String>) -> BTreeMap<String, String> {
    // `into_iter` consome o mapa original (evita clonar); o `map` + `collect`
    // devolve um mapa novo. `BTreeMap` garante ordem determinística na
    // serialização — chave em ordem alfabética no JSON.
    variaveis
        .into_iter()
        .map(|(chave, valor)| {
            if chave_e_sensivel(&chave) {
                (chave, REDIGIDO.to_string())
            } else {
                (chave, valor)
            }
        })
        .collect()
}

/// Converte uma lista `CHAVE=valor` do `docker inspect` num mapa JÁ redigido.
///
/// Regras de interpretação:
/// - linha sem `=` é descartada (tem `None` do split, sem chave/valor);
/// - o valor pode conter `=` — separa-se apenas no PRIMEIRO;
/// - a chave é aparada (`trim`); o valor NÃO (espaço pode ser significativo).
pub fn variaveis_de_lista(linhas: &[String]) -> BTreeMap<String, String> {
    // Monta o mapa bruto e delega a redação à `redigir_variaveis`: assim a
    // pergunta "o que é sensível?" tem UMA única fonte de verdade — o mesmo
    // padrão que aplica a quem montar um mapa por outra via.
    let mut brutas = BTreeMap::new();
    for linha in linhas {
        // `split_once` quebra no PRIMEIRO `=` e devolve `None` quando não há
        // separador — a linha é descartada.
        let Some((chave, valor)) = linha.split_once('=') else {
            continue;
        };
        // Aparar SÓ a chave normaliza nomes tipo ` TOKEN=` (que o compose
        // pode gerar); chave vazia (`=valor`) também fica de fora.
        let chave = chave.trim();
        if chave.is_empty() {
            continue;
        }
        brutas.insert(chave.to_string(), valor.to_string());
    }
    redigir_variaveis(brutas)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Converte pares `(chave, valor)` num `BTreeMap` — menos ruído nos
    /// testes.
    fn mapa_de(pares: &[(&str, &str)]) -> BTreeMap<String, String> {
        pares
            .iter()
            .map(|(chave, valor)| (chave.to_string(), valor.to_string()))
            .collect()
    }

    /// Acceptance 1: `DB_PASSWORD=hunter2` tem o valor REDIGIDO em vez do
    /// segredo.
    #[test]
    fn senha_comum_e_redigida() {
        let mapa = variaveis_de_lista(&["DB_PASSWORD=hunter2".to_string()]);
        assert_eq!(mapa["DB_PASSWORD"], REDIGIDO);
        // Prova extra contra "regressão silenciosa": não basta conferir só a
        // presença da sentinela — garante que o segredo sumiu da saída.
        let json = serde_json::to_string(&mapa).expect("serializar mapa");
        assert!(!json.contains("hunter2"), "segredo vazou: {json}");
    }

    /// Acceptance 2: o token casa em QUALQUER posição do nome, não precisa
    /// ser prefixo.
    #[test]
    fn casa_token_no_meio_do_nome() {
        let mapa = variaveis_de_lista(&[
            "MEU_TOKEN_AQUI=abc123".to_string(),
            "APP_SECRET_V2=xyz999".to_string(),
            "x-api-key=api-abc".to_string(),
        ]);
        assert_eq!(mapa["MEU_TOKEN_AQUI"], REDIGIDO);
        assert_eq!(mapa["APP_SECRET_V2"], REDIGIDO);
        assert_eq!(mapa["x-api-key"], REDIGIDO);
    }

    /// Acceptance 3: case-insensitive — maiúsculas, minúsculas e misturado
    /// são todos flagrados.
    #[test]
    fn diferenca_de_caixa_nao_engana() {
        let mapa = variaveis_de_lista(&[
            "password=minusc".to_string(),
            "Password=Misto".to_string(),
            "PASSWORD=MAIUSC".to_string(),
        ]);
        for valor in mapa.values() {
            assert_eq!(valor, REDIGIDO);
        }
    }

    /// Acceptance 4: chave inocente é preservada com o valor EXATO (sem
    /// trim, sem troca silenciosa).
    #[test]
    fn chave_inocente_e_preservada() {
        let mapa = variaveis_de_lista(&[
            "PATH=/usr/local/bin:/usr/bin".to_string(),
            "LANG=pt_BR.UTF-8".to_string(),
            "VIRTUAL_HOST=app.exemplo.interno".to_string(),
            "TZ=America/Sao_Paulo".to_string(),
        ]);
        assert_eq!(mapa["PATH"], "/usr/local/bin:/usr/bin");
        assert_eq!(mapa["LANG"], "pt_BR.UTF-8");
        assert_eq!(mapa["VIRTUAL_HOST"], "app.exemplo.interno");
        assert_eq!(mapa["TZ"], "America/Sao_Paulo");
    }

    /// Acceptance 5: valor com `=` dentro sobrevive INTEIRO quando a chave é
    /// inocente — o separador é apenas o primeiro `=`.
    #[test]
    fn opcoes_com_igual_interno_e_preservada() {
        let mapa = variaveis_de_lista(&["OPCOES=a=1,b=2".to_string()]);
        assert_eq!(mapa["OPCOES"], "a=1,b=2");
    }

    /// Acceptance 6: valor com `=` dentro é redigido INTEIRO quando a chave é
    /// sensível — nenhum fragmento do valor original na saída.
    #[test]
    fn igual_interno_e_redigido_por_inteiro() {
        let mapa = variaveis_de_lista(&[
            "DB_PASSWORD=segredo=a=1,b=2".to_string(),
            "API_TOKEN=token=x=y=z".to_string(),
        ]);
        assert_eq!(mapa["DB_PASSWORD"], REDIGIDO);
        assert_eq!(mapa["API_TOKEN"], REDIGIDO);
        // Nem a parte anterior ao `=` nem o fragmento interno podem vazar.
        let json = serde_json::to_string(&mapa).expect("serializar mapa");
        assert!(!json.contains("segredo"));
        assert!(!json.contains("a=1"));
        assert!(!json.contains("x=y=z"));
    }

    /// Acceptance 7: linha sem `=` é descartada; chave vazia (`=valor`) não
    /// entra no mapa.
    #[test]
    fn linha_invalida_e_ignorada() {
        let mapa = variaveis_de_lista(&[
            "SEM_IGUAL".to_string(),
            "=valor_anonimo".to_string(),
            "OK=mantem".to_string(),
        ]);
        assert_eq!(mapa.len(), 1, "só a linha válida deve entrar");
        assert_eq!(mapa["OK"], "mantem");
    }

    /// Acceptance 8: chave com espaços em volta é normalizada (` TOKEN ` →
    /// `TOKEN`) e, sendo sensível, redigida.
    #[test]
    fn espacos_em_volta_sao_aparados() {
        let mapa = variaveis_de_lista(&["  TOKEN   =abc".to_string()]);
        assert_eq!(mapa["TOKEN"], REDIGIDO);
    }

    /// Acceptance 9 (a que de fato protege): com ~20 chaves sensíveis
    /// variadas, NENHUM dos valores originais aparece na serialização JSON do
    /// resultado. Falha se qualquer caminho novo da implementação escapar da
    /// redação — por isso escolhemos valores bem específicos, difíceis de
    /// coincidir por acaso.
    #[test]
    fn varredura_nao_deixa_segredo_no_json() {
        let originais: BTreeMap<String, String> = mapa_de(&[
            ("DB_PASSWORD", "10x-4afb-12kd-21pr"),
            ("POSTGRES_PASSWORD", "21-mxvdh-tts-z1-1"),
            ("MINIO_ROOT_PASSWORD", "22-zxk1-xp9-mnwb"),
            ("REDIS_PASSWD", "bc-qw31-htrs-0pq"),
            ("SMTP_PASSWORD", "zl8-nn0-3abc"),
            ("JWT_SECRET", "sk-jwt-9d2e-88w0"),
            ("AWS_SECRET_ACCESS_KEY", "ak-bc1-7q7-xfid"),
            ("CLIENT_SECRET", "cs-90f2-f1a-2bb0"),
            ("API_KEY", "key-77abcd-9jg"),
            ("GITHUB_TOKEN", "gh-tok-xyz99abc1"),
            ("ATLASSIAN_TOKEN", "atl-44-fh-2zz"),
            ("AUTH_SERVICE_KEY", "auth-1010x-vtu"),
            ("SENHA_SUPERVISOR", "spd77-fjk-4xz"),
            ("PRIVATE_KEY", "rsa-priv-7da9c2"),
            ("WIREGUARD_PRIVATE_KEY", "wg-frc-5f0-9e3"),
            ("CA_PRIVATE_KEY", "ca-9mdv-1q2x"),
            ("CREDENTIAL_FEED", "cf-ff-11aa77"),
            ("SIGNING_PRIVATE_KEY", "sgp-9z-9f1"),
            ("DB_CREDENTIALS", "dbql-5529-1"),
            ("REGISTRY_AUTH", "ra-20-777z-1c"),
        ]);
        assert_eq!(originais.len(), 20, "varredura deve cobrir 20 chaves");

        let redigidas = redigir_variaveis(originais.clone());
        let json = serde_json::to_string(&redigidas).expect("serializar mapa redigido");

        // `iter()` devolve pares `(&String, &String)` — evitamos reindexar o
        // mapa dentro do laço.
        for (chave, valor) in &originais {
            assert_eq!(
                redigidas[chave], REDIGIDO,
                "chave {chave} deveria ter sido redigida"
            );
            assert!(
                !json.contains(valor.as_str()),
                "valor de {chave} vazou na serialização: {json}"
            );
        }
    }

    /// A API de mapa também redige (mesma fonte de verdade) — sem passar
    /// pela lista.
    #[test]
    fn redige_mapa_montado_manualmente() {
        let brutas = mapa_de(&[("APP_SECRET", "cuidado"), ("PATH", "/bin:/usr/bin")]);
        let redigidas = redigir_variaveis(brutas);
        assert_eq!(redigidas["APP_SECRET"], REDIGIDO);
        assert_eq!(redigidas["PATH"], "/bin:/usr/bin");
    }
}
