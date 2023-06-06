use keyring;
use pimalaya_keyring::{Entry, Error};

const KEY: &str = "key";
const VAL: &str = "val";

fn main() {
    env_logger::builder().is_test(true).init();

    // set entry
    let entry = Entry::from(KEY);
    entry.set(VAL).unwrap();

    // get entry
    let val = entry.get().unwrap();
    assert_eq!(val, VAL);

    // delete entry
    entry.delete().unwrap();
    match entry.get() {
        Err(Error::GetSecretError(keyring::Error::NoEntry, key)) if key == KEY => (),
        res => panic!("unexpected result {res:?}"),
    }
}
