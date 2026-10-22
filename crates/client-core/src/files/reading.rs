//! Reading operations on the original Engine state.

use super::*;

impl Engine {
    pub fn file_bytes(&self, item: &str, version: Option<u32>) -> R<Option<Vec<u8>>> {
        self.file_bytes_bounded(item, version, MAX_FILE_BYTES as u64)
    }

    pub(super) fn file_doc_at(&self, item: &str, version: Option<u32>) -> R<&FileDoc> {
        let it = self
            .state
            .items
            .get(item)
            .ok_or_else(|| CoreError::NotFound {
                what: "Item".into(),
            })?;
        if matches!(&it.current().content, ItemContent::App(_)) {
            self.app_document_identity(&it.space)?;
        }
        let v = match version {
            // Most demand workers and recent-source checks pin the current
            // number. Resolving it must not walk every earlier File manifest.
            Some(n) if it.current().number == n => Some(it.current()),
            Some(n) => it.versions.iter().find(|v| {
                #[cfg(test)]
                FILE_VERSION_HISTORY_PROBES.with(|count| count.set(count.get() + 1));
                v.number == n
            }),
            None => it.versions.last(),
        };
        let Some(f) = v.and_then(|v| v.content.owned_file()) else {
            return Err(invalid(
                "este Item não é um arquivo",
                "this Item is not a file",
            ));
        };
        Ok(f)
    }

    pub fn file_info_at(&self, item: &str, version: Option<u32>) -> R<FileDto> {
        let file = self.file_doc_at(item, version)?;
        validate_file(file, MAX_FILE_BYTES as u64)?;
        Ok(self.file_dto(file))
    }

    pub(super) fn file_chunk_bytes(
        &self,
        chunk: &ChunkRef,
        max_bytes: usize,
    ) -> R<Option<Vec<u8>>> {
        let expected = validate_chunk(chunk, max_bytes)?;
        let bytes = match self.store.media_bytes_bounded(&chunk.sha256, expected) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Ok(None),
            Err(store::StoreError::MediaTooLarge) => return Err(invalid_file()),
            Err(err) => return Err(err.into()),
        };
        if bytes.len() != expected || hex::encode(Sha256::digest(&bytes)) != chunk.sha256 {
            return Err(invalid_file());
        }
        Ok(Some(bytes))
    }

    pub fn file_bytes_bounded(
        &self,
        item: &str,
        version: Option<u32>,
        max_bytes: u64,
    ) -> R<Option<Vec<u8>>> {
        let f = self.file_doc_at(item, version)?;
        self.owned_file_bytes(f, max_bytes)
    }

    pub(crate) fn owned_file_bytes(&self, f: &FileDoc, max_bytes: u64) -> R<Option<Vec<u8>>> {
        let size = validate_file(f, max_bytes)?;
        // Missing or oversized cached pieces cannot trigger a file-sized allocation.
        for chunk in &f.chunks {
            match self.store.media_bytes_len(&chunk.sha256)? {
                None => return Ok(None),
                Some(len) if len == chunk.bytes => {}
                Some(_) => return Err(invalid_file()),
            }
        }
        let mut out = Vec::new();
        out.try_reserve_exact(size).map_err(|_| invalid_file())?;
        for c in &f.chunks {
            match self.file_chunk_bytes(c, MAX_CHUNK)? {
                Some(b) => out.extend_from_slice(&b),
                None => return Ok(None),
            }
        }
        if out.len() != size || hex::encode(Sha256::digest(&out)) != f.sha256 {
            return Err(invalid_file());
        }
        Ok(Some(out))
    }

    pub fn file_thumbnail(&self, item: &str) -> R<Option<Vec<u8>>> {
        let f = self.file_doc_at(item, None)?;
        validate_file(f, MAX_FILE_BYTES as u64)?;
        match &f.thumb {
            Some(t) => self.file_chunk_bytes(t, MAX_THUMBNAIL_BYTES),
            None => Ok(None),
        }
    }

    pub(crate) fn file_dto(&self, f: &FileDoc) -> FileDto {
        let here = f
            .chunks
            .iter()
            .filter(|c| self.store.media_bytes_len(&c.sha256).ok().flatten() == Some(c.bytes))
            .count() as u32;
        FileDto {
            name: f.name.clone(),
            path: f.path.clone(),
            mime: f.mime.clone(),
            bytes: f.bytes,
            sha256: f.sha256.clone(),
            ready: here as usize == f.chunks.len(),
            chunks: f.chunks.len() as u32,
            chunks_here: here,
            has_thumbnail: f.thumb.is_some(),
        }
    }
}
