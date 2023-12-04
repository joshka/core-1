#[cfg(feature = "imap")]
#[cfg(feature = "smtp")]
#[tokio::test(flavor = "multi_thread")]
async fn test_smtp_features() {
    use email::{
        account::{AccountConfig, PasswdConfig},
        backend::BackendBuilder,
        envelope::list::imap::ListEnvelopesImap,
        folder::purge::imap::PurgeFolderImap,
        imap::{ImapAuthConfig, ImapConfig, ImapSessionBuilder},
        message::send_raw::smtp::SendRawMessageSmtp,
        smtp::{SmtpAuthConfig, SmtpClientBuilder, SmtpConfig},
    };
    use mail_builder::MessageBuilder;
    use secret::Secret;
    use std::time::Duration;

    env_logger::builder().is_test(true).init();

    let account_config = AccountConfig::default();
    let imap_config = ImapConfig {
        host: "localhost".into(),
        port: 3143,
        ssl: Some(false),
        starttls: Some(false),
        insecure: Some(true),
        login: "bob@localhost".into(),
        auth: ImapAuthConfig::Passwd(PasswdConfig {
            passwd: Secret::new_raw("echo 'password'"),
        }),
        ..Default::default()
    };
    let smtp_config = SmtpConfig {
        host: "localhost".into(),
        port: 3025,
        ssl: Some(false),
        starttls: Some(false),
        insecure: Some(true),
        login: "alice@localhost".into(),
        auth: SmtpAuthConfig::Passwd(PasswdConfig {
            passwd: Secret::new_raw("password"),
        }),
        ..Default::default()
    };

    let imap_ctx = ImapSessionBuilder::new(account_config.clone(), imap_config);
    let smtp_ctx = SmtpClientBuilder::new(account_config.clone(), smtp_config);
    let backend_builder = BackendBuilder::new(account_config.clone(), (imap_ctx, smtp_ctx))
        .with_purge_folder(|ctx| PurgeFolderImap::new(&ctx.0))
        .with_list_envelopes(|ctx| ListEnvelopesImap::new(&ctx.0))
        .with_send_raw_message(|ctx| SendRawMessageSmtp::new(&ctx.1));
    let backend = backend_builder.build().await.unwrap();

    // setting up folders

    backend.purge_folder("INBOX").await.unwrap();

    // checking that an email can be built and sent

    let raw_msg = MessageBuilder::new()
        .from("alice@localhost")
        .to("bob@localhost")
        .subject("Plain message!")
        .text_body("Plain message!")
        .write_to_vec()
        .unwrap();
    backend.send_raw_message(&raw_msg).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // checking that the envelope of the sent email exists

    let envelopes = backend.list_envelopes("INBOX", 10, 0).await.unwrap();
    assert_eq!(1, envelopes.len());
    let envelope = envelopes.first().unwrap();
    assert_eq!("alice@localhost", envelope.from.addr);
    assert_eq!("Plain message!", envelope.subject);
}
