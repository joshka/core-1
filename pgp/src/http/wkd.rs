//! Module dedicated to Web Key Directory.
//!
//! Since HKP is just HTTP, this module only contains a function that
//! formats a given URI to match [HKP specs].
//!
//! A [Web Key Directory] is a Web service that can be queried with
//! email addresses to obtain the associated OpenPGP keys.
//!
//! This module has been heavily inspired by the great work from the
//! [sequoia] team.
//!
//! [Web Key Directory]: https://datatracker.ietf.org/doc/html/draft-koch-openpgp-webkey-service
//! [sequoia]: https://gitlab.com/sequoia-pgp/sequoia

use std::fmt;

use async_recursion::async_recursion;
use futures::{stream, StreamExt};
use http_body_util::BodyExt;
use hyper::{body::Incoming, http::Response, Uri};
use native::{Deserializable, SignedPublicKey};
use sha1::{Digest, Sha1};
use tokio::task;
use tracing::debug;

use crate::{Error, Result};

use super::{new_http_client, HttpClient};

struct EmailAddress {
    pub local_part: String,
    pub domain: String,
}

impl EmailAddress {
    /// Returns an EmailAddress from an email address string.
    ///
    /// From [draft-koch]:
    ///
    ///```text
    /// To help with the common pattern of using capitalized names
    /// (e.g. "Joe.Doe@example.org") for mail addresses, and under the
    /// premise that almost all MTAs treat the local-part case-insensitive
    /// and that the domain-part is required to be compared
    /// case-insensitive anyway, all upper-case ASCII characters in a User
    /// ID are mapped to lowercase.  Non-ASCII characters are not changed.
    ///```
    pub fn from(email_address: impl AsRef<str>) -> Result<Self> {
        // Ensure that is a valid email address by parsing it and
        // return the errors that it returns. This is also done in
        // hagrid.
        let email_address = email_address.as_ref();
        let v: Vec<&str> = email_address.split('@').collect();
        if v.len() != 2 {
            return Err(Error::ParseEmailAddressError(email_address.into()));
        };

        // Convert domain to lowercase without tailoring, i.e. without
        // taking any locale into account.
        // See <https://doc.rust-lang.org/std/primitive.str.html#method.to_lowercase>.
        //
        // Keep the local part as-is as we'll need that to generate WKD URLs.
        let email = EmailAddress {
            local_part: v[0].to_string(),
            domain: v[1].to_lowercase(),
        };

        Ok(email)
    }
}

/// WKD variants.
///
/// There are two variants of the URL scheme. `Advanced` should be
/// preferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Variant {
    /// Advanced variant.
    ///
    /// This method uses a separate subdomain and is more flexible.
    /// This method should be preferred.
    #[default]
    Advanced,

    /// Direct variant.
    ///
    /// This method is deprecated.
    Direct,
}

/// Stores the parts needed to create a Web Key Directory URL.
///
/// NOTE: This is a different `Url` than [`url::Url`] (`url` crate) that is
/// actually returned with the method [to_url](Url::to_url())
#[derive(Debug, Clone)]
struct Url {
    domain: String,
    local_encoded: String,
    local_part: String,
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.build(None))
    }
}

impl Url {
    /// Returns a [`Url`] from an email address string.
    pub fn from(email_address: impl AsRef<str>) -> Result<Self> {
        let email = EmailAddress::from(email_address)?;
        let local_encoded = encode_local_part(email.local_part.to_lowercase());
        let url = Url {
            domain: email.domain,
            local_encoded,
            local_part: email.local_part,
        };
        Ok(url)
    }

    /// Returns an URL string from a [`Url`].
    pub fn build<V>(&self, variant: V) -> String
    where
        V: Into<Option<Variant>>,
    {
        let variant = variant.into().unwrap_or_default();
        if variant == Variant::Direct {
            format!(
                "https://{}/.well-known/openpgpkey/hu/{}?l={}",
                self.domain, self.local_encoded, self.local_part
            )
        } else {
            format!(
                "https://openpgpkey.{}/.well-known/openpgpkey/{}/hu/{}\
                    ?l={}",
                self.domain, self.domain, self.local_encoded, self.local_part
            )
        }
    }

    /// Returns an [`hyper::Uri`].
    pub fn to_uri<V>(&self, variant: V) -> Result<Uri>
    where
        V: Into<Option<Variant>>,
    {
        let url_string = self.build(variant);
        let uri = url_string
            .as_str()
            .parse::<Uri>()
            .map_err(|err| Error::ParseUriError(err, url_string.clone()))?;
        Ok(uri)
    }
}

