//! limits cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn legacy_line_addition_refuses_negative_cost_without_publishing_zero() {
    let mut fixture = Fixture::new().unwrap();
    let before = storage(&fixture);
    for cents in [-1, -1_025, i64::MIN] {
        assert!(matches!(
            {
                let observed_hash = fixture
                    .engine
                    .plan_page(&fixture.item)
                    .unwrap()
                    .content_hash;
                fixture
                    .engine
                    .add_plan_line(&fixture.item, 0, "Credit", cents, &observed_hash)
            },
            Err(CoreError::Invalid { .. })
        ));
        assert_eq!(storage(&fixture), before);
        assert_eq!(fixture.engine.item(&fixture.item).unwrap().version, 1);
    }
    for cents in [0, 1_025] {
        let outcome = {
            let observed_hash = fixture
                .engine
                .plan_page(&fixture.item)
                .unwrap()
                .content_hash;
            fixture
                .engine
                .add_plan_line(&fixture.item, 0, "Exact cost", cents, &observed_hash)
        }
        .unwrap();
        let plan = outcome.item.plan.unwrap();
        let added = plan.sections[0].lines.last().unwrap();
        assert_eq!(added.text, "Exact cost");
        assert_eq!(added.cost_cents, cents);
        assert_eq!(plan.sections[0].lines[1].cost_cents, -7);
    }
}

#[test]
pub(super) fn legacy_line_addition_refuses_an_unrepresentable_total_before_writing() {
    let mut fixture = Fixture::new().unwrap();
    let before = storage(&fixture);
    assert!(matches!(
        {
            let observed_hash = fixture
                .engine
                .plan_page(&fixture.item)
                .unwrap()
                .content_hash;
            fixture
                .engine
                .add_plan_line(&fixture.item, 0, "Too much", i64::MAX, &observed_hash)
        },
        Err(CoreError::Invalid { .. })
    ));
    assert_eq!(storage(&fixture), before);
    let outcome = {
        let observed_hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        fixture.engine.add_plan_line(
            &fixture.item,
            0,
            "Exact upper total",
            i64::MAX - 193,
            &observed_hash,
        )
    }
    .unwrap();
    assert_eq!(outcome.item.plan.unwrap().total_cents, i64::MAX);
}

#[test]
pub(super) fn total_overflow_is_unavailable_and_minimum_legacy_cents_stay_readable() {
    let mut fixture = Fixture::new().unwrap();
    let mut original = original();
    original.sections[0].lines[0].cost_cents = i64::MAX;
    original.sections[0].lines[1].cost_cents = 1;
    fixture
        .ordered(fixture.version(ItemContent::Plan(original.clone())))
        .unwrap();
    let before = storage(&fixture);
    let card = fixture.engine.card(&fixture.item).unwrap();
    assert_eq!(card.total_cents, None);
    assert_eq!(card.line_count, 2);
    assert!(fixture.engine.item(&fixture.item).is_err());
    assert_eq!(storage(&fixture), before);
    let document = fixture.fold().document.unwrap();
    promote(&mut fixture, &document).unwrap();
    assert_eq!(
        fixture.engine.card(&fixture.item).unwrap().total_cents,
        None
    );
    assert!(fixture.engine.item(&fixture.item).is_err());
    let mut legacy = Fixture::new().unwrap();
    original.sections[0].lines[0].cost_cents = i64::MIN;
    original.sections[0].lines[1].cost_cents = 0;
    legacy
        .ordered(legacy.version(ItemContent::Plan(original)))
        .unwrap();
    assert_eq!(
        legacy
            .engine
            .item(&legacy.item)
            .unwrap()
            .plan
            .unwrap()
            .total_cents,
        i64::MIN
    );
    assert_eq!(
        legacy.engine.card(&legacy.item).unwrap().total_cents,
        Some(i64::MIN)
    );
    assert_eq!(legacy.engine.search("Hotel").len(), 1);
    assert_eq!(
        crate::i18n::money_in(crate::i18n::Lang::En, i64::MIN),
        "−R$ 92,233,720,368,547,758.08"
    );
}

#[test]
pub(super) fn credits_cancel_intermediate_overflow_independently_of_row_order() {
    for (costs, total) in [
        ([i64::MAX, 1, -1], i64::MAX),
        ([-1, 1, i64::MAX], i64::MAX),
        ([i64::MIN, -1, 1], i64::MIN),
        ([1, -1, i64::MIN], i64::MIN),
    ] {
        let mut fixture = Fixture::new().unwrap();
        let mut legacy = original();
        let template = legacy.sections[0].lines[0].clone();
        legacy.sections[0].lines = costs
            .iter()
            .enumerate()
            .map(|(index, cost)| PlanLine {
                id: format!("cancel-credit-{index}"),
                text: format!("Exact credit row {index}"),
                cost_cents: *cost,
                ..template.clone()
            })
            .collect();
        fixture
            .ordered(fixture.version(ItemContent::Plan(legacy)))
            .unwrap();
        let document = fixture.fold().document.unwrap();
        assert_eq!(
            document.view().unwrap().total,
            documents::plan::TotalCents::Cents(total)
        );
        let before = storage(&fixture);
        let card = fixture.engine.card(&fixture.item).unwrap();
        assert_eq!(card.total_cents, Some(total));
        assert_eq!(card.line_count, 3);
        let item = fixture.engine.item(&fixture.item).unwrap();
        assert_eq!(item.plan.as_ref().unwrap().total_cents, total);
        assert_eq!(
            item.plan.unwrap().sections[0]
                .lines
                .iter()
                .map(|line| line.cost_cents)
                .collect::<Vec<_>>(),
            costs
        );
        assert_eq!(storage(&fixture), before);
        promote(&mut fixture, &document).unwrap();
        assert_eq!(
            fixture.engine.card(&fixture.item).unwrap().total_cents,
            Some(total)
        );
        assert_eq!(
            fixture
                .engine
                .item(&fixture.item)
                .unwrap()
                .plan
                .unwrap()
                .total_cents,
            total
        );
        fixture.engine.reload().unwrap();
        assert_eq!(
            fixture.engine.card(&fixture.item).unwrap().total_cents,
            Some(total)
        );
    }
}
