//! Test e2e — prouve que le backend peut appeler l'IA gRPC v2 sur une vraie
//! socket. Nécessite l'IA écoutant sur `GRPC_AI_URL` (défaut :50051).
//!
//! Skip auto si l'IA n'est pas joignable — évite de casser CI standalone.
//!
//! Lancement local :
//!     cd ../skilluv-ia
//!     SKILLUV_AI_MOCK_CLAUDE=1 uv run python -c "import asyncio; from src.grpc_server.server import serve; asyncio.run(serve())"

use std::time::Duration;

async fn connect_or_skip() -> Option<skilluv_backend::grpc::AiClient> {
    let url = std::env::var("GRPC_AI_URL")
        .unwrap_or_else(|_| "http://localhost:50051".to_string());
    skilluv_backend::grpc::AiClient::connect(&url).await
}

#[tokio::test]
async fn e2e_review_code_returns_response_from_real_ia() {
    let Some(ai) = connect_or_skip().await else {
        eprintln!("IA gRPC unreachable at :50051 — skipping e2e");
        return;
    };

    // Timeout court pour ne pas bloquer si l'IA est en surcharge.
    let call = tokio::time::timeout(
        Duration::from_secs(30),
        ai.review_code(
            "e2e-test-submission",
            "def add(a, b):\n    return a + b\n",
            "python",
            "Somme de deux entiers",
            "Écrire une fonction `add(a, b)` qui retourne la somme.",
            2,
        ),
    )
    .await;

    let response = call
        .expect("gRPC call timed out")
        .expect("gRPC call failed");

    // Mock LLM renvoie un dict conforme au schema — quality_score peut être 0
    // (int par défaut). Le contrat est juste : la réponse existe et a les champs.
    let _score = response.quality_score;
    let _summary: &str = &response.summary;
    let _model: &str = &response.model_version;
}

#[tokio::test]
async fn e2e_suggest_career_path_returns_response_from_real_ia() {
    let Some(ai) = connect_or_skip().await else {
        eprintln!("IA gRPC unreachable at :50051 — skipping e2e");
        return;
    };

    use skilluv_backend::grpc::proto::CareerPathRequest;
    let req = CareerPathRequest {
        user_id: "e2e-user-42".into(),
        skills: Vec::new(),
        working_languages: vec!["fr".into()],
        target_market: "africa".into(),
        max_suggestions: 3,
    };
    let call = tokio::time::timeout(
        Duration::from_secs(15),
        ai.suggest_career_path(req),
    )
    .await;

    let response = call
        .expect("gRPC call timed out")
        .expect("gRPC call failed");
    // Cas dégénéré côté IA : 0 skills → réponse safe sans Claude ; primary est
    // "dev-frontend" ou vide selon la version. On vérifie juste que ça renvoie.
    let _primary: &str = &response.primary_recommendation;
}
