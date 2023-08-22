use mail_builder::MessageBuilder;
use mail_parser::Message;
use std::{io, path::PathBuf};
use thiserror::Error;

#[cfg(feature = "pgp")]
use crate::Pgp;
use crate::{header, FilterParts, MmlBodyInterpreter, Result};

#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot parse raw email")]
    ParseRawEmailError,
    #[error("cannot build email")]
    BuildEmailError(#[source] io::Error),
}

/// Represents the strategy used to display headers when interpreting
/// emails.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ShowHeadersStrategy {
    /// Transfers all available headers to the interpreted template.
    #[default]
    All,
    /// Transfers only specific headers to the interpreted template.
    Only(Vec<String>),
}

impl ShowHeadersStrategy {
    pub fn contains(&self, header: &String) -> bool {
        match self {
            Self::All => false,
            Self::Only(headers) => headers.contains(header),
        }
    }
}

/// The template interpreter interprets full emails as
/// [`crate::Tpl`]. The interpreter needs to be customized first. The
/// customization follows the builder pattern. When the interpreter is
/// customized, calling any function matching `interpret_*()` consumes
/// the interpreter and generates the final [`crate::Tpl`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MmlInterpreter {
    /// Defines the strategy to display headers.
    /// [`ShowHeadersStrategy::All`] transfers all the available
    /// headers to the interpreted template,
    /// [`ShowHeadersStrategy::Only`] only transfers the given headers
    /// to the interpreted template.
    show_headers: ShowHeadersStrategy,

    mml_body_interpreter: MmlBodyInterpreter,
}

impl MmlInterpreter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_show_headers(mut self, s: ShowHeadersStrategy) -> Self {
        self.show_headers = s;
        self
    }

    pub fn with_show_all_headers(mut self) -> Self {
        self.show_headers = ShowHeadersStrategy::All;
        self
    }

    pub fn with_show_only_headers(
        mut self,
        headers: impl IntoIterator<Item = impl ToString>,
    ) -> Self {
        let headers = headers.into_iter().fold(Vec::new(), |mut headers, header| {
            let header = header.to_string();
            if !headers.contains(&header) {
                headers.push(header)
            }
            headers
        });
        self.show_headers = ShowHeadersStrategy::Only(headers);
        self
    }

    pub fn with_show_additional_headers(
        mut self,
        headers: impl IntoIterator<Item = impl ToString>,
    ) -> Self {
        let next_headers = headers.into_iter().fold(Vec::new(), |mut headers, header| {
            let header = header.to_string();
            if !headers.contains(&header) && !self.show_headers.contains(&header) {
                headers.push(header)
            }
            headers
        });

        match &mut self.show_headers {
            ShowHeadersStrategy::All => {
                self.show_headers = ShowHeadersStrategy::Only(next_headers);
            }
            ShowHeadersStrategy::Only(headers) => {
                headers.extend(next_headers);
            }
        };

        self
    }

    pub fn with_hide_all_headers(mut self) -> Self {
        self.show_headers = ShowHeadersStrategy::Only(Vec::new());
        self
    }

    pub fn with_show_multiparts(mut self, b: bool) -> Self {
        self.mml_body_interpreter = self.mml_body_interpreter.show_multiparts(b);
        self
    }

    pub fn with_filter_parts(mut self, f: FilterParts) -> Self {
        self.mml_body_interpreter = self.mml_body_interpreter.filter_parts(f);
        self
    }

    pub fn with_show_plain_texts_signature(mut self, b: bool) -> Self {
        self.mml_body_interpreter = self.mml_body_interpreter.show_plain_texts_signature(b);
        self
    }

    pub fn with_show_attachments(mut self, b: bool) -> Self {
        self.mml_body_interpreter = self.mml_body_interpreter.show_attachments(b);
        self
    }

    pub fn with_show_inline_attachments(mut self, b: bool) -> Self {
        self.mml_body_interpreter = self.mml_body_interpreter.show_inline_attachments(b);
        self
    }

    pub fn with_save_attachments(mut self, b: bool) -> Self {
        self.mml_body_interpreter = self.mml_body_interpreter.save_attachments(b);
        self
    }

    pub fn with_save_attachments_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.mml_body_interpreter = self.mml_body_interpreter.save_attachments_dir(dir);
        self
    }

    #[cfg(feature = "pgp")]
    pub fn with_pgp(mut self, pgp: impl Into<Pgp>) -> Self {
        self.mml_body_interpreter = self.mml_body_interpreter.with_pgp(pgp.into());
        self
    }

    /// Interprets the given [`mail_parser::Message`] as a MML string.
    pub async fn interpret_msg(self, msg: &Message<'_>) -> Result<String> {
        let mut mml = String::new();

        match self.show_headers {
            ShowHeadersStrategy::All => msg.headers().iter().for_each(|header| {
                let key = header.name.as_str();
                let val = header::display_value(key, &header.value);
                mml.push_str(&format!("{key}: {val}\n"));
            }),
            ShowHeadersStrategy::Only(keys) => keys
                .iter()
                .filter_map(|key| msg.header(key).map(|val| (key, val)))
                .for_each(|(key, val)| {
                    let val = header::display_value(key, val);
                    mml.push_str(&format!("{key}: {val}\n"));
                }),
        };

        if !mml.is_empty() {
            mml.push_str("\n");
        }

        let mml_body_interpreter = self.mml_body_interpreter;

        #[cfg(feature = "pgp")]
        let mml_body_interpreter = mml_body_interpreter
            .with_pgp_sender(header::extract_first_email(msg.from()))
            .with_pgp_recipient(header::extract_first_email(msg.to()));

        let mml_body = mml_body_interpreter.interpret_msg(msg).await?;

        mml.push_str(mml_body.trim_end());
        mml.push('\n');

        Ok(mml)
    }

    /// Interprets the given bytes as a MML string.
    pub async fn interpret_bytes(self, bytes: impl AsRef<[u8]>) -> Result<String> {
        let msg = Message::parse(bytes.as_ref()).ok_or(Error::ParseRawEmailError)?;
        self.interpret_msg(&msg).await
    }

    /// Interprets the given [`mail_builder::MessageBuilder`] as a MML
    /// string.
    pub async fn interpret_msg_builder(self, builder: MessageBuilder<'_>) -> Result<String> {
        let bytes = builder.write_to_vec().map_err(Error::BuildEmailError)?;
        self.interpret_bytes(&bytes).await
    }
}