/// Returns a 32 characters string from the local part of an email address
///
/// From [draft-koch]:
///     The so mapped local-part is hashed using the SHA-1 algorithm. The
///     resulting 160 bit digest is encoded using the Z-Base-32 method as
///     described in RFC6189, section 5.1.6. The resulting string has a
///     fixed length of 32 octets.
fn encode_local_part<S: AsRef<str>>(local_part: S) -> String {
    let local_part = local_part.as_ref();

    let mut hasher = Sha1::new();
    hasher.update(local_part.as_bytes());
    let digest = hasher.finalize();

    // After z-base-32 encoding 20 bytes, it will be 32 bytes long.
    zbase32::encode(&digest[..])
}

#[async_recursion]
async fn get_following_redirects(
    client: &HttpClient,
    url: Uri,
    depth: i32,
) -> Result<Response<Incoming>> {
    let response = client.get(url).await;

    if depth < 0 {
        return Err(Error::RedirectOverflowError);
    }

    if let Ok(ref resp) = response {
        if resp.status().is_redirection() {
            let url = resp
                .headers()
                .get("Location")
                .and_then(|value| value.to_str().ok())
                .map(|value| value.parse::<Uri>());
            if let Some(Ok(url)) = url {
                return get_following_redirects(client, url, depth - 1).await;
            }
        }
    }

    response.map_err(Error::ParseResponseError)
}

/// Retrieves the Certs that contain userids with a given email
/// address from a Web Key Directory URL.
///
/// From [draft-koch]:
///
/// ```text
/// There are two variants on how to form the request URI: The
/// advanced and the direct method. Implementations MUST first try the
/// advanced method. Only if the required sub-domain does not exist,
/// they SHOULD fall back to the direct method.
///
/// […]
///
/// The HTTP GET method MUST return the binary representation of the
/// OpenPGP key for the given mail address.
///
/// […]
///
/// Note that the key may be revoked or expired - it is up to the
/// client to handle such conditions. To ease distribution of revoked
/// keys, a server may return revoked keys in addition to a new key.
/// The keys are returned by a single request as concatenated key
/// blocks.
/// ```
///
/// [draft-koch]: https://datatracker.ietf.org/doc/html/draft-koch-openpgp-webkey-service/#section-3.1
async fn get(client: &HttpClient, email: &String) -> Result<SignedPublicKey> {
    // First, prepare URIs and client.
    let wkd_url = Url::from(email)?;
    let uri = wkd_url.to_uri(Variant::Advanced)?;

    const REDIRECT_LIMIT: i32 = 10;

    // First, try the Advanced Method.
    let res = match get_following_redirects(client, uri.clone(), REDIRECT_LIMIT).await {
        Ok(res) => Ok(res),
        Err(_) => {
            let uri = wkd_url.to_uri(Variant::Direct)?;
            get_following_redirects(client, uri.clone(), REDIRECT_LIMIT).await
        }
    }?;

    let status = res.status();
    let body = res
        .into_body()
        .collect()
        .await
        .map_err(Error::ParseBodyError)?
        .to_bytes();

    if !status.is_success() {
        let err = String::from_utf8_lossy(&body).to_string();
        return Err(Error::GetPublicKeyError(uri, status, err));
    }

    let pkey = SignedPublicKey::from_bytes(&*body).map_err(Error::ParseCertError)?;

    Ok(pkey)
}

/// Gets the public key associated to the given email.
pub async fn get_one(email: String) -> Result<SignedPublicKey> {
    let client = new_http_client()?;
    self::get(&client, &email).await
}

/// Gets public keys associated to the given emails.
pub async fn get_all(emails: Vec<String>) -> Result<Vec<(String, Result<SignedPublicKey>)>> {
    let client = new_http_client()?;

    let pkeys = stream::iter(emails)
        .map(|email| {
            let client = client.clone();
            task::spawn(async move { (email.clone(), self::get(&client, &email).await) })
        })
        .buffer_unordered(8)
        .filter_map(|res| async {
            match res {
                Ok(res) => Some(res),
                Err(err) => {
                    debug!("cannot join async task: {err:?}");
                    None
                }
            }
        })
        .collect()
        .await;

    Ok(pkeys)
}