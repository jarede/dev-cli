// O CONTRATO do módulo `panorama`: a forma do snapshot JSON gravado em disco
// e consumido por uma aplicação externa. Este arquivo define o contrato e
// NADA mais — nenhuma coleta, nenhum I/O. As demais issues do módulo
// dependem destas structs.
//
// # Sobre o campo `versao`
//
// O consumidor compara `versao` com a constante que ele conhece e recusa o
// que não entende — mostrando "coletor em versão incompatível" em vez de
// quebrar com campo faltando no meio de um template.
//
// Regra para alterações futuras: **um campo novo e opcional NÃO incrementa a
// versão** (o leitor usa o padrão via `#[serde(default)]`). Sobe a versão
// apenas quando um campo existente muda de tipo, muda de significado ou
// desaparece.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Versão do formato do snapshot. O consumidor usa para recusar de forma
/// clara códigos que não entende em vez de quebrar por campo faltando.
pub const VERSAO_SNAPSHOT: u32 = 1;

/// Um inventário completo, gravado como um único documento JSON.
///
/// `#[serde(default)]` no nível da struct: todo campo ausente no JSON é
/// preenchido com o valor padrão em vez de falhar a desserialização — campo
/// novo e opcional num snapshot antigo não quebra leitores ao contrário.
/// docs: https://serde.rs/container-attrs.html#default
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    /// Deve ser `VERSAO_SNAPSHOT` do coletor que gravou.
    pub versao: u32,
    /// ISO 8601 sem fuso, precisão de segundo — exemplo: "2026-08-07T14:00:00".
    pub coletado_em: String,
    pub hosts: Vec<InfoHost>,
    pub containers: Vec<Container>,
    pub vhosts: Vec<VHost>,
    pub repositorios: Vec<Repositorio>,
    pub erros: Vec<ErroColeta>,
}

/// Saúde de um dos hosts da coleta.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoHost {
    /// Rótulo lógico vindo da config, NÃO o hostname real. A topologia é
    /// configurada em tempo de execução — nada de hostname hardcoded.
    pub nome: String,
    pub sistema: String,
    pub kernel: String,
    pub uptime_segundos: u64,
    pub memoria_total_bytes: u64,
    pub memoria_usada_bytes: u64,
    pub carga_1m: f64,
    pub carga_5m: f64,
    pub carga_15m: f64,
    pub disco_total_bytes: u64,
    pub disco_usado_bytes: u64,
}

/// Um container Docker inventariado (rodando ou não).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Container {
    /// Casa com o `nome` de um `Host` — em qual host o container roda.
    pub host: String,
    pub nome: String,
    pub imagem: String,
    pub estado: String,
    pub criado_em: String,
    pub iniciado_em: String,
    pub reinicios: u32,
    pub tem_healthcheck: bool,
    pub saude: Option<String>,
    /// Endereço público (vem do ambiente `VIRTUAL_HOST`) — a ponte entre o
    /// inventário de containers e o log do proxy reverso.
    pub vhost: Option<String>,
    pub limite_memoria_bytes: Option<u64>,
    pub memoria_usada_bytes: u64,
    pub cpu_percentual: f64,
    pub cpus_limite: Option<f64>,
    pub portas: Vec<String>,
    pub volumes: Vec<String>,
    /// Variáveis de ambiente JÁ redigidas — ver a issue da redação.
    pub variaveis: BTreeMap<String, String>,
}

/// Volume agregado de um vhost, lido do log do proxy reverso.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VHost {
    pub vhost: String,
    pub requisicoes: u64,
    pub erros_4xx: u64,
    pub erros_5xx: u64,
    /// Série diária, ordenada por data crescente.
    pub dias: Vec<DiaRequisicoes>,
    /// IPs de origem distintos, ordenados.
    pub maquinas: Vec<String>,
    /// Rotas mais requisitadas, ordenadas por contagem decrescente.
    pub rotas_top: Vec<RotaContagem>,
}

