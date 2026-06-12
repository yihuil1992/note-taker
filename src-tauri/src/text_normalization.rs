use ferrous_opencc::{config::BuiltinConfig, OpenCC};

pub fn normalize_transcript_text(language_hint: &str, text: &str) -> String {
    if should_convert_to_simplified(language_hint, text) {
        return OpenCC::from_config(BuiltinConfig::T2s)
            .map(|converter| converter.convert(text))
            .unwrap_or_else(|_| text.to_string());
    }
    text.to_string()
}

fn should_convert_to_simplified(language_hint: &str, text: &str) -> bool {
    matches!(language_hint, "zh" | "zh-CN" | "Chinese" | "chinese")
        && text.chars().any(is_cjk_unified)
}

fn is_cjk_unified(character: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&character)
}

#[cfg(test)]
mod tests {
    use super::normalize_transcript_text;

    #[test]
    fn converts_traditional_chinese_to_simplified_for_zh_hint() {
        let text = normalize_transcript_text("zh", "我今天可以做四個人，然後這樣我們可以知道。");

        assert_eq!(text, "我今天可以做四个人，然后这样我们可以知道。");
    }

    #[test]
    fn leaves_japanese_text_untouched() {
        let text = normalize_transcript_text("ja", "これは日本語の文字起こしです。");

        assert_eq!(text, "これは日本語の文字起こしです。");
    }
}
