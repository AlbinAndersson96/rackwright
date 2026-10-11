use rackwright_core::limits::Limits;
use rackwright_core::name::{DocumentName, DocumentNameError, Name, NameError, auto_rename};

fn limits() -> Limits {
    Limits::default()
}

fn name(s: &str) -> Name {
    Name::parse(s, &limits()).unwrap()
}

#[test]
fn removes_all_whitespace() {
    assert_eq!(name(" DMM 1\t").as_str(), "DMM1");
}

#[test]
fn rejects_empty_after_normalisation() {
    assert_eq!(Name::parse("  \n", &limits()), Err(NameError::Empty));
}

#[test]
fn accepts_max_len_and_rejects_longer() {
    assert_eq!(name("ABCDEFGHIJ").as_str(), "ABCDEFGHIJ");
    assert_eq!(
        Name::parse("ABCDEFGHIJK", &limits()),
        Err(NameError::TooLong { max: 10 })
    );
}

#[test]
fn length_counts_characters_not_bytes() {
    // 10 characters, 20 bytes.
    assert_eq!(name("ÅÄÖåäöÅÄÖå").as_str(), "ÅÄÖåäöÅÄÖå");
}

#[test]
fn max_len_comes_from_limits() {
    let short = Limits {
        name_max_len: 3,
        ..Limits::default()
    };
    assert_eq!(
        Name::parse("ABCD", &short),
        Err(NameError::TooLong { max: 3 })
    );
}

#[test]
fn comparison_is_case_insensitive_including_non_ascii() {
    assert!(name("DMM").same_as(&name("dmm")));
    assert!(name("ÅSA").same_as(&name("åsa")));
    assert!(!name("DMM").same_as(&name("DMM2")));
}

fn taken_by<'a>(names: &'a [Name]) -> impl Fn(&Name) -> bool + 'a {
    move |candidate| names.iter().any(|n| n.same_as(candidate))
}

#[test]
fn auto_rename_keeps_a_free_name() {
    let taken = [name("PSU")];
    assert_eq!(
        auto_rename(&name("DMM"), taken_by(&taken), &limits()).as_str(),
        "DMM"
    );
}

#[test]
fn auto_rename_appends_first_free_suffix() {
    let taken = [name("DMM"), name("dmm_2")];
    assert_eq!(
        auto_rename(&name("DMM"), taken_by(&taken), &limits()).as_str(),
        "DMM_3"
    );
}

#[test]
fn auto_rename_truncates_base_to_fit() {
    let taken = [name("MultiMeter")];
    assert_eq!(
        auto_rename(&name("MultiMeter"), taken_by(&taken), &limits()).as_str(),
        "MultiMet_2"
    );
}

#[test]
fn auto_rename_truncates_more_for_two_digit_suffix() {
    let mut taken = vec![name("MultiMeter")];
    for n in 2..=9 {
        taken.push(name(&format!("MultiMet_{n}")));
    }
    assert_eq!(
        auto_rename(&name("MultiMeter"), taken_by(&taken), &limits()).as_str(),
        "MultiMe_10"
    );
}

#[test]
fn document_name_trims_outer_whitespace_only() {
    assert_eq!(
        DocumentName::parse("  My Station 1 ").unwrap().as_str(),
        "My Station 1"
    );
}

#[test]
fn document_name_rejects_empty() {
    assert_eq!(DocumentName::parse("   "), Err(DocumentNameError::Empty));
}

#[test]
fn document_name_rejects_forbidden_characters() {
    for c in ['<', '>', ':', '"', '/', '\\', '|', '?', '*'] {
        assert_eq!(
            DocumentName::parse(&format!("a{c}b")),
            Err(DocumentNameError::ForbiddenChar(c)),
            "character {c:?}"
        );
    }
}

#[test]
fn document_name_rejects_control_characters() {
    assert_eq!(
        DocumentName::parse("a\tb"),
        Err(DocumentNameError::ControlChar)
    );
}

#[test]
fn document_name_rejects_trailing_dot() {
    assert_eq!(
        DocumentName::parse("Station."),
        Err(DocumentNameError::TrailingDot)
    );
}

#[test]
fn document_name_rejects_reserved_names_case_insensitively() {
    for reserved in ["con", "Nul", "PRN", "aux", "COM1", "lpt9"] {
        assert_eq!(
            DocumentName::parse(reserved),
            Err(DocumentNameError::Reserved(reserved.to_string())),
            "{reserved}"
        );
    }
    for allowed in ["COM10", "CONSOLE", "COM0", "LPT"] {
        assert!(DocumentName::parse(allowed).is_ok(), "{allowed}");
    }
}

#[test]
fn document_name_length_limit_is_100() {
    assert!(DocumentName::parse(&"a".repeat(100)).is_ok());
    assert_eq!(
        DocumentName::parse(&"a".repeat(101)),
        Err(DocumentNameError::TooLong { max: 100 })
    );
}
