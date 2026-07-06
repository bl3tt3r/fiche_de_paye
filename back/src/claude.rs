use duct::cmd;
use serde::Deserialize;

pub struct Claude;

#[derive(Deserialize, Debug)]
pub struct ClaudeResponse {
    pub r#type: String,
    pub subtype: String,
    pub is_error: bool,
    pub api_error_status: Option<String>,
    pub duration_ms: i32,
    pub duration_api_ms: i32,
    pub ttft_ms: i32,
    pub ttft_stream_ms: i32,
    pub time_to_request_ms: i32,
    pub num_turns: i32,
    pub result: String,
}

impl Claude {
    pub fn installed(&self) -> bool {
        let (code, _stdout, _stderr) = exec(vec!["--version"]);
        code == 0
    }

    pub fn connected(&self) -> bool {
        let (code, stdout, _stderr) = exec(vec!["auth", "status"]);
        code == 0 && stdout.contains("\"loggedIn\": true")
    }

    pub fn prompt(
        &self,
        system_prompt: &'static str,
        user_prompt: &str,
    ) -> Result<ClaudeResponse, String> {
        let (code, stdout, stderr) = exec(vec![
            "-p",
            "--system-prompt",
            system_prompt,
            "--allowedTools",
            "Read",
            "--output-format",
            "json",
            user_prompt,
        ]);

        if code != 0 {
            return Err(stderr.clone());
        }

        Ok(
            serde_json::from_str::<ClaudeResponse>(&stdout).map_err(|error| {
                let msg = "Lecture de la réponse de claude code.";
                tracing::error!(cauded = %error,  msg);
                msg
            })?,
        )
    }
}

fn exec(args: Vec<&str>) -> (i32, String, String) {
    let process = cmd("claude", args.clone())
        .stdout_capture()
        .stderr_capture();
    let result = process.run();
    match result {
        Ok(result) => (
            result.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&result.stdout).to_string(),
            String::from_utf8_lossy(&result.stderr).to_string(),
        ),
        Err(error) => {
            let msg = "Execution d'une commande.";
            tracing::warn!(cauded = %error, command = format!("claude {}", args.join(" ")),  msg);
            (-1, "".to_string(), "".to_string())
        }
    }
}
