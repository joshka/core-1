use concat_with::concat_line;
use email::{
    account::config::AccountConfig,
    backend::BackendBuilder,
    envelope::{list::maildir::ListMaildirEnvelopes, Id},
    flag::{
        add::maildir::AddMaildirFlags, remove::maildir::RemoveMaildirFlags,
        set::maildir::SetMaildirFlags, Flag,
    },
    folder::{
        add::maildir::AddMaildirFolder, config::FolderConfig, delete::maildir::DeleteFolderMaildir,
        expunge::maildir::ExpungeMaildirFolder, list::maildir::ListMaildirFolders,
    },
    maildir::{config::MaildirConfig, MaildirContextBuilder},
    message::{
        add::maildir::AddMaildirMessage, copy::maildir::CopyMaildirMessages,
        move_::maildir::MoveMaildirMessages, peek::maildir::PeekMaildirMessages,
    },
};
use mail_builder::MessageBuilder;
use maildirpp::Maildir;
use std::{collections::HashMap, fs, iter::FromIterator};
use tempfile::tempdir;

#[tokio::test]
async fn test_maildir_features() {
    env_logger::builder().is_test(true).init();

    // set up maildir folders

    let mdir: Maildir = tempdir().unwrap().path().to_owned().into();
    if let Err(_) = fs::remove_dir_all(mdir.path()) {}
    mdir.create_dirs().unwrap();

    let mdir_sub: Maildir = mdir.path().join(".Subdir").into();
    if let Err(_) = fs::remove_dir_all(mdir_sub.path()) {}
    mdir_sub.create_dirs().unwrap();

    let mdir_trash = Maildir::from(mdir.path().join(".Trash"));
    if let Err(_) = fs::remove_dir_all(mdir_trash.path()) {}
    mdir_trash.create_dirs().unwrap();

    let config = AccountConfig {
        name: "account".into(),
        folder: Some(FolderConfig {
            aliases: Some(HashMap::from_iter([
                ("subdir".into(), "Subdir".into()),
                (
                    "abs-subdir".into(),
                    mdir.path().join(".Subdir").to_string_lossy().into(),
                ),
            ])),
            ..Default::default()
        }),
        ..Default::default()
    };

    // Main maildir backend

    let mdir_path = mdir.path().to_owned();
    let mdir_config = MaildirConfig {
        root_dir: mdir_path.clone(),
    };
    let backend_ctx = MaildirContextBuilder::new(config.clone(), mdir_config);
    let backend_builder = BackendBuilder::new(config.clone(), backend_ctx)
        .with_add_folder(|ctx| Some(AddMaildirFolder::new_boxed(ctx.clone())))
        .with_list_folders(|ctx| Some(ListMaildirFolders::new_boxed(ctx.clone())))
        .with_expunge_folder(|ctx| Some(ExpungeMaildirFolder::new_boxed(ctx.clone())))
        .with_delete_folder(|ctx| Some(DeleteFolderMaildir::new_boxed(ctx.clone())))
        .with_list_envelopes(|ctx| Some(ListMaildirEnvelopes::new_boxed(ctx.clone())))
        .with_add_flags(|ctx| Some(AddMaildirFlags::new_boxed(ctx.clone())))
        .with_set_flags(|ctx| Some(SetMaildirFlags::new_boxed(ctx.clone())))
        .with_remove_flags(|ctx| Some(RemoveMaildirFlags::new_boxed(ctx.clone())))
        .with_add_message(|ctx| Some(AddMaildirMessage::new_boxed(ctx.clone())))
        .with_peek_messages(|ctx| Some(PeekMaildirMessages::new_boxed(ctx.clone())))
        .with_copy_messages(|ctx| Some(CopyMaildirMessages::new_boxed(ctx.clone())))
        .with_move_messages(|ctx| Some(MoveMaildirMessages::new_boxed(ctx.clone())));
    let mdir = backend_builder.build().await.unwrap();

    // Sub maildir backend

    let mdir_config = MaildirConfig {
        root_dir: mdir_path.clone(),
    };
    let backend_ctx = MaildirContextBuilder::new(config.clone(), mdir_config);
    let backend_builder = BackendBuilder::new(config.clone(), backend_ctx)
        .with_add_folder(|ctx| Some(AddMaildirFolder::new_boxed(ctx.clone())))
        .with_list_folders(|ctx| Some(ListMaildirFolders::new_boxed(ctx.clone())))
        .with_expunge_folder(|ctx| Some(ExpungeMaildirFolder::new_boxed(ctx.clone())))
        .with_delete_folder(|ctx| Some(DeleteFolderMaildir::new_boxed(ctx.clone())))
        .with_list_envelopes(|ctx| Some(ListMaildirEnvelopes::new_boxed(ctx.clone())))
        .with_add_flags(|ctx| Some(AddMaildirFlags::new_boxed(ctx.clone())))
        .with_set_flags(|ctx| Some(SetMaildirFlags::new_boxed(ctx.clone())))
        .with_remove_flags(|ctx| Some(RemoveMaildirFlags::new_boxed(ctx.clone())))
        .with_add_message(|ctx| Some(AddMaildirMessage::new_boxed(ctx.clone())))
        .with_peek_messages(|ctx| Some(PeekMaildirMessages::new_boxed(ctx.clone())))
        .with_copy_messages(|ctx| Some(CopyMaildirMessages::new_boxed(ctx.clone())))
        .with_move_messages(|ctx| Some(MoveMaildirMessages::new_boxed(ctx.clone())));
    let submdir = backend_builder.build().await.unwrap();

    // check that a message can be built and added
    let email = MessageBuilder::new()
        .from("alice@localhost")
        .to("bob@localhost")
        .subject("Plain message!")
        .text_body("Plain message!")
        .write_to_vec()
        .unwrap();
    let id = mdir
        .add_message_with_flag("INBOX", &email, Flag::Seen)
        .await
        .unwrap();

    // check that the added message exists
    let emails = mdir.get_messages("INBOX", &id.into()).await.unwrap();
    let tpl = emails
        .to_vec()
        .first()
        .unwrap()
        .to_read_tpl(&config, |i| i.with_show_only_headers(["From", "To"]))
        .await
        .unwrap();
    let expected_tpl = concat_line!(
        "From: alice@localhost",
        "To: bob@localhost",
        "",
        "Plain message!",
        "",
    );

    assert_eq!(tpl, expected_tpl);

    // check that the envelope of the added message exists
    let envelopes = mdir.list_envelopes("INBOX", 10, 0).await.unwrap();
    let envelope = envelopes.first().unwrap();
    assert_eq!(1, envelopes.len());
    assert_eq!("alice@localhost", envelope.from.addr);
    assert_eq!("Plain message!", envelope.subject);

    // check that a flag can be added to the message
    mdir.add_flag("INBOX", &Id::single(&envelope.id), Flag::Flagged)
        .await
        .unwrap();
    let envelopes = mdir.list_envelopes("INBOX", 1, 0).await.unwrap();
    let envelope = envelopes.first().unwrap();
    assert!(envelope.flags.contains(&Flag::Seen));
    assert!(envelope.flags.contains(&Flag::Flagged));

    // check that the message flags can be changed
    mdir.set_flag("INBOX", &Id::single(&envelope.id), Flag::Answered)
        .await
        .unwrap();
    let envelopes = mdir.list_envelopes("INBOX", 1, 0).await.unwrap();
    let envelope = envelopes.first().unwrap();
    assert!(!envelope.flags.contains(&Flag::Seen));
    assert!(!envelope.flags.contains(&Flag::Flagged));
    assert!(envelope.flags.contains(&Flag::Answered));

    // check that a flag can be removed from the message
    mdir.remove_flag("INBOX", &Id::single(&envelope.id), Flag::Answered)
        .await
        .unwrap();
    let envelopes = mdir.list_envelopes("INBOX", 1, 0).await.unwrap();
    let envelope = envelopes.first().unwrap();
    assert!(!envelope.flags.contains(&Flag::Seen));
    assert!(!envelope.flags.contains(&Flag::Flagged));
    assert!(!envelope.flags.contains(&Flag::Answered));

    // check that the message can be copied
    mdir.copy_messages("INBOX", "subdir", &Id::single(&envelope.id))
        .await
        .unwrap();
    let inbox = mdir.list_envelopes("INBOX", 0, 0).await.unwrap();
    let subdir = mdir.list_envelopes("subdir", 0, 0).await.unwrap();
    let abs_subdir = mdir.list_envelopes("abs-subdir", 0, 0).await.unwrap();
    let trash = mdir.list_envelopes("Trash", 0, 0).await.unwrap();
    assert_eq!(1, inbox.len());
    assert_eq!(1, subdir.len());
    assert_eq!(1, abs_subdir.len());
    assert_eq!(0, trash.len());
    assert!(mdir
        .get_messages("INBOX", &Id::single(&inbox[0].id))
        .await
        .is_ok());
    assert!(mdir
        .get_messages("subdir", &Id::single(&subdir[0].id))
        .await
        .is_ok());
    assert!(submdir
        .get_messages("INBOX", &Id::single(&subdir[0].id))
        .await
        .is_ok());

    // check that the email can be marked as deleted then expunged
    mdir.add_flag("subdir", &Id::single(&subdir[0].id), Flag::Deleted)
        .await
        .unwrap();
    let inbox = mdir.list_envelopes("INBOX", 0, 0).await.unwrap();
    let subdir = mdir.list_envelopes("subdir", 0, 0).await.unwrap();
    let abs_subdir = mdir.list_envelopes("abs-subdir", 0, 0).await.unwrap();
    let trash = mdir.list_envelopes("Trash", 0, 0).await.unwrap();
    assert_eq!(1, inbox.len());
    assert_eq!(1, subdir.len());
    assert_eq!(1, abs_subdir.len());
    assert_eq!(0, trash.len());

    mdir.expunge_folder("subdir").await.unwrap();
    let subdir = mdir.list_envelopes("subdir", 0, 0).await.unwrap();
    let abs_subdir = mdir.list_envelopes("subdir", 0, 0).await.unwrap();
    assert_eq!(0, subdir.len());
    assert_eq!(0, abs_subdir.len());

    // check that the message can be moved
    mdir.move_messages("INBOX", "subdir", &Id::single(&envelope.id))
        .await
        .unwrap();
    let inbox = mdir.list_envelopes("INBOX", 0, 0).await.unwrap();
    let subdir = mdir.list_envelopes("subdir", 0, 0).await.unwrap();
    let trash = mdir.list_envelopes("Trash", 0, 0).await.unwrap();
    assert_eq!(0, inbox.len());
    assert_eq!(1, subdir.len());
    assert_eq!(0, trash.len());

    // check that the message can be deleted
    mdir.delete_messages("subdir", &Id::single(&subdir[0].id))
        .await
        .unwrap();
    let inbox = mdir.list_envelopes("INBOX", 0, 0).await.unwrap();
    let subdir = mdir.list_envelopes("subdir", 0, 0).await.unwrap();
    let trash = mdir.list_envelopes("Trash", 0, 0).await.unwrap();
    assert_eq!(0, inbox.len());
    assert_eq!(0, subdir.len());
    assert_eq!(1, trash.len());

    mdir.delete_messages("Trash", &Id::single(&trash[0].id))
        .await
        .unwrap();
    let trash = mdir.list_envelopes("Trash", 0, 0).await.unwrap();
    assert_eq!(1, trash.len());
    assert!(trash[0].flags.contains(&Flag::Deleted));

    mdir.expunge_folder("Trash").await.unwrap();
    let trash = mdir.list_envelopes("Trash", 0, 0).await.unwrap();
    assert_eq!(0, trash.len());
}