/// Contagem de um único dia para um vhost.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DiaRequisicoes {
    /// "YYYY-MM-DD".
    pub data: String,
    pub requisicoes: u64,
    pub erros_4xx: u64,
    pub erros_5xx: u64,
}

/// Contagem de uma única rota.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RotaContagem {
    pub rota: String,
    pub requisicoes: u64,
}

/// Um repositório da instância GitLab.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Repositorio {
    pub identificador: u64,
    pub caminho: String,
    pub branch_padrao: String,
    pub ultima_atividade: String,
    pub commits: u64,
    pub autores: Vec<Autor>,
    /// nome -> SHA do último commit da branch.
    pub branches: BTreeMap<String, String>,
    pub tem_ci: bool,
}

/// Um autor dentro de um repositório, com identidade JÁ unificada
/// (ver a issue de identidade).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Autor {
    pub nome: String,
    pub commits: u64,
    pub emails: Vec<String>,
    /// Arredondamento de `100 * commits / total` do repositório.
    pub percentual: u8,
}

/// Falha encontrada por um coletor — sempre parcial, nunca derruba a coleta.
/// O campo `coletor` identifica a origem (ex.: "docker:app-01").
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ErroColeta {
    pub coletor: String,
    pub mensagem: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializa e desserializa, devolvendo o valor reconstruído.
    /// `expect` é aceitável: round-trip de struct derivada de serde só falha
    /// se houver bug no próprio teste.
    fn round_trip(snapshot: &Snapshot) -> Snapshot {
        let json = serde_json::to_string(snapshot).expect("serializar snapshot");
        serde_json::from_str(&json).expect("desserializar snapshot")
    }

    /// Um snapshot completo, com as coleções fora de ordem de propósito
    /// para provar determinismo da serialização.
    fn exemplo() -> Snapshot {
        Snapshot {
            versao: VERSAO_SNAPSHOT,
            coletado_em: "2026-08-07T14:00:00".to_string(),
            hosts: vec![InfoHost {
                nome: "app-01".to_string(),
                sistema: "Fedora Linux".to_string(),
                kernel: "6.12.7".to_string(),
                uptime_segundos: 1_200_000,
                memoria_total_bytes: 16u64 * 1024 * 1024 * 1024,
                memoria_usada_bytes: 6u64 * 1024 * 1024 * 1024,
                carga_1m: 0.25,
                carga_5m: 0.10,
                carga_15m: 0.02,
                disco_total_bytes: 500u64 * 1024 * 1024 * 1024,
                disco_usado_bytes: 230u64 * 1024 * 1024 * 1024,
            }],
            containers: vec![Container {
                host: "app-01".to_string(),
                nome: "web-1".to_string(),
                imagem: "exemplo.interno/web:3.2".to_string(),
                estado: "running".to_string(),
                criado_em: "2026-07-01T10:00:00Z".to_string(),
                iniciado_em: "2026-07-04T08:00:00Z".to_string(),
                reinicios: 3,
                tem_healthcheck: true,
                saude: Some("healthy".to_string()),
                vhost: Some("app.exemplo.interno".to_string()),
                limite_memoria_bytes: Some(1_073_741_824),
                memoria_usada_bytes: 512_183_776,
                cpu_percentual: 2.5,
                cpus_limite: Some(2.0),
                portas: vec!["0.0.0.0:80->8000/tcp".to_string()],
                volumes: vec!["dados:/var/lib/app".to_string()],
                variaveis: BTreeMap::new(),
            }],
            vhosts: vec![VHost {
                vhost: "app.exemplo.interno".to_string(),
                requisicoes: 3,
                erros_4xx: 1,
                erros_5xx: 0,
                dias: vec![
                    DiaRequisicoes {
                        data: "2026-08-06".to_string(),
                        requisicoes: 1,
                        erros_4xx: 0,
                        erros_5xx: 0,
                    },
                    DiaRequisicoes {
                        data: "2026-08-07".to_string(),
                        requisicoes: 2,
                        erros_4xx: 1,
                        erros_5xx: 0,
                    },
                ],
                maquinas: vec!["10.1.30.44".to_string()],
                rotas_top: vec![
                    RotaContagem {
                        rota: "/pedidos".to_string(),
                        requisicoes: 1,
                    },
                    RotaContagem {
                        rota: "/home".to_string(),
                        requisicoes: 1,
                    },
                ],
            }],
            repositorios: vec![Repositorio {
                identificador: 42,
                caminho: "grupo/appz".to_string(),
                branch_padrao: "main".to_string(),
                ultima_atividade: "2026-08-07T10:00:00Z".to_string(),
                commits: 120,
                autores: vec![Autor {
                    nome: "Jarede F. Silva".to_string(),
                    commits: 100,
                    emails: vec!["j.silva@exemplo.interno".to_string()],
                    percentual: 83,
                }],
                branches: BTreeMap::from([
                    ("main".to_string(), "abc123".to_string()),
                    ("producao".to_string(), "def456".to_string()),
                ]),
                tem_ci: true,
            }],
            erros: vec![ErroColeta {
                coletor: "docker:app-02".to_string(),
                mensagem: "host inacessível".to_string(),
            }],
        }
    }

    /// Acceptance: `Snapshot` -> JSON -> `Snapshot` devolve valor igual.
    #[test]
    fn round_trip_devolve_valor_igual() {
        let original = exemplo();
        assert_eq!(original, round_trip(&original));
    }

    /// Acceptance: JSON com campos ausentes em todas as coleções
    /// (Vec/Option/BTreeMap) desserializa com os campos no padrão.
    #[test]
    fn json_com_campos_ausentes_desserializa() {
        // Nenhum campo de coleção presente: `#[serde(default)]` preenche vazio.
        let texto = r#"{ "versao": 1, "coletado_em": "2026-08-07T14:00:00" }"#;
        let snap: Snapshot = serde_json::from_str(texto).expect("desserializar sem coleções");
        assert_eq!(snap.versao, 1);
        assert!(snap.hosts.is_empty());
        assert!(snap.containers.is_empty());
        assert!(snap.vhosts.is_empty());
        assert!(snap.repositorios.is_empty());
        assert!(snap.erros.is_empty());
    }

    /// Acceptance: campo extra, desconhecido pelo contrato atual, não falha —
    /// o consumidor mais novo simplesmente o mantém ignorados.
    #[test]
    fn json_com_campo_extra_desserializa() {
        let json = r#"{ "versao": 1, "torpedo_futuro": {"n": 1} }"#;
        let snap: Snapshot = serde_json::from_str(json).expect("campo extra ignorado");
        assert_eq!(snap.versao, 1);
    }

    /// Acceptance: serializar o mesmo valor duas vezes produz a mesma string —
    /// ordem determinística (BTreeMap + ordem de campos do struct).
    #[test]
    fn serializacao_e_deterministica() {
        let a = serde_json::to_string(&exemplo()).expect("serializar");
        let b = serde_json::to_string(&exemplo()).expect("serializar");
        assert_eq!(a, b);
    }

    /// A ordem das chaves no JSON reflete a ordem lexicográfica do BTreeMap.
    #[test]
    fn branches_ordenam_lexicograficamente_no_json() {
        let repo = Repositorio {
            branches: BTreeMap::from([
                ("zeta".to_string(), "1".to_string()),
                ("alpha".to_string(), "2".to_string()),
            ]),
            ..Default::default()
        };
        let json = serde_json::to_string(&repo).expect("serializar");
        let pos_alpha = json.find("\"alpha\"").expect("alpha presente");
        let pos_zeta = json.find("\"zeta\"").expect("zeta presente");
        assert!(
            pos_alpha < pos_zeta,
            "BTreeMap deve vir em ordem alfabética"
        );
    }
}
