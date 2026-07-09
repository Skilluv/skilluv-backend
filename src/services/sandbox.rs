use serde::{Deserialize, Serialize};

use crate::errors::AppError;

/// Language mapping: name → Judge0 language_id
/// Judge0 CE 1.13.1 language IDs
pub fn language_id(lang: &str) -> Option<u32> {
    match lang.to_lowercase().as_str() {
        // Tier 1 — 20 core languages
        "python" | "python3" => Some(71),
        "javascript" | "js" => Some(63),
        "typescript" | "ts" => Some(74),
        "rust" | "rs" => Some(73),
        "go" | "golang" => Some(60),
        "java" => Some(62),
        "c" => Some(50),
        "cpp" | "c++" => Some(54),
        "csharp" | "c#" => Some(51),
        "ruby" => Some(72),
        "php" => Some(68),
        "swift" => Some(83),
        "kotlin" => Some(78),
        "scala" => Some(81),
        "r" => Some(80),
        "bash" | "shell" => Some(46),
        "sql" | "sqlite" => Some(82),
        "lua" => Some(64),
        "perl" => Some(85),
        "haskell" => Some(61),
        // Tier 2 — additional languages
        "elixir" => Some(57),
        "clojure" => Some(86),
        "dart" => Some(90),
        "fsharp" | "f#" => Some(87),
        "groovy" => Some(88),
        "pascal" => Some(67),
        "fortran" => Some(59),
        "cobol" => Some(77),
        "assembly" | "asm" => Some(45),
        "ocaml" => Some(65),
        "erlang" => Some(58),
        "prolog" => Some(69),
        "lisp" | "commonlisp" => Some(55),
        "d" => Some(56),
        "racket" => Some(79),
        "text" | "plaintext" => Some(43),
        _ => None,
    }
}

pub fn supported_languages() -> Vec<LanguageInfo> {
    vec![
        // Tier 1
        LanguageInfo::tier1("python", "Python 3", 71),
        LanguageInfo::tier1("javascript", "JavaScript (Node.js)", 63),
        LanguageInfo::tier1("typescript", "TypeScript", 74),
        LanguageInfo::tier1("rust", "Rust", 73),
        LanguageInfo::tier1("go", "Go", 60),
        LanguageInfo::tier1("java", "Java", 62),
        LanguageInfo::tier1("c", "C (GCC)", 50),
        LanguageInfo::tier1("cpp", "C++ (GCC)", 54),
        LanguageInfo::tier1("csharp", "C#", 51),
        LanguageInfo::tier1("ruby", "Ruby", 72),
        LanguageInfo::tier1("php", "PHP", 68),
        LanguageInfo::tier1("swift", "Swift", 83),
        LanguageInfo::tier1("kotlin", "Kotlin", 78),
        LanguageInfo::tier1("scala", "Scala", 81),
        LanguageInfo::tier1("r", "R", 80),
        LanguageInfo::tier1("bash", "Bash", 46),
        LanguageInfo::tier1("sql", "SQLite", 82),
        LanguageInfo::tier1("lua", "Lua", 64),
        LanguageInfo::tier1("perl", "Perl", 85),
        LanguageInfo::tier1("haskell", "Haskell", 61),
        // Tier 2
        LanguageInfo::tier2("elixir", "Elixir", 57),
        LanguageInfo::tier2("clojure", "Clojure", 86),
        LanguageInfo::tier2("dart", "Dart", 90),
        LanguageInfo::tier2("fsharp", "F#", 87),
        LanguageInfo::tier2("groovy", "Groovy", 88),
        LanguageInfo::tier2("pascal", "Pascal", 67),
        LanguageInfo::tier2("fortran", "Fortran", 59),
        LanguageInfo::tier2("cobol", "COBOL", 77),
        LanguageInfo::tier2("assembly", "Assembly (NASM)", 45),
        LanguageInfo::tier2("ocaml", "OCaml", 65),
        LanguageInfo::tier2("erlang", "Erlang", 58),
        LanguageInfo::tier2("prolog", "Prolog", 69),
        LanguageInfo::tier2("lisp", "Common Lisp", 55),
        LanguageInfo::tier2("d", "D", 56),
        LanguageInfo::tier2("racket", "Racket", 79),
    ]
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageInfo {
    pub id: String,
    pub name: String,
    pub judge0_id: u32,
    pub tier: u8,
}

impl LanguageInfo {
    fn tier1(id: &str, name: &str, judge0_id: u32) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            judge0_id,
            tier: 1,
        }
    }
    fn tier2(id: &str, name: &str, judge0_id: u32) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            judge0_id,
            tier: 2,
        }
    }
}

