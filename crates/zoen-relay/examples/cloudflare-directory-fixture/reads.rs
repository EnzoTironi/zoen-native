use super::*;

pub(super) async fn timeline(f: &mut Fixture<'_>) -> Result<()> {
    f.now_ms = NOW_MS.into();
    let owner = author(100, 150);
    f.login("read-owner-login", "read-owner", &owner, None)
        .await?;
    f.request(
        "read-owner-register",
        "read-owner",
        Op::Register {
            profile: profile(&owner, "read_owner"),
        },
        None,
    )
    .await?;
    let mut people = vec![owner.clone()];
    for index in 0..24u8 {
        let person = author(101 + index, 151 + index);
        let actor = format!("read-person-{index}");
        f.login(&format!("{actor}-login"), &actor, &person, None)
            .await?;
        f.request(
            &format!("{actor}-register"),
            &actor,
            Op::Register {
                profile: profile(&person, &format!("read_{index:02}")),
            },
            None,
        )
        .await?;
        people.push(person);
    }
    for (index, device) in [201, 202].into_iter().enumerate() {
        let linked = author(100, device);
        f.request(
            &format!("read-link-{index}"),
            "read-owner",
            Op::DeliverLink {
                id: format!("{:064x}", index + 1000),
                sealed: vec![1, 2, 3],
                device: Some(certificate(&linked)?),
            },
            None,
        )
        .await?;
    }
    let linked = author(100, 201);
    f.login("read-linked-login", "read-linked", &linked, None)
        .await?;
    f.request(
        "read-unlink",
        "read-owner",
        Op::Unlink {
            device: certificate(&linked)?.device,
        },
        None,
    )
    .await?;
    for (label, op) in [
        (
            "read-exact-normalized",
            Op::Lookup {
                handle: "  @@@READ_02  ".into(),
                prefix: false,
            },
        ),
        (
            "read-prefix-limited",
            Op::Lookup {
                handle: "read_".into(),
                prefix: true,
            },
        ),
        (
            "read-prefix-percent",
            Op::Lookup {
                handle: "read%_".into(),
                prefix: true,
            },
        ),
        (
            "read-prefix-backslash",
            Op::Lookup {
                handle: "read\\_".into(),
                prefix: true,
            },
        ),
        (
            "read-empty-handle",
            Op::Lookup {
                handle: " @@ ".into(),
                prefix: true,
            },
        ),
        (
            "read-exact-percent",
            Op::Lookup {
                handle: "read%_02".into(),
                prefix: false,
            },
        ),
        ("read-devices", Op::Devices),
        ("read-profiles-empty", Op::Profiles { ids: vec![] }),
        (
            "read-profiles-missing",
            Op::Profiles {
                ids: vec!["missing".into()],
            },
        ),
        (
            "read-profiles-mixed",
            Op::Profiles {
                ids: vec![
                    people[0].identity.clone(),
                    people[3].identity.clone(),
                    "missing".into(),
                    people[0].identity.clone(),
                ],
            },
        ),
        (
            "read-profiles-first500",
            Op::Profiles {
                ids: (0..500)
                    .map(|_| people[0].identity.clone())
                    .chain([people[1].identity.clone()])
                    .collect(),
            },
        ),
        (
            "read-profiles-500-distinct",
            Op::Profiles {
                ids: (0..500)
                    .map(|index| format!("missing-{index}"))
                    .chain([people[0].identity.clone()])
                    .collect(),
            },
        ),
    ] {
        let unordered = matches!(&op, Op::Profiles { .. });
        f.request(label, "read-owner", op, None).await?;
        if unordered {
            f.steps.last_mut().context("missing read step")?["unordered_profiles"] = json!(true);
        }
    }
    f.request(
        "read-revoked-caller",
        "read-linked",
        Op::Devices,
        Some("this device was unlinked"),
    )
    .await?;
    let provisional = author(203, 203);
    f.login(
        "read-provisional-login",
        "read-provisional",
        &provisional,
        None,
    )
    .await?;
    f.request(
        "read-provisional-devices",
        "read-provisional",
        Op::Devices,
        Some("register first"),
    )
    .await?;
    f.request(
        "read-provisional-profiles",
        "read-provisional",
        Op::Profiles {
            ids: vec![owner.identity.clone()],
        },
        Some("register first"),
    )
    .await?;
    Ok(())
}
