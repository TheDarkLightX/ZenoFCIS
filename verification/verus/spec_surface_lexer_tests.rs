use super::*;
use alloc::string::ToString;
use lexical_core::LexemeKind;

#[test]
fn successful_original_frontiers_and_final_eof() {
    for source in [
        "",
        " // λ comment\n\t",
        "alpha 42 + beta",
        "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    ] {
        let (tokens, diagnostics) = surface_lexer::lex(source, SourceLimits::default());
        assert!(diagnostics.is_empty(), "{source:?}");
        let (eof, prefix) = tokens.split_last().unwrap();
        assert_eq!(eof.kind, LexemeKind::End);
        assert_eq!(eof.spelling, "");
        assert_eq!(eof.span.start(), source.len() as u32);
        assert_eq!(eof.span.end(), source.len() as u32);
        assert!(prefix.iter().all(|token| token.kind != LexemeKind::End));
        assert!(source_token::tokens_match_source(
            source,
            prefix,
            SourceLimits::default().max_tokens()
        ));
    }
}

#[test]
fn non_ascii_diagnostics_preserve_whole_scalar_and_byte_offsets() {
    for character in ['λ', '€', '😀'] {
        let source = alloc::format!("a {character} b");
        let (tokens, diagnostics) = surface_lexer::lex(&source, SourceLimits::default());
        assert_eq!(diagnostics.len(), 1);
        let error = &diagnostics[0];
        assert_eq!(error.code(), DiagnosticCode::UnexpectedCharacter);
        assert_eq!(error.actual(), character.to_string());
        assert_eq!(error.span().start(), 2);
        assert_eq!(error.span().end(), 2 + character.len_utf8() as u32);
        assert_eq!(tokens.last().unwrap().span.start(), source.len() as u32);
    }
}

#[test]
fn refusal_limits_keep_existing_diagnostics() {
    let limits = SourceLimits::try_new(2, 1, 1).unwrap();
    let (tokens, diagnostics) = surface_lexer::lex("abc", limits);
    assert_eq!(diagnostics[0].code(), DiagnosticCode::SourceTooLarge);
    assert_eq!(tokens[0].span.start(), 0);
    let limits = SourceLimits::try_new(10, 1, 1).unwrap();
    let (_, diagnostics) = surface_lexer::lex("a b", limits);
    assert_eq!(diagnostics[0].code(), DiagnosticCode::TokenLimit);
    let (tokens, diagnostics) = surface_lexer::lex("λ€😀", limits);
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(tokens.last().unwrap().span.start(), 5);
}