/// Result from Judge0 execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub compile_output: Option<String>,
    pub status: ExecutionStatus,
    pub time: Option<String>,
    pub memory: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStatus {
    pub id: u32,
    pub description: String,
}

/// Judge0 submission response
#[derive(Debug, Deserialize)]
struct Judge0SubmissionResponse {
    #[allow(dead_code)]
    token: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
    compile_output: Option<String>,
    status: Option<Judge0Status>,
    time: Option<String>,
    memory: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Judge0Status {
    id: u32,
    description: String,
}

#[derive(Debug, Deserialize)]
struct Judge0TokenResponse {
    token: String,
}

pub struct SandboxService {
    judge0_url: String,
    client: reqwest::Client,
}

impl SandboxService {
    pub fn new(judge0_url: &str) -> Self {
        Self {
            judge0_url: judge0_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(3))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    /// Submit code and wait for result (synchronous mode)
    pub async fn execute(
        &self,
        source_code: &str,
        language: &str,
        stdin: Option<&str>,
        expected_output: Option<&str>,
        cpu_time_limit: Option<f32>,
        memory_limit: Option<u32>,
    ) -> Result<ExecutionResult, AppError> {
        let lang_id = language_id(language)
            .ok_or_else(|| AppError::Validation(format!("Unsupported language: {language}")))?;

        let mut body = serde_json::json!({
            "source_code": source_code,
            "language_id": lang_id,
            "cpu_time_limit": cpu_time_limit.unwrap_or(10.0),
            "memory_limit": memory_limit.unwrap_or(512_000),
        });

        if let Some(input) = stdin {
            body["stdin"] = serde_json::json!(input);
        }
        if let Some(expected) = expected_output {
            body["expected_output"] = serde_json::json!(expected);
        }

        // Use wait=true for synchronous execution (Judge0 waits for result)
        let url = format!(
            "{}/submissions?base64_encoded=false&wait=true&fields=stdout,stderr,compile_output,status,time,memory",
            self.judge0_url
        );

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Judge0 request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Judge0 returned {status}: {text}"
            )));
        }

        let result: Judge0SubmissionResponse = response
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Judge0 response parse failed: {e}")))?;

        let status = result.status.unwrap_or(Judge0Status {
            id: 0,
            description: "Unknown".to_string(),
        });

        Ok(ExecutionResult {
            stdout: result.stdout,
            stderr: result.stderr,
            compile_output: result.compile_output,
            status: ExecutionStatus {
                id: status.id,
                description: status.description,
            },
            time: result.time,
            memory: result.memory.map(|m| m as u64),
        })
    }

    /// Submit code asynchronously (returns token for polling)
    pub async fn execute_async(
        &self,
        source_code: &str,
        language: &str,
        stdin: Option<&str>,
    ) -> Result<String, AppError> {
        let lang_id = language_id(language)
            .ok_or_else(|| AppError::Validation(format!("Unsupported language: {language}")))?;

        let mut body = serde_json::json!({
            "source_code": source_code,
            "language_id": lang_id,
            "cpu_time_limit": 10.0,
            "memory_limit": 512_000,
        });

        if let Some(input) = stdin {
            body["stdin"] = serde_json::json!(input);
        }

        let url = format!("{}/submissions?base64_encoded=false", self.judge0_url);

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Judge0 request failed: {e}")))?;

        let result: Judge0TokenResponse = response
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Judge0 response parse failed: {e}")))?;

        Ok(result.token)
    }

    /// Get result by token (for async polling)
    pub async fn get_result(&self, token: &str) -> Result<ExecutionResult, AppError> {
        let url = format!(
            "{}/submissions/{}?base64_encoded=false&fields=stdout,stderr,compile_output,status,time,memory",
            self.judge0_url, token
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Judge0 request failed: {e}")))?;

        let result: Judge0SubmissionResponse = response
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Judge0 response parse failed: {e}")))?;

        let status = result.status.unwrap_or(Judge0Status {
            id: 0,
            description: "Unknown".to_string(),
        });

        Ok(ExecutionResult {
            stdout: result.stdout,
            stderr: result.stderr,
            compile_output: result.compile_output,
            status: ExecutionStatus {
                id: status.id,
                description: status.description,
            },
            time: result.time,
            memory: result.memory.map(|m| m as u64),
        })
    }

    /// Check if Judge0 is healthy
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/system_info", self.judge0_url);
        self.client.get(&url).send().await.is_ok()
    }
}
