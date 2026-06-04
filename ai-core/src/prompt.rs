pub(crate) fn join_parts(parts: &[String]) -> String {
    parts.join(" ").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::join_parts;

    #[test]
    fn joins_prompt_parts_with_spaces() {
        let parts = vec![
            "what".to_owned(),
            "is".to_owned(),
            "running".to_owned(),
            "on".to_owned(),
            "port".to_owned(),
            "3000".to_owned(),
        ];

        assert_eq!(join_parts(&parts), "what is running on port 3000");
    }

    #[test]
    fn trims_empty_quoted_parts() {
        let parts = vec!["".to_owned(), "  ".to_owned()];

        assert_eq!(join_parts(&parts), "");
    }
}
