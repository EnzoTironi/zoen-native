use bytes::Bytes;
use futures_util::{Stream, StreamExt};

use super::Error;

pub(super) async fn bounded_bytes(
    mut stream: impl Stream<Item = object_store::Result<Bytes>> + Unpin,
    length: u64,
) -> Result<Bytes, Error> {
    if !(1..=protocol::sites::MAX_HTML_BYTES).contains(&length) {
        return Err(Error::Unavailable);
    }
    let mut bytes = Vec::with_capacity(length as usize);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| Error::Unavailable)?;
        if bytes.len().saturating_add(chunk.len()) as u64 > length {
            return Err(Error::Unavailable);
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != length {
        return Err(Error::Unavailable);
    }
    Ok(bytes.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn storage_metadata_does_not_authorize_extra_stream_bytes() {
        let polls = Arc::new(AtomicUsize::new(0));
        let observed = polls.clone();
        let chunks = stream::iter([
            Ok(Bytes::from_static(b"1234")),
            Ok(Bytes::from_static(b"never")),
        ])
        .inspect(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(bounded_bytes(chunks, 3).await, Err(Error::Unavailable));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn truncated_and_invalid_lengths_fail_but_exact_split_bytes_survive() {
        assert_eq!(
            bounded_bytes(stream::iter([Ok(Bytes::from_static(b"12"))]), 3).await,
            Err(Error::Unavailable)
        );
        for length in [0, protocol::sites::MAX_HTML_BYTES + 1, u64::MAX] {
            assert_eq!(
                bounded_bytes(stream::empty(), length).await,
                Err(Error::Unavailable)
            );
        }
        assert_eq!(
            bounded_bytes(
                stream::iter([Ok(Bytes::from_static(b"1")), Ok(Bytes::from_static(b"23"))]),
                3
            )
            .await
            .unwrap(),
            Bytes::from_static(b"123")
        );
    }
}
