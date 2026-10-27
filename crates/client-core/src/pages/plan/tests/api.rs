use super::*;
use crate::pages::session::{DocumentEdit, DocumentView};
use crate::pages::TextSpanDto;
use documents::plan::{PlanBlockContent, PlanBlockEdit};
use documents::{Kind, Span};

#[test]
fn typed_client_read_preserves_duplicate_line_ids_and_exact_legacy_text() {
    let fixture = Fixture::new().unwrap();
    let pending = fixture.engine.store.outbox().unwrap().len();
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(read.ready);
    assert!(!read.unsaved);
    assert_eq!(read.version, 1);
    assert_eq!(read.total_cents, Some(193));
    assert_eq!(read.blocks[0].text, original().title);
    assert_eq!(read.blocks[1].text, original().summary);
    let lines: Vec<_> = read
        .blocks
        .iter()
        .filter_map(|block| {
            if let PlanPageContentDto::Line {
                legacy_id,
                cost_cents,
                done,
            } = &block.content
            {
                Some((&block.id, legacy_id, &block.text, *cost_cents, *done))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(lines.len(), 2);
    assert_ne!(lines[0].0, lines[1].0);
    assert_eq!(lines[0].1, lines[1].1);
    assert_eq!(lines[0].1, &original().sections[0].lines[0].id);
    assert_eq!(lines[0].2, "Hotel\r\nreservation");
    assert_eq!((lines[0].3, lines[0].4), (200, false));
    assert_eq!((lines[1].3, lines[1].4), (-7, true));
    assert!(fixture.engine.page(&fixture.item).is_err());
    assert_eq!(fixture.engine.store.outbox().unwrap().len(), pending);
}

#[test]
fn typed_client_read_keeps_all_prose_metadata_and_utf16_marks_after_cutover() {
    let mut fixture = Fixture::new().unwrap();
    let mut document = fixture.fold().document.unwrap();
    let kinds = vec![
        Kind::Paragraph,
        Kind::Heading {
            level: 2,
            setext: true,
        },
        Kind::Bullet { indent: 3 },
        Kind::Numbered {
            indent: 2,
            number: 9,
        },
        Kind::Task {
            indent: 1,
            checked: true,
        },
        Kind::Quote,
        Kind::Code {
            lang: "rust".into(),
        },
        Kind::Divider,
        Kind::Image {
            url: "https://example.com/image.png".into(),
            alt: "cafe\u{301} 👩🏽‍💻".into(),
            title: "Exact image title".into(),
        },
        Kind::Raw,
    ];
    let text = "cafe\u{301} 👩🏽‍💻\r\nsecond\u{2028}line";
    let mut order: Vec<_> = document
        .blocks()
        .unwrap()
        .into_iter()
        .map(|b| b.id)
        .collect();
    let edits: Vec<_> = kinds
        .iter()
        .map(|kind| {
            let id = new_id("prose").unwrap();
            order.push(id.clone());
            PlanBlockEdit {
                id,
                content: PlanBlockContent::Prose { kind: kind.clone() },
                text: text.into(),
                spans: vec![Span {
                    start: 0,
                    end: text.encode_utf16().count() as u32,
                    key: "b".into(),
                    value: String::new(),
                }],
            }
        })
        .collect();
    document
        .apply_from(&document.revision().unwrap(), &order, &edits)
        .unwrap();
    fixture
        .ordered(fixture.version(ItemContent::Page(page(
            fixture.fold().manifest().unwrap(),
            &document,
        ))))
        .unwrap();
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert_eq!(read.version, 2);
    assert!(read.ready);
    assert!(!read.unsaved);
    let prose = &read.blocks[read.blocks.len() - kinds.len()..];
    for (index, block) in prose.iter().enumerate() {
        assert_eq!(block.id, edits[index].id);
        assert_eq!(block.text, text);
        assert_eq!(
            block.spans,
            vec![TextSpanDto {
                start: 0,
                end: text.encode_utf16().count() as u32,
                key: "b".into(),
                value: String::new()
            }]
        );
    }
    assert_eq!(
        prose[1].content,
        PlanPageContentDto::Prose {
            kind: PlanProseKindDto::Heading {
                level: 2,
                setext: true
            }
        }
    );
    assert_eq!(
        prose[8].content,
        PlanPageContentDto::Prose {
            kind: PlanProseKindDto::Image {
                url: "https://example.com/image.png".into(),
                alt: "cafe\u{301} 👩🏽‍💻".into(),
                title: "Exact image title".into()
            }
        }
    );
    assert_eq!(
        prose[3].content,
        PlanPageContentDto::Prose {
            kind: PlanProseKindDto::Numbered {
                indent: 2,
                number: 9
            }
        }
    );
    assert_eq!(
        prose[4].content,
        PlanPageContentDto::Prose {
            kind: PlanProseKindDto::Task {
                indent: 1,
                checked: true
            }
        }
    );
    assert_eq!(
        prose[6].content,
        PlanPageContentDto::Prose {
            kind: PlanProseKindDto::Code {
                lang: "rust".into()
            }
        }
    );
    assert_eq!(
        fixture.engine.plan_snapshot(&fixture.item).unwrap().version,
        2
    );
}

#[test]
fn typed_client_read_labels_private_draft_without_leaking_it_into_shared_cards(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new().unwrap();
    let current = fixture.engine.plan_page(&fixture.item).unwrap();
    let pending = fixture.engine.store.outbox().unwrap().len();
    fixture
        .engine
        .with_document(&fixture.item, ItemKind::Plan, |_, session| {
            let read = session.read(&fixture.engine)?;
            let DocumentView::Plan { blocks } = &read.view else {
                panic!("typed Plan required")
            };
            let block = blocks
                .iter()
                .find(|b| {
                    matches!(
                        b.content,
                        PlanBlockContent::Line {
                            cost_cents: 200,
                            ..
                        }
                    )
                })
                .unwrap();
            let edit = PlanBlockEdit {
                id: block.id.clone(),
                content: block.content.clone(),
                text: "Private unsaved cafe\u{301} 👩🏽‍💻".into(),
                spans: vec![],
            };
            session.apply_from(
                &fixture.engine,
                "private-plan-reader-test",
                &read.edit_context,
                DocumentEdit::Plan {
                    order: &blocks.iter().map(|b| b.id.clone()).collect::<Vec<_>>(),
                    changed: &[edit],
                },
            )?;
            Ok(())
        })
        .unwrap();
    let private = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(private.unsaved);
    assert_eq!(private.version, current.version);
    assert_eq!(private.content_hash, current.content_hash);
    assert!(private
        .blocks
        .iter()
        .any(|b| b.text.starts_with("Private unsaved")));
    let shared = fixture.engine.plan_snapshot(&fixture.item).unwrap();
    assert_eq!(shared.document, original());
    assert!(!shared.text.contains("Private unsaved"));
    assert_eq!(fixture.engine.store.outbox().unwrap().len(), pending);

    Ok(())
}

#[test]
fn typed_client_read_returns_exact_extreme_cents_even_when_total_is_unrepresentable() {
    let mut fixture = Fixture::new().unwrap();
    let mut plan = original();
    plan.budget_cents = Some(i64::MIN);
    plan.sections[0].lines[0].cost_cents = i64::MAX;
    plan.sections[0].lines[1].cost_cents = i64::MAX;
    fixture
        .ordered(fixture.version(ItemContent::Plan(plan)))
        .unwrap();
    let read = fixture.engine.plan_page(&fixture.item).unwrap();
    assert_eq!(read.version, 2);
    assert_eq!(read.total_cents, None);
    assert_eq!(
        read.blocks[0].content,
        PlanPageContentDto::Title {
            budget_cents: Some(i64::MIN)
        }
    );
    assert_eq!(
        read.blocks
            .iter()
            .filter(|b| matches!(
                b.content,
                PlanPageContentDto::Line {
                    cost_cents: i64::MAX,
                    ..
                }
            ))
            .count(),
        2
    );
    assert!(fixture.engine.item(&fixture.item).is_err());
    let mut credit = original();
    credit.sections[0].lines[0].cost_cents = i64::MAX;
    credit.sections[0].lines[1].cost_cents = 1;
    credit.sections[0].lines.push(PlanLine {
        id: "credit".into(),
        text: "Exact credit".into(),
        cost_cents: -1,
        done: false,
    });
    fixture
        .ordered(fixture.version(ItemContent::Plan(credit.clone())))
        .unwrap();
    assert_eq!(
        fixture.engine.plan_page(&fixture.item).unwrap().total_cents,
        Some(i64::MAX)
    );
    credit.sections[0].lines.reverse();
    fixture
        .ordered(fixture.version(ItemContent::Plan(credit)))
        .unwrap();
    assert_eq!(
        fixture.engine.plan_page(&fixture.item).unwrap().total_cents,
        Some(i64::MAX)
    );
    let page = fixture
        .engine
        .page_create(&fixture.space, "Ordinary Page")
        .unwrap();
    assert!(fixture.engine.plan_page(&page).is_err());
}
