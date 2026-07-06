use duct::cmd;

pub struct Claude;

impl Claude {
    pub fn installed(&self) -> bool {
        let (code, _stdout, _stderr) = exec("claude --version");
        code == 0
    }

    pub fn connected(&self) -> bool {
        let (code, stdout, _stderr) = exec("claude auth status");
        code == 0 && stdout.contains("\"loggedIn\": true")
    }

    pub fn prompt(
        &self,
        system_prompt: &'static str,
        user_prompt: &'static str,
    ) -> (i32, String, String) {
        exec(&format!(
            "claude -p --system-prompt {} --allowedTools Read --output-format json {}",
            system_prompt, user_prompt
        ))
    }
}

fn exec(command: &str) -> (i32, String, String) {
    let process = {
        let mut parts = command.split_whitespace();
        let program = parts.next().unwrap_or_default().to_string();
        let args: Vec<String> = parts.map(|m| m.to_string()).collect();
        cmd(program, args).stdout_capture().stderr_capture()
    };
    let result = process.run();
    match result {
        Ok(result) => (
            result.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&result.stdout).to_string(),
            String::from_utf8_lossy(&result.stderr).to_string(),
        ),
        Err(error) => {
            let msg = "Execution d'une commande.";
            tracing::warn!(cauded = %error, command , msg);
            (-1, "".to_string(), "".to_string())
        }
    }
}
