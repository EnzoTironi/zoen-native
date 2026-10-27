use super::{recovery_key_with_random, CoreError};

#[test]
fn entropy_failure_never_returns_a_partial_recovery_key() {
    let mut calls = 0;
    let result = recovery_key_with_random(|| {
        calls += 1;
        if calls == 1 {
            Ok([7; 32])
        } else {
            Err(CoreError::Storage {
                message: "injected entropy failure".into(),
            })
        }
    });
    assert!(matches!(result, Err(CoreError::Storage { .. })));
    assert_eq!(calls, 2);
}

#[test]
fn recovery_digits_reject_biased_bytes_and_keep_the_portable_group_format(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut calls = 0;
    let key = recovery_key_with_random(|| {
        calls += 1;
        Ok(match calls {
            1 => [250; 32],
            2 => [7; 32],
            _ => [8; 32],
        })
    })?;
    let groups: Vec<_> = key.split(' ').collect();
    assert_eq!(groups, [vec!["7777"; 8], vec!["8888"; 8]].concat());
    assert_eq!(key.len(), 79);
    assert_eq!(calls, 3);
    Ok(())
}
