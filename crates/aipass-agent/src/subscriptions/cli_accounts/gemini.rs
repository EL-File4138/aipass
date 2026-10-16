//! Read the pinned CLI home's explicit Cloud project without evaluating shell
//! expressions or borrowing another account's process environment.
use super::*;
pub(super) fn project(dir: &Path) -> Result<String> {
    use std::io::Read;
    let file = match std::fs::File::open(dir.join(".env")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(_) => return Err("Cannot read Gemini CLI project settings".into()),
    };
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    file.take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read Gemini CLI project settings")?;
    if bytes.len() > 64 * 1024 {
        return Err("Gemini CLI project settings exceed limit".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Invalid Gemini CLI project settings")?;
    let mut project = String::new();
    let mut alternate = String::new();
    for line in text.lines() {
        let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !matches!(
            key.trim(),
            "GOOGLE_CLOUD_PROJECT" | "GOOGLE_CLOUD_PROJECT_ID"
        ) {
            continue;
        }
        let value = value.trim();
        let value = if value.starts_with(['\'', '"']) {
            let quote = value.as_bytes()[0] as char;
            let end = value[1..]
                .find(quote)
                .ok_or("Invalid Gemini project quote")?
                + 1;
            if !value[end + 1..].trim().is_empty() && !value[end + 1..].trim().starts_with('#') {
                return Err("Invalid Gemini project setting".into());
            }
            &value[1..end]
        } else {
            value.split(" #").next().unwrap_or("").trim()
        };
        if value.len() > 253
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
        {
            return Err("Gemini project must be a literal Google Cloud project ID".into());
        }
        if key.trim() == "GOOGLE_CLOUD_PROJECT" {
            project = value.into();
        } else {
            alternate = value.into();
        }
    }
    let project = if project.is_empty() {
        alternate
    } else {
        project
    };
    if !project.is_empty() && project.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Gemini requires a project ID rather than a numeric project number".into());
    }
    Ok(project)
}
pub(super) fn setup(project: &str) -> Value {
    let mut body = json!({"metadata":{"ideType":"IDE_UNSPECIFIED","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}});
    if !project.is_empty() {
        body["metadata"]["duetProject"] = json!(project);
        body["cloudaicompanionProject"] = json!(project);
    }
    body
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_project_is_bound_and_never_evaluated_as_shell() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "UNRELATED_SECRET=do-not-copy\nexport GOOGLE_CLOUD_PROJECT='my-project' # selected\n",
        )
        .unwrap();
        assert_eq!(project(dir.path()).unwrap(), "my-project");
        assert_eq!(setup("my-project")["metadata"]["duetProject"], "my-project");
        std::fs::write(
            dir.path().join(".env"),
            "GOOGLE_CLOUD_PROJECT=$(invalid-command)",
        )
        .unwrap();
        assert!(project(dir.path()).is_err());
    }
}
