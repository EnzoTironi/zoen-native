//! Ordinary file imports retain Core's membership, encryption and publication.
//! The optional MIME is metadata, never an App installation or grant.
use client_core::{Client, CoreError, ItemDetail};
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Input {
    pub(crate) chat: String,
    pub(crate) file: String,
    pub(crate) thumbnail: Option<String>,
    path: String,
    name: String,
    mime: String,
}

impl Input {
    /// Arguments after `file add`; named options may surround the two operands.
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut flags = BTreeSet::new();
        let mut operands = Vec::new();
        let (mut mime, mut directory, mut thumbnail) = (None, None, None);
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--mime" | "--path" | "--thumb" => {
                    if !flags.insert(arg) {
                        return Err("file_add_duplicate_option".into());
                    }
                    let value = args.next().ok_or("file_add_missing_option_value")?;
                    if (value.is_empty() && arg != "--path") || value.starts_with("--") {
                        return Err("file_add_missing_option_value".into());
                    }
                    match arg.as_str() {
                        "--mime" => {
                            let essence = value.split(';').next().unwrap_or_default();
                            if value.chars().any(char::is_control)
                                || value.trim() != value.as_str()
                                || !essence.split_once('/').is_some_and(|(kind, subtype)| {
                                    !kind.is_empty()
                                        && !subtype.is_empty()
                                        && !essence.chars().any(char::is_whitespace)
                                })
                            {
                                return Err("file_add_invalid_mime".into());
                            }
                            mime = Some(value.clone());
                        }
                        "--path" => directory = Some(value.clone()),
                        "--thumb" => thumbnail = Some(value.clone()),
                        _ => return Err("file_add_invalid_option".into()),
                    }
                }
                _ if arg.starts_with("--") => return Err("file_add_invalid_option".into()),
                _ => operands.push(arg),
            }
        }
        let [chat, file] = operands.as_slice() else {
            return Err("file_add_requires_chat_and_file".into());
        };
        if chat.is_empty() || file.is_empty() {
            return Err("file_add_requires_chat_and_file".into());
        }
        let name = Path::new(file)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| (*file).clone());
        let path = directory.filter(|path| !path.is_empty()).map_or_else(
            || name.clone(),
            |path| format!("{}/{name}", path.trim_end_matches('/')),
        );
        Ok(Self {
            chat: (*chat).clone(),
            file: (*file).clone(),
            thumbnail,
            path,
            name,
            mime: mime.unwrap_or_else(|| infer(file).into()),
        })
    }

    pub(crate) fn add(
        &self,
        client: &Client,
        space: String,
        bytes: Vec<u8>,
        thumbnail: Option<Vec<u8>>,
    ) -> Result<ItemDetail, CoreError> {
        client.file_add(
            space,
            self.path.clone(),
            self.name.clone(),
            self.mime.clone(),
            bytes,
            thumbnail,
        )
    }
}

/// Validate before main opens a home, vault or network session. Other command
/// operands remain opaque: a message containing `--mime` is still ordinary text.
pub(crate) fn preflight(command: &str, args: &[String]) -> Result<(), String> {
    if command == "--mime"
        || command.starts_with("--mime=")
        || (command.starts_with('-')
            && args
                .windows(2)
                .any(|pair| pair[0] == "file" && pair[1] == "add"))
    {
        return Err("file_add_invalid_option".into());
    }
    if command == "file" {
        match args.first().map(String::as_str) {
            Some("add") => {
                Input::parse(&args[1..])?;
            }
            Some(flag) if flag == "--mime" || flag.starts_with("--mime=") => {
                return Err("file_add_invalid_option".into());
            }
            Some(flag) if flag.starts_with('-') && args.iter().any(|arg| arg == "add") => {
                return Err("file_add_invalid_option".into());
            }
            _ => {}
        }
    }
    Ok(())
}

fn infer(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => "text/markdown",
        "txt" => "text/plain",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "heic" => "image/heic",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "zip" => "application/zip",
        "csv" => "text/csv",
        "json" => "application/json",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests;
