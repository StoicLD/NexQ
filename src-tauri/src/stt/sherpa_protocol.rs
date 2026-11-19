//! Protocol verified against the pinned Windows sherpa-onnx v2.0.1 binary.
use std::path::Path;

pub fn arguments(
    model_flag: &str,
    model: &Path,
    tokens: &Path,
    language: Option<&str>,
    wav: &Path,
) -> Vec<String> {
    // Each option is one argv entry. Command handles spaces in Windows paths.
    let mut args = vec![
        format!("{}={}", model_flag, model.display()),
        format!("--tokens={}", tokens.display()),
        "--num-threads=4".into(),
        "--provider=cpu".into(),
    ];
    if let Some(language) = language {
        let lang = match language.split('-').next().unwrap_or("zh") {
            "zh" => "zh",
            "ja" => "ja",
            "ko" => "ko",
            "yue" => "yue",
            _ => "en",
        };
        args.push(format!("--sense-voice-language={}", lang));
        args.push("--sense-voice-use-itn=true".into());
    }
    args.push(wav.to_string_lossy().into_owned());
    args
}

pub fn recognized_text(stderr: &str, stdout: &str, success: bool) -> Option<String> {
    if !success {
        return None;
    }
    for line in stderr.lines().chain(stdout.lines()) {
        let line = line.trim();
        // Newer sherpa releases emit one JSON object with text + token details.
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                if !text.trim().is_empty() {
                    return Some(text.trim().to_string());
                }
            }
        }
        // Retain compatibility with legacy text output.
        if let Some(text) = line
            .strip_prefix("text:")
            .or_else(|| line.strip_prefix("text :"))
        {
            if !text.trim().is_empty() {
                return Some(text.trim().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_paths_with_spaces_stay_in_single_arguments() {
        let args = arguments(
            "--sense-voice-model",
            Path::new(r"C:\Users\Dong Li\model.int8.onnx"),
            Path::new(r"C:\Users\Dong Li\tokens.txt"),
            Some("zh-CN"),
            Path::new(r"C:\Test Audio\input.wav"),
        );
        assert_eq!(
            args[0],
            r"--sense-voice-model=C:\Users\Dong Li\model.int8.onnx"
        );
        assert_eq!(args[1], r"--tokens=C:\Users\Dong Li\tokens.txt");
        assert!(args.contains(&"--sense-voice-language=zh".to_string()));
        assert_eq!(args.last().unwrap(), r"C:\Test Audio\input.wav");
        assert_eq!(args.len(), 7);
    }
    #[test]
    fn accepts_chinese_json_after_diagnostics() {
        assert_eq!(
            recognized_text(
                "Creating recognizer ...\nDone!",
                r#"{"text":"如何保证消息不丢失？","tokens":["如何"]}"#,
                true
            ),
            Some("如何保证消息不丢失？".into())
        );
    }
    #[test]
    fn retains_legacy_output_and_ignores_errors() {
        assert_eq!(recognized_text("text: 中文", "", true), Some("中文".into()));
        assert_eq!(
            recognized_text("text : 中文", "", true),
            Some("中文".into())
        );
        assert_eq!(recognized_text("", r#"{"text":"不应接受"}"#, false), None);
        assert_eq!(recognized_text("diagnostic", r#"{"text":" "}"#, true), None);
    }
}