#[cfg(test)]
mod tests {
    use concat_with::concat_line;
    use mail_builder::MessageBuilder;

    use super::MmlInterpreter;

    fn msg_builder() -> MessageBuilder<'static> {
        MessageBuilder::new()
            .message_id("id@localhost")
            .in_reply_to("reply-id@localhost")
            .date(0 as u64)
            .from("from@localhost")
            .to("to@localhost")
            .subject("subject")
            .text_body("Hello, world!")
    }

    #[tokio::test]
    async fn all_headers() {
        let mml = MmlInterpreter::new()
            .with_show_all_headers()
            .interpret_msg_builder(msg_builder())
            .await
            .unwrap();

        let expected_mml = concat_line!(
            "Message-ID: <id@localhost>",
            "In-Reply-To: <reply-id@localhost>",
            "Date: Thu, 1 Jan 1970 00:00:00 +0000",
            "From: from@localhost",
            "To: to@localhost",
            "Subject: subject",
            "Content-Type: text/plain; charset=utf-8",
            "Content-Transfer-Encoding: 7bit",
            "",
            "Hello, world!",
            "",
        );

        assert_eq!(mml, expected_mml);
    }

    #[tokio::test]
    async fn only_headers() {
        let mml = MmlInterpreter::new()
            .with_show_only_headers(["From", "Subject"])
            .interpret_msg_builder(msg_builder())
            .await
            .unwrap();

        let expected_mml = concat_line!(
            "From: from@localhost",
            "Subject: subject",
            "",
            "Hello, world!",
            "",
        );

        assert_eq!(mml, expected_mml);
    }

    #[tokio::test]
    async fn only_headers_duplicated() {
        let mml = MmlInterpreter::new()
            .with_show_only_headers(["From", "Subject", "From"])
            .interpret_msg_builder(msg_builder())
            .await
            .unwrap();

        let expected_mml = concat_line!(
            "From: from@localhost",
            "Subject: subject",
            "",
            "Hello, world!",
            "",
        );

        assert_eq!(mml, expected_mml);
    }

    #[tokio::test]
    async fn no_headers() {
        let mml = MmlInterpreter::new()
            .with_hide_all_headers()
            .interpret_msg_builder(msg_builder())
            .await
            .unwrap();

        let expected_mml = concat_line!("Hello, world!", "");

        assert_eq!(mml, expected_mml);
    }

    #[tokio::test]
    async fn mml_markup_escaped() {
        let msg_builder = MessageBuilder::new()
            .message_id("id@localhost")
            .in_reply_to("reply-id@localhost")
            .date(0 as u64)
            .from("from@localhost")
            .to("to@localhost")
            .subject("subject")
            .text_body("<#part>Should be escaped.<#/part>");

        let mml = MmlInterpreter::new()
            .with_show_only_headers(["From", "Subject"])
            .interpret_msg_builder(msg_builder)
            .await
            .unwrap();

        let expected_mml = concat_line!(
            "From: from@localhost",
            "Subject: subject",
            "",
            "<#!part>Should be escaped.<#!/part>",
            "",
        );

        assert_eq!(mml, expected_mml);
    }
}